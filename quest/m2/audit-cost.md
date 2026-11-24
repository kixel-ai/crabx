# [L] Audit build and CI cost

## Goal

Run the `cost` audit: `quest audit cost` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

Baseline, from the run in [kixel-ai/crabx#7](https://github.com/kixel-ai/crabx/pull/7).
CI figures are GitHub-hosted runners' own job and step times; local ones are `fedora` (24 CPUs) at a 1-minute load of 20 to 34, so compare their ratios, not their seconds.

- The gate, `check` on `ubuntu-24.04`: 142 s median over the 12 runs up to `f1d6151` (3 on slow runners took 195 to 203 s), as 13 s setup, 42 s dev shell, 42 s clippy, 35 s test build, 1.5 s for all 68 tests, and 5 s the runner spent waiting on a `sleep` a test leaked. After that run it is 136 s cold (`17eb692`) and 72 s on a cache hit (`9701f4a`, before the leak's 5 s went): dev shell 40 s, clippy 3 s, test build 4 s. The cache is 506 MB, saved only by `main`, keyed on `Cargo.lock`, `rust-toolchain.toml`, and `flake.lock`.
- Crabx fixture, on pull requests touching `pipeline/**` or `crabx*.yml`: 253 s of runner time over 7 jobs, 136 s of it on the two Windows runners and 63 s on the two macOS ones.
- Pullfrog: 127 to 148 s a review, 68 s of it putting the dev shell on `PATH`.
- Release: its plan job fails in 4 to 12 s on every push and daily until [the first publish](/quest/m0/first-publish.md); no release has run, so its build and publish jobs have no timings yet.
- Tests, one `cargo test --locked` (dev profile, 24 threads): 28 tests in 0.32 s, the slowest 0.23 s; `test_release.py` 29 tests in 1.2 s; `pipeline/test_pipeline.py` 11 tests in 0.14 s.
- Build, at `f1d6151`: cold clippy 21 s, then the test build 15 s; after an edit to `src/cache.rs`, `cargo build` 1.8 s, clippy 7.7 s, the test build 8.5 s. A cold release build is 32 to 37 s, most of its CPU time in aws-lc-sys (18 s), jiff (14 s), and zstd-sys (12 s).
- Size, at `f1d6151`, x86_64-unknown-linux-gnu from the dev shell's toolchain, release profile, stripped: `crabx` 15,032,816 bytes, 6,131,798 as a `.tgz`; aws-lc-sys (1.3 MiB of 9.9 MiB of `.text`) and std (1.0 MiB) lead. The crate is 122,969 bytes, `npm/crabx.js` 3,880, `install.sh` 3,074, `install.ps1` 3,155. No release exists, so no shipped archive was measured.

Check first:

- Whether a release has run: time `release.yml`'s `check` and `binaries` jobs, which build all six targets twice so crates.io never holds a version whose binaries fail, and measure the archives it shipped against the size above.
- Whether the gate's cache still hits: a run whose `Cargo.lock` matches `main`'s restores it in about 7 s. A pull request that changes the lockfile, as Dependabot's Cargo bumps do, runs cold, and each one that lands saves another 506 MB against the repository's 10 GB.

Tried and rejected:

- `lto = "thin"`: no smaller (15,082,616 bytes) and no faster to build.
- A cache for release builds: an attested binary must not be built from a cache another run wrote.
- Caching `/nix/store` for the dev shell's 40 s: restoring gigabytes costs about what substituting them does, and it adds a third-party action to the gate.
- Skipping the Rust steps for changes that can't affect them: on a cache hit they are 7 s.
- A `restore-keys` fallback for the cargo cache: `main` would save stale artifacts with each new lockfile.

## Audit

- cost

## Scheduled

- 2026-11-30 - a week after the last run

## Required

- [m1](/quest/m1/README.md) - the milestone the last run sat in, so the audit runs once per milestone
