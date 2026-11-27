# [M] Audit test coverage

## Goal

Run the `tests` audit: `quest audit tests` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

- Baseline, measured on the m1 run's PR head with `cargo llvm-cov --locked --summary-only` (cargo-llvm-cov 0.9.2 and nixpkgs' `llvmPackages_23` for `LLVM_COV` and `LLVM_PROFDATA`, run inside the dev shell, nothing committed) and `coverage run --branch` on each Python suite. Lines: `cache.rs` 96%, `cli.rs` 98%, `init.rs` 87%, `install.rs` 47%, `main.rs` 72%, `network.rs` 82%, `trust.rs` 85%, Rust total 51% with the vendored fetchers; `release.py` 79%, `pipeline/pipeline.py` 82%.
- The tool cannot see a crabx process that execs its binary, since exec drops its counters: `main.rs`'s run-and-exec path and `network::spawn_refresh` read as uncovered although `tests/execute.rs` runs them.
- The author-release path (`install::fetch`, `Package::install`, `trust::verify`'s request loop, and the vendored fetchers), `install.ps1`, and the Windows `execute` path run only in `.github/workflows/e2e.yml`'s nightly live runs, which also run `cargo test` on macOS and Windows; `check` runs only Linux's offline tests, after each push to `main`. Check what those runs miss rather than adding offline fakes: the maintainer chose live runs over fake servers in the gate.
- Out of scope: the imported binstalk-fetchers code beyond the local changes `vendor/README.md` lists, which upstream's tests cover.

## Audit

- tests

## Required

- [m1](/quest/m1/README.md) - runs once per milestone, after m1's work

## Scheduled

- 2026-11-30 - a week after the m1 run, so a milestone that closes fast does not rerun it at once
