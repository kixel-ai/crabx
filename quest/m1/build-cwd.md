# [XS] Isolate working directory for --build and background --refresh

## Goal

Running `crabx --build` or an unpinned `crabx` invocation inside an untrusted directory never reads that directory's `.cargo/config.toml` and never holds that directory open in a detached refresh process.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#758` (runner).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ ship this in `m1` for [0.2.0](/quest/m1/release-0.2.md), requiring [0.1.1](/quest/m0/release-0.1.1.md) so `0.1.1` stays musl-only.

Changes to make:

- In `src/install.rs` (`build()`), set `command.current_dir(stage)` before running `cargo install` so an untrusted working directory's `.cargo/config.toml` (such as `[build] rustc-wrapper`, `[target.*.linker]`, or `[source.crates-io] replace-with`) cannot hijack the build or poison crabx's global `build` cache.
- In `src/network.rs` (`spawn_refresh()`), set `command.current_dir(&cache.root)` (passing `&cache.root` into `spawn_refresh`) so the detached updater does not lock the caller's working directory on Windows/mounts or inherit an untrusted `.cargo/config.toml` during background `--build` refreshes.
- In `src/install.rs`, call `let _ = stage.keep();` after `fs::rename(stage.path(), &destination)?` succeeds, avoiding a redundant `ENOENT` cleanup on drop.
- Add a test in `tests/execute.rs` verifying that `crabx --build` does not run `cargo` in the caller's working directory (e.g., checking `std::env::current_dir()` in the fake `CARGO` helper).

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
