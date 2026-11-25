# [XS] Publish crabx 0.1.0 by hand once

## Goal

crabx 0.1.0 is on crates.io and npm, and both registries trust only
`.github/workflows/release.yml` to publish what follows, so `npm install -g @kixel/crabx`,
`cargo install crabx`, and both installers in [crabx's README](/README.md) work.

## Plan

Neither registry lets trusted publishing create a package, so each first version is
published with the maintainer's own login; release.yml publishes every later version.
Until crabx is on crates.io, release.yml fails at its plan job, saying so. The first run
after the crates.io publish attaches the binaries and then fails at its npm job, since npm
doesn't trust the workflow until the packages exist: that's expected, and the steps below
fix it.

The maintainer chose to keep this quest rather than fold its steps into the PR that added
release.yml ([#826](https://github.com/kixel-ai/rust2/pull/826)).

Decided by the maintainer on 2026-11-22, in
[#948](https://github.com/kixel-ai/rust2/pull/948#issuecomment-7074616370): ✅ this waits
for crabx's move from `kixel-ai/rust2` to this repository, so both registries trust this
repository's `release.yml` from the first version.

GitHub Pages serves the installers; a Cloudflare redirect to the raw files was rejected
because it lives outside the repository and serves whatever path layout main happens to have.

Done on 2026-11-18 (/quest-complete, with the maintainer's go-ahead): `kixel.ai` is verified
for Pages in the kixel-ai organization; Pages builds from Actions at `crabx.kixel.ai`;
Cloudflare has the challenge TXT record and the `crabx` CNAME to `kixel-ai.github.io`, DNS
only; and only `main` may deploy to the `github-pages`, `crates-io`, and `npm` environments.
The installers deployed and HTTPS was enforced, so `curl -fsSL https://crabx.kixel.ai/install.sh`
printed the installer. GitHub issued the certificate only after the custom domain was removed
and set again. The move redid the Pages and environment steps in `kixel-ai/crabx`. On
2026-11-25 the site moved to `crabx.kixel.me` (verified for Pages too) and the kixel.ai
`crabx` CNAME was deleted.

Decided 2026-11-24 (maintainer, /quest-run): the npm user `crabx`, the only member of the
existing `crabx` npm organization, isn't the maintainer's (signed in as `kixelated`, who isn't
a member), so ✅ the platform packages publish as `@kixel/crabx-<platform>`, under the
`kixel` npm organization the maintainer owns (rejected: the maintainer's user scope
`@kixelated`; register and use an `@kixel-ai` organization; dispute `@crabx` with npm). The launcher stays the unscoped `crabx`.

Done on 2026-11-24 (/quest-run, at the maintainer's request): crabx 0.1.0 is on crates.io,
published from a clean detached checkout of `main` at `f1d6151`; crates.io trusts
`kixel-ai/crabx`'s `release.yml` in the `crates-io` environment, added through the
crates.io API with the maintainer's token; and
[release.yml's run](https://github.com/kixel-ai/crabx/actions/runs/45302449030) attached all
six attested archives to `crabx-v0.1.0`, then failed at its npm job as expected.

Done on 2026-11-24 (/quest-run, with the maintainer's 2FA code): the six
`@kixel/crabx-<platform>` packages are on npm at 0.1.0, packed by `release.py` from `main` at
`1818d86` after it verified each archive's attestation. npm refused the unscoped launcher
`crabx` with "403 Package name too similar to existing package rax".

Decided 2026-11-24 (maintainer, /quest-run): ✅ the npm launcher is `@kixel/crabx`, which
users install once with `npm install -g @kixel/crabx` to get the plain `crabx` command;
crabx's docs drop `npx crabx` and `bunx crabx`, since the point is running `crabx` itself
(rejected: asking npm support to allow `crabx`; another unscoped name).

Done on 2026-11-24 (/quest-run, with the maintainer's 2FA code): `@kixel/crabx` 0.1.0 is on
npm, packed from `main` at `e2ab15a` on fedora, where the maintainer is signed in to npm. npm
answered the first publish of a new package name with `202` and served only a `0.0.0-stage`
placeholder for about two minutes before 0.1.0 appeared. `npm install -g @kixel/crabx` then
installed it from the registry, and `crabx --version` printed `crabx 0.1.0`.

Done on 2026-11-25 (/quest-run): [release.yml's run](https://github.com/kixel-ai/crabx/actions/runs/45600130205)
on `main` at `48d08ef` published crabx 0.1.1 to crates.io and all seven npm packages through
trusted publishing, each with a provenance attestation, well before the 2026-11-27 00:18 UTC
expiry, so npm accepted every pending publisher. npm served `@kixel/crabx-win32-arm64@0.1.1`
a few minutes after the others. On glibc 2.44, `npx @kixel/crabx@0.1.1 token-max@0.0.1
--version` with empty crabx and npm caches printed `0.0.1`, from token-max's musl build.
The agent Chrome session wasn't signed in to npm, so no one read the settings pages.

## Waiting

- Maintainer: turn on crabx's "Require trusted publishing" (Settings → Trusted Publishing),
  so a leaked token can't publish. The proof it waited for is in: release.yml published 0.1.1
  to crates.io. An agent can do this through the crates.io API with the maintainer's token,
  which only the maintainer can provide. Check `trustpub_only` is `true` at
  `https://crates.io/api/v1/crates/crabx`.
