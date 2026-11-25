# [XS] Isolate working directory for --build and background --refresh

## Goal

Running `crabx --build` or an unpinned `crabx` invocation inside an untrusted directory never loads that directory's `.cargo/config.toml` (even when `XDG_CACHE_HOME` points inside the checkout) and never holds the caller's directory open in a detached refresh process.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#758` (runner).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ ship this in `m1` for [0.2.0](/quest/m1/release-0.2.md), requiring [0.1.1](/quest/m0/release-0.1.1.md) so `0.1.1` stays musl-only.

Changes to make:

- Keep `stage` as `cargo install`'s `--root` destination, and run both `cargo install` (`src/install.rs` `build()`) and the detached `--refresh` process (`src/network.rs` `spawn_refresh()`) with a trusted working directory guaranteed to sit outside the caller's checkout ancestry (not `stage` or `cache.root`, which still have the checkout as an ancestor whenever `XDG_CACHE_HOME` points inside the workspace):
  - Create or select an isolated directory outside the caller's `current_dir` ancestry (for example, a dedicated empty directory under the OS temp dir when outside the caller's cwd, falling back to `/` on Unix or system drive root on Windows so Cargo's upward `.cargo/config.toml` search never walks through an untrusted checkout).
  - Verify the same isolation holds for background `--refresh --build` runs.
- In `src/install.rs`, call `let _ = stage.keep();` after `fs::rename(stage.path(), &destination)?` succeeds, avoiding a redundant `ENOENT` cleanup on drop.
- Add tests in `tests/execute.rs` covering both foreground `crabx --build` and background `--refresh` with `XDG_CACHE_HOME` pointed inside a temporary checkout containing a hostile `.cargo/config.toml`, proving Cargo runs outside that checkout's ancestry and never reads the checkout's config.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
