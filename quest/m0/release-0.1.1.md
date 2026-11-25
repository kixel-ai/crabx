# [S] Release crabx 0.1.1 before npm's trusted publishers expire

## Goal

crabx 0.1.1, with the [musl fallback](/quest/m0/musl-fallback.md), is on crates.io and npm,
published by `release.yml` before 2026-11-27 00:18 UTC, so npm validates the trusted
publishers added on 2026-11-25 and `npx @kixel/crabx token-max --version` runs token-max's
musl build on glibc Linux.

## Plan

Decided 2026-11-25 (maintainer, /quest-plan in kixel-ai/token-max): ✅ ship 0.1.1 with just
the musl fallback before the deadline (rejected: one 0.2.0 release, which may miss it and
need the seven publishers re-added with new 2FA codes). A validated publisher doesn't
expire, so [0.2.0](/quest/m1/release-0.2.md) then publishes through the same setup.

The version PR bumps `version` in `Cargo.toml` from `main` once the fallback has landed.
After `release.yml` runs, check that its npm job published 0.1.1, that each package's
npmjs.com settings page no longer says "Pending validation", and that
`npx @kixel/crabx@0.1.1 token-max@0.0.1 --version` prints token-max's version. If the
deadline passes first, follow [first publish](/quest/m0/first-publish.md)'s `Waiting`.

## Scheduled

- 2026-11-25 - the deadline is 2026-11-27 00:18 UTC

## Required

- [Musl fallback](/quest/m0/musl-fallback.md) - the change this release ships
