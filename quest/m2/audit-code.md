# [M] Audit the code

## Goal

Run the `code` audit: `quest audit code` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

Baseline from the m1 run, [kixel-ai/crabx#8](https://github.com/kixel-ai/crabx/pull/8): about 2,500 lines of Rust in `src/` and `tests/`, 1,100 of Python (`release.py`, `pipeline/`), the npm launcher, two installers, and seven workflows.
`cargo clippy -D warnings` catches dead Rust in `src/`, since it is a binary crate; `ruff check` catches unused Python imports and names, not unused functions or return values, so search those by hand.

Excluded, kept on purpose:

- `vendor/`: imported code keeps its upstream surface. `#[allow(dead_code)]` on `mod fetchers` in `src/main.rs` covers it; dropping it warns only inside `vendor/`, such as quickinstall's unused stats `report`.
- `trust.rs`'s `deprecated_github` claims: Fulcio's legacy OIDs, read only when a certificate lacks the current ones. A trust decision, not a deprecated API.
- The job-identity step copied into `crabx.yml`'s two jobs and `crabx-build.yml`: a reusable workflow can't share a local composite action without pinning its own ref.
- The target lists in `pipeline.py`, `release.py`, and `npm/crabx.js`: each runtime needs its own, and `test_release.py` checks they agree.
- `cache::Record.source`: no code reads it, but it records the trust result in each cache entry.
- `just run` and `just fix`: entry points for people.

Check first: whatever landed since the m1 run.

## Audit

- code

## Required

- [m1](/quest/m1/README.md) - the audit runs once per milestone

## Scheduled

- 2026-11-30 - a week after the m1 run, so a fast milestone doesn't rerun it at once
