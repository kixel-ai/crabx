# [M] Audit dependencies

## Goal

Run the `deps` audit: `quest audit deps` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

- Baseline: the last run covered through 2026-11-23; look for releases after it.
- Dependabot tracks Cargo, rust-toolchain.toml, flake.lock, GitHub Actions (including `.github/actions/*`), and the pipeline's pip pins in `pipeline/requirements.txt`, each with a 7-day cooldown, so most bumps arrive as its PRs. Spend the run on what it cannot see: release notes, the binstalk-fetchers fork in `vendor/`, the setup-python and setup-node versions in the workflows, and the runner images in `pipeline/pipeline.py`'s `TARGETS`.
- Check first: the fork's drift from cargo-bins/cargo-binstall, compared by commit from the revision `vendor/README.md` names, and whether upstream has taken its resolver-capacity fix.

## Audit

- deps

## Required

- [m1](/quest/m1/README.md) - the audit runs once per milestone

## Scheduled

- 2026-11-30 - a week after the last run
