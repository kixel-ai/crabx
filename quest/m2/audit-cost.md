# [L] Audit build and CI cost

## Goal

Run the `cost` audit: `quest audit cost` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

Baseline, from the run in [kixel-ai/crabx#7](https://github.com/kixel-ai/crabx/pull/7).
CI figures are GitHub-hosted runners' own job and step times; local ones are `fedora` (24 CPUs) at a 1-minute load of 20 to 34, so compare their ratios, not their seconds.

- The gate, `check` on `ubuntu-24.04`: 142 s median over the 12 runs up to `f1d6151` (3 on slow runners took 195 to 203 s), as 13 s setup, 42 s dev shell, 42 s clippy, 35 s test build, 1.5 s for all 68 tests, and 5 s the runner spent waiting on a `sleep` a test leaked. After that run it is 136 s cold (`17eb692`) and 72 s on a cache hit (`9701f4a`, before the leak's 5 s went): dev shell 40 s, clippy 3 s, test build 4 s. The cache is 506 MB, saved only by `main`, keyed on `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, and `flake.lock`.
- Crabx fixture, on pull requests touching `pipeline/**` or `crabx*.yml`: 253 s of runner time over 7 jobs, 136 s of it on the two Windows runners and 63 s on the two macOS ones.
- Release: its plan job fails in 4 to 12 s on every push and daily until the first publish; no release had run then, so its build and publish jobs have no timings yet.
- Tests, one `cargo nextest run --locked` (dev profile, 24 threads): 28 tests in 0.32 s, the slowest 0.23 s; `test_release.py` 29 tests in 1.2 s; `pipeline/test_pipeline.py` 11 tests in 0.14 s.
- Build, at `f1d6151`: cold clippy 21 s, then the test build 15 s; after an edit to `src/cache.rs`, `cargo build` 1.8 s, clippy 7.7 s, the test build 8.5 s. A cold release build was 32 to 37 s, most of its CPU time in aws-lc-sys (18 s), jiff (14 s), and zstd-sys (12 s); with the release profile's `lto = "fat"` and `codegen-units = 1` it is 90 s.
- Size, x86_64-unknown-linux-gnu built by `pipeline.py build` (zigbuild against glibc 2.28), stripped: `crabx` 11,495,336 bytes, 5,025,359 as the `.tgz` it archives. The profile before `lto = "fat"` and `codegen-units = 1` gave 14,947,968 and 6,031,554 the same way, with aws-lc-sys (1.3 MiB of 9.9 MiB of `.text`) and std (1.0 MiB) leading. The crate is 122,969 bytes, `npm/crabx.js` 3,880, `install.sh` 3,074, `install.ps1` 3,155. No release exists, so no shipped archive was measured.

Check first:

- Whether a release has run: time `release.yml`'s `check` and `binaries` jobs, which build all six targets twice so crates.io never holds a version whose binaries fail, and measure the archives it shipped against the size above.
- Whether the gate's cache still hits: a run whose manifest and lockfile match `main`'s restores it in about 7 s. A pull request that changes the manifest or the lockfile, as Dependabot's Cargo bumps do, runs cold, and each one that lands saves another 506 MB against the repository's 10 GB.

Tried and rejected:

- `lto = "thin"`: no smaller (15,082,616 bytes) and no faster to build.
- `opt-level = "s"` on top of fat LTO: smaller still (7.9 MB, 3.7 MB as a `.tgz`), but it trades runtime, and the maintainer chose build time as the only cost.
- Requiring the Crabx fixture on every pull request: it stays path-filtered and optional, since `quest merge` already holds a head it failed on.
- A cache for release builds: an attested binary must not be built from a cache another run wrote.
- Caching `/nix/store` for the dev shell's 40 s: restoring gigabytes costs about what substituting them does, and it adds a third-party action to the gate.
- Skipping the Rust steps for changes that can't affect them: on a cache hit they are 7 s.
- Dropping crabx's own artifacts from the cache with `cargo clean -p crabx` before the save: they are 138 MiB of 949 MiB of `target`, a tenth of it compressed, for one more step.
- A `restore-keys` fallback for the cargo cache: `main` would save stale artifacts with each new lockfile.

## Audit

- cost

## Scheduled

- 2026-11-30 - a week after the last run

## Required

- [m1](/quest/m1/README.md) - the milestone the last run sat in, so the audit runs once per milestone
