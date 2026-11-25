# [S] Release crabx 0.2.0

## Goal

crabx 0.2.0, with the musl fallback, `crabx install` and the release-age
delay, is on crates.io and on npm as `@kixel/crabx`, published by
`release.yml`, so `npx @kixel/crabx@latest token-max --version` runs
token-max on glibc Linux and `npx @kixel/crabx@latest install token-max`
installs it.

## Plan

The version PR bumps `version` in `Cargo.toml`, and once it lands,
`release.yml` publishes everything. 0.2.0 rather than 0.1.1, since
`crabx install` is a new command and the delay changes which version an
unpinned run picks.

This is crabx's first release through `release.yml`. npm's trusted publishers for its
packages were added on 2026-11-25 and expire unless a publish lands by 2026-11-27 00:18 UTC;
[first publish](/quest/m0/first-publish.md)'s `Waiting` says how to re-add them if this
release comes later. After it lands, check that `release.yml`'s npm job published
0.2.0. Then run `npx @kixel/crabx@0.2.0 token-max@0.1.0 --version` and
`npx @kixel/crabx@0.2.0 install token-max@0.1.0`, pinned so the 24h delay
doesn't matter. Once token-max 0.1.0 is a day old, run the unpinned
commands in the goal, with `@kixel/crabx@0.2.0`, on an empty crabx cache, so
a cold run resolves it rather than a warm cache's day-old index.

kixel-ai/token-max's `quest/m1/crabx.md`, which adds crabx tabs to
max.kixel.me, waits on this release.

## Required

- [Musl fallback](/quest/m0/musl-fallback.md) - runs crates with only musl Linux builds, such as token-max
- [`crabx install`](/quest/m1/install.md) - the install command max.kixel.me offers
- [Release-age delay](/quest/m1/release-age.md) - the default the maintainer wants in place before more people run crabx

## Related

- [First publish](/quest/m0/first-publish.md) - holds the npm trust step this release needs
