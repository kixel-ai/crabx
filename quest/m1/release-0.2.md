# [XS] Check crabx 0.2.0 on npm and offer npx install in the docs

## Goal

crabx 0.2.0, with the musl fallback, `crabx install` and the release-age
delay, is on crates.io and on npm as `@kixel/crabx`, published by
`release.yml`, so `npx @kixel/crabx@latest fnox --version` runs fnox on
glibc Linux and `npx @kixel/crabx@latest install fnox` installs it; and the
install docs offer `npx @kixel/crabx install`.

## Plan

The version PR bumped `version` in `Cargo.toml` to 0.2.0 (rather than 0.1.1's
successor 0.1.2, since `crabx install` is a new command and the delay changes
which version an unpinned run picks); its merge runs `release.yml`.

Check that `release.yml`'s npm job published 0.2.0, then on glibc Linux with
an empty crabx cache (`XDG_CACHE_HOME` and `HOME` pointed at new directories) run
`npx @kixel/crabx@0.2.0 crabx@0.2.0 --version`,
`npx @kixel/crabx@0.2.0 fnox@1.50.0 --version` and
`npx @kixel/crabx@0.2.0 install fnox@1.50.0`, then the unpinned commands in
the goal with `@kixel/crabx@0.2.0`, so a cold run resolves through the 24h
delay rather than a warm cache's index. `release.yml`'s `npm-check` job also
runs `crabx install` from the published package on all three OSes.

fnox stands in for token-max, which the plan named before
[attestation binding](https://github.com/kixel-ai/crabx/pull/45) refused
token-max@0.0.1 (its only attestation signs from `refs/heads/main`) and
before token-max 0.1.0 existed. The musl-fallback check against a real crate
stays with [e2e-musl-fallback](/quest/m1/e2e-musl-fallback.md), which waits on
token-max republishing through crabx's pipeline.

Once npm's `latest` is 0.2.0, a PR offers `npx @kixel/crabx install` beside
the npm line in README.md's install block and the site's install tabs
(`site/index.html`), and drops "From crabx 0.2.0" from README.md's
`crabx install` section. npm's 0.1.x reads `install` as a crate name, so
neither can offer it before then.

kixel-ai/token-max's `quest/m1/crabx.md`, which adds crabx tabs to
max.kixel.me, waits on this release.

## Waiting

- Maintainer: merge the 0.2.0 version PR, which publishes to crates.io and npm
  through `release.yml`; check with `npm view @kixel/crabx version`.
