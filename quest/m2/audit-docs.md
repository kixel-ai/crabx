# [M] Audit the docs

## Goal

Run the `docs` audit: `quest audit docs` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

- Baseline: `f1d6151`, the `main` commit the m1 run read. Start from `git diff f1d6151 -- . ':!quest'` and read what changed claim by claim.
- Out of scope: `quest/`, which the tree audit owns; the imported upstream code and `CHANGELOG.md` under `vendor/binstalk-fetchers/`, though `vendor/README.md`'s list of local changes is in scope; and the quest-installed skills under `.claude/` and `.agents/`.
- Check first: `README.md` and `pipeline/README.md` against `src/`, `pipeline/pipeline.py`, and the workflows, since both restate behavior in prose; and the deployed installers at `https://crabx.kixel.ai/install.sh` and `/install.ps1` against `main`'s copies.
- crabx is a binary crate, so it has no exported items for a missing-docs lint to check.
- Until crabx's first publish, `npx crabx`, `bunx crabx`, and `cargo install crabx` describe the intended state; once it lands, check them end to end.

## Audit

- docs

## Required

- [m1](/quest/m1/README.md) - runs once per milestone, after m1's work

## Scheduled

- 2026-11-30 - a week after the m1 run, so a milestone that closes fast does not rerun it at once
