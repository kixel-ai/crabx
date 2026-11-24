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

The installers are served at `crabx.kixel.me` by GitHub Pages, beside crabx's site:
`.github/workflows/site.yml` deploys `site/`, `install.sh`, and `install.ps1` whenever main
changes them. A Cloudflare redirect to the raw files was rejected because it lives outside
the repository and serves whatever path layout main happens to have.

Done on 2026-11-18 (/quest-complete, with the maintainer's go-ahead), at the domain the site
later replaced: `kixel.ai` is verified
for Pages in the kixel-ai organization; Pages builds from Actions at `crabx.kixel.ai`;
Cloudflare has the challenge TXT record and the `crabx` CNAME to `kixel-ai.github.io`, DNS
only; and only `main` may deploy to the `github-pages`, `crates-io`, and `npm` environments.
The installers deployed and HTTPS is enforced, so `curl -fsSL https://crabx.kixel.ai/install.sh`
prints the installer. GitHub issued the certificate only after the custom domain was removed
and set again. The move redid the Pages and environment steps in `kixel-ai/crabx`.

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

## Waiting

- Maintainer: when the next version's release PR is about to land, trust release.yml for
  each npm package (npm 11.15.0 or later), not before: npm expires a new trusted-publisher
  configuration that hasn't published within 2 days
  (https://docs.npmjs.com/trusted-publishers), and release.yml's npm job only publishes a
  version npm lacks. It needs the maintainer's npm login and a 2FA code, so an agent runs it
  on fedora with a code the maintainer gives through the secret prompt. If a
  configuration expires first, delete it and create it again. Check release.yml's npm job
  publishes that version.

  ```sh
  for p in @kixel/crabx @kixel/crabx-linux-x64-gnu @kixel/crabx-linux-arm64-gnu \
    @kixel/crabx-darwin-x64 @kixel/crabx-darwin-arm64 @kixel/crabx-win32-x64 \
    @kixel/crabx-win32-arm64; do
    npm trust github "$p" --file release.yml --repo kixel-ai/crabx --env npm --allow-publish --yes
    sleep 2
  done
  ```

- Maintainer: once release.yml has published a crates.io version, turn on crabx's "Require
  trusted publishing" (Settings → Trusted Publishing), so a leaked token can't publish. An
  agent can do this through the crates.io API with the maintainer's token, but it waits for
  proof that the workflow's publish works. Check `trustpub_only` is `true` at
  `https://crates.io/api/v1/crates/crabx`.
