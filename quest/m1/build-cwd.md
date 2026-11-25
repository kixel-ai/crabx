# [XS] Isolate working directory for --build and background --refresh

## Goal

Running `crabx --build` or an unpinned `crabx` invocation inside an untrusted checkout never loads that checkout's `.cargo/config.toml` (even when launched from a nested subdirectory or when `XDG_CACHE_HOME` or `TMPDIR` points inside the checkout) and never holds the caller's directory open in a detached refresh process.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#758` (runner).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ ship this in `m1` for [0.2.0](/quest/m1/release-0.2.md), requiring [0.1.1](/quest/m0/release-0.1.1.md) so `0.1.1` stays musl-only.

Changes to make:

- Keep `stage` as `cargo install`'s `--root` destination, and run both `cargo install` (`src/install.rs` `build()`) and the detached `--refresh` process (`src/network.rs` `spawn_refresh()`) from a trusted working directory whose **entire canonical ancestor chain** is free of untrusted Cargo configuration:
  - On Unix, `/` has no parent directories at all (so Cargo reads only `$CARGO_HOME/config.toml` and never walks through any workspace or checkout). On Windows (or whenever selecting a temporary directory), walk every canonical ancestor of the candidate directory and reject any candidate that has a `.cargo/config.toml` or `.cargo/config` (outside the user's `CARGO_HOME` / `$HOME/.cargo`) or shares a workspace/git checkout root with the caller's working directory (for example, `checkout/tools` when `TMPDIR` is `checkout/tmp`), falling back to the system root directory (or failing closed if no clean directory is available).
  - Apply the same ancestor-checked working directory selection to background `--refresh` (including `--refresh --build`).
- In `src/install.rs`, call `let _ = stage.keep();` after `fs::rename(stage.path(), &destination)?` succeeds, avoiding a redundant `ENOENT` cleanup on drop.
- Add tests in `tests/execute.rs` covering both foreground `crabx --build` and background `--refresh` launched from a nested subdirectory `checkout/tools` with `XDG_CACHE_HOME=checkout/cache`, `TMPDIR=checkout/tmp`, and a hostile `checkout/.cargo/config.toml`, proving Cargo runs from a directory whose ancestor chain does not contain `checkout` and never reads `checkout/.cargo/config.toml`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
