# [M] A GitHub App that sets up crabx's pipeline

## Goal

Installing the App on a repository opens the PR that `crabx init` would make, so a
maintainer opts in with one click and one merge.

## Plan

Decided in [the quest tree's plan](/quest/README.md#plan): ✅ the App comes after the
workflow and `crabx init`, in rust2's m3, now this tree's m1. It needs a small webhook server and secrets, the only
hosting this project would run.

The PR adds what `crabx init` writes: [`src/binaries.yml`](/src/binaries.yml)
with the repository's default branch filled in.

Decided 2026-11-18 (maintainer, /quest-run): ✅ wait for crabx's first publish, since nothing
can install what the App sets up until then (rejected: start now; delete the App).

Decided 2026-11-26 (maintainer, /quest-run): crabx has published (0.1.0, then 0.2.x), so that
wait has cleared. ✅ settle where the webhook server is hosted and who registers the App and
holds its secrets with `/quest-plan` before an agent starts (rejected: start now and leave
hosting as Waiting decisions; delete the App).
