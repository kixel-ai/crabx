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

0.1.1, the first publish through `release.yml`, validates npm's trusted publishers first, so this
release needs no npm setup. Before bumping, check that npm has all seven packages at 0.1.1:
`release.py npm-plan` only publishes crates.io's newest version, so once 0.2.0 is there no
run backfills 0.1.1. After it lands, check that `release.yml`'s npm job published
0.2.0. Then run `npx @kixel/crabx@0.2.0 token-max@0.1.0 --version` and
`npx @kixel/crabx@0.2.0 install token-max@0.1.0`, pinned so the 24h delay
doesn't matter. Once token-max 0.1.0 is a day old, run the unpinned
commands in the goal, with `@kixel/crabx@0.2.0`, on an empty crabx cache, so
a cold run resolves it rather than a warm cache's day-old index.

kixel-ai/token-max's `quest/m1/crabx.md`, which adds crabx tabs to
max.kixel.me, waits on this release.

## Required

- [`crabx install`](/quest/m1/install.md) - the install command max.kixel.me offers
- [Release-age delay](/quest/m1/release-age.md) - the default the maintainer wants in place before more people run crabx
- [Pipeline release](/quest/m1/pipeline-v1.0.1.md) - pins the hardened release pipeline in `crabx init`
- [Attestation binding](/quest/m1/attestation-binding.md) - binds branch-ref attestations to the crate, version, and runner, enforces GitHub origin checks across `pkg-url` and redirects, and isolates 0.2.0 entries in a policy-versioned cache namespace
- [Verify before extract](/quest/m1/verify-before-extract.md) - verifies signatures and attestations before extracting and bounds registry, network, and staging budgets
- [Build cwd and Cargo home isolation](/quest/m1/build-cwd.md) - isolates working directory and `CARGO_HOME` for `--build` and background `--refresh`

## Related

- [First publish](/quest/m0/first-publish.md) - holds the npm trust step this release needs
- [Attestation ratchet](/quest/m1/attestation-ratchet.md) - TOFU downgrade protection and `--require-attestation`
