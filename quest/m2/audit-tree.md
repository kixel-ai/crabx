# [M] Audit the quest tree

## Goal

Run the `tree` audit: `quest audit tree` prints what it checks, what each finding becomes, and how to plan its next run.

## Plan

- Baseline: the m1 run (2026-11-23) found the tree small enough to read in one pass, without sub-agents.
- Check first that each follow-up a PR assigns to another repository's quest tree exists there; the moq one from kixel-ai/rust2#719 never did.
- Read the quest files open PRs add, from `quest pr list`, alongside `main`'s.

## Audit

- tree

## Scheduled

- 2026-11-30

## Required

- [m1](/quest/m1/README.md) - the audit reruns once per milestone
