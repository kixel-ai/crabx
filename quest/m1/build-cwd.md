# [XS] Isolate working directory for --build and background --refresh

## Goal

Running `crabx --build` or an unpinned `crabx` invocation inside an untrusted checkout never loads that checkout's `.cargo/config.toml` or `.cargo/config` (even when launched from a nested subdirectory, when `XDG_CACHE_HOME` or `TMPDIR` points inside the checkout, or when a container mounts a checkout at `/`) and never holds the caller's directory open in a detached refresh process.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#758` (runner).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ ship this in `m1` for [0.2.0](/quest/m1/release-0.2.md), requiring [0.1.1](/quest/m0/release-0.1.1.md) so `0.1.1` stays musl-only.

Changes to make:

- Keep `stage` as `cargo install`'s `--root` destination, and run both `cargo install` (`src/install.rs` `build()`) and the detached `--refresh` process (`src/network.rs` `spawn_refresh()`) from a trusted working directory where the candidate directory **itself and every one of its canonical ancestors** are free of untrusted Cargo configuration:
  - Check both the candidate directory itself and all of its canonical ancestors (including when the candidate is `/` on Unix or a drive root on Windows, because Cargo checks `.cargo/config.toml` and `.cargo/config` in its working directory itself).
  - Reject any candidate whose self or ancestor chain contains `.cargo/config.toml` or `.cargo/config` (outside the user's `CARGO_HOME` / `$HOME/.cargo`) or shares a workspace/git checkout root with the caller's working directory (for example, `checkout/tools` when `TMPDIR` is `checkout/tmp`). Try a dedicated empty temp dir and the filesystem root against this same check, and fail closed with a clear error if no clean working directory exists.
  - Apply the same self-and-ancestor-checked working directory selection to background `--refresh` (including `--refresh --build`).
- In `src/install.rs`, call `let _ = stage.keep();` after `fs::rename(stage.path(), &destination)?` succeeds, avoiding a redundant `ENOENT` cleanup on drop.
- Add tests in `tests/execute.rs` (and unit tests for candidate-directory validation) covering:
  - foreground `crabx --build` and background `--refresh` launched from `checkout/tools` with `XDG_CACHE_HOME=checkout/cache`, `TMPDIR=checkout/tmp`, and `checkout/.cargo/config.toml`, proving Cargo runs outside `checkout` and never reads `checkout/.cargo/config.toml`, and
  - rejecting a candidate directory (including a root-like candidate) that directly contains `.cargo/config.toml` or `.cargo/config`, failing closed when no clean directory is available.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
