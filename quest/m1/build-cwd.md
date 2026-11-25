# [XS] Isolate working directory and Cargo home for --build and background --refresh

## Goal

Running `crabx --build` or an unpinned `crabx` invocation inside an untrusted checkout never loads untrusted `.cargo/config.toml` or `.cargo/config` files (whether through the working directory, its ancestors, `/` in a container, `XDG_CACHE_HOME`, `TMPDIR`, or `CARGO_HOME` and its configuration ancestors), preserves valid relative `CARGO_HOME` paths when switching directories, isolates newly built entries under `crabx/v2/` (never executing legacy `crabx/v1/build/` entries built before isolation), and never holds the caller's directory open in a detached refresh process.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#758` (runner).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ ship this in `m1` for [0.2.0](/quest/m1/release-0.2.md), requiring [0.1.1](/quest/m0/release-0.1.1.md) so `0.1.1` stays musl-only.

Changes to make:

- Keep `stage` as `cargo install`'s `--root` destination, and run both `cargo install` (`src/install.rs` `build()`) and the detached `--refresh` process (`src/network.rs` `spawn_refresh()`) from a trusted working directory where the candidate directory **itself and every one of its canonical ancestors** are free of untrusted Cargo configuration:
  - Check both the candidate directory itself and all of its canonical ancestors (including when the candidate is `/` on Unix or a drive root on Windows, because Cargo checks `.cargo/config.toml` and `.cargo/config` in its working directory itself).
  - Reject any candidate whose self or ancestor chain contains `.cargo/config.toml` or `.cargo/config` (outside the user's trusted `CARGO_HOME`) or shares a workspace/git checkout root with the caller's working directory (for example, `checkout/tools` when `TMPDIR` is `checkout/tmp`). Try a dedicated empty temp dir and the filesystem root against this same check, and fail closed with a clear error if no clean working directory exists.
  - Resolve and validate the effective `CARGO_HOME` (and its configuration search ancestors) against the caller's **original** working directory before changing `current_dir` for both foreground `--build` and detached `--refresh --build`:
    - If `CARGO_HOME` is relative (e.g., `../trusted-cargo-home`), resolve it relative to the caller's original `current_dir`, validate it, and pass that validated **absolute** `CARGO_HOME` explicitly via `.env("CARGO_HOME", ...)` to both `cargo install` and detached `--refresh` so changing `current_dir` never redirects Cargo to the wrong home or drops the caller's registry configuration.
    - Reject the resolved `CARGO_HOME` if it lies inside the caller's checkout/workspace (e.g., `CARGO_HOME=/path/to/checkout/.cargo`) OR if any ancestor of the resolved `CARGO_HOME` shared with the caller's checkout (e.g., `/work/cargo-home` when the caller is in `/work/checkout` and `/work/.cargo/config.toml` exists) introduces untrusted Cargo configuration.
  - Resolve any relative `CARGO` executable path (a `CARGO` value containing a path separator, such as `./bin/cargo`, rather than a bare command name resolved via `PATH`) against the caller's original working directory before changing `current_dir` in `src/install.rs` `build()` and before spawning detached `--refresh` in `src/network.rs` `spawn_refresh()` (passing the resolved absolute path via `.env("CARGO", ...)` to `--refresh`).
- Move the `build` cache mode from `crabx/v1/<host>/build/` to `crabx/v2/<host>/build/` atomically in this change (independent of [attestation-binding](/quest/m1/attestation-binding.md)'s cutover of `author` and `quickinstall`, so neither order allows a pre-isolation runner to populate `crabx/v2/<host>/build/`). Preserve legacy `crabx/v1/<host>/build/` for rollback and update `build` cache path assertions in `tests/execute.rs` and `README.md`.
- In `src/install.rs`, call `let _ = stage.keep();` after `fs::rename(stage.path(), &destination)?` succeeds, avoiding a redundant `ENOENT` cleanup on drop.
- Add tests in `tests/execute.rs` (and unit tests for candidate-directory and `CARGO_HOME` validation) covering:
  - foreground `crabx --build` and background `--refresh` launched from `checkout/tools` with `XDG_CACHE_HOME=checkout/cache`, `TMPDIR=checkout/tmp`, and `checkout/.cargo/config.toml`, proving Cargo runs outside `checkout` and never reads `checkout/.cargo/config.toml`,
  - preserving a trusted relative `CARGO_HOME` (e.g., `../trusted-home`) and a relative `CARGO` executable path (e.g., `./bin/cargo`) as absolute paths in both `cargo install` and detached `--refresh`,
  - rejecting a checkout-controlled `CARGO_HOME` (both absolute and relative) AND a `CARGO_HOME` whose shared ancestor holds untrusted `.cargo/config.toml` (e.g., `/work/checkout` + `/work/cargo-home` + `/work/.cargo/config.toml`) on both foreground `--build` and detached `--refresh`,
  - rejecting a candidate directory (including a root-like candidate) that directly contains `.cargo/config.toml` or `.cargo/config`, failing closed when no clean directory is available, and
  - confirming upgraded `--build` ignores legacy `crabx/v1/` `build/` entries.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
