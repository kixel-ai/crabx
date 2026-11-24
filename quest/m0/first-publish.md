# [XS] Publish crabx 0.1.0 by hand once

## Goal

crabx 0.1.0 is on crates.io and npm, and both registries trust only
`.github/workflows/release.yml` to publish what follows, so `npx crabx <crate>`,
`bunx crabx <crate>`, `cargo install crabx`, and both installers in
[crabx's README](/README.md) work.

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

The installers are served at `crabx.kixel.ai` by GitHub Pages:
`.github/workflows/installers.yml` deploys `install.sh` and `install.ps1`
whenever main changes them. kixel.ai's DNS is on Cloudflare, so the record points the
subdomain at GitHub; a Cloudflare redirect to the raw files was rejected because it lives
outside the repository and serves whatever path layout main happens to have.

Done on 2026-11-18 (/quest-complete, with the maintainer's go-ahead): `kixel.ai` is verified
for Pages in the kixel-ai organization; Pages builds from Actions at `crabx.kixel.ai`;
Cloudflare has the challenge TXT record and the `crabx` CNAME to `kixel-ai.github.io`, DNS
only; and only `main` may deploy to the `github-pages`, `crates-io`, and `npm` environments.
The installers deployed and HTTPS is enforced, so `curl -fsSL https://crabx.kixel.ai/install.sh`
prints the installer. GitHub issued the certificate only after the custom domain was removed
and set again. The move redid the Pages and environment steps in `kixel-ai/crabx`.

Decided 2026-11-24 (maintainer, /quest-run): the npm user `crabx`, the only member of the
existing `crabx` npm organization, isn't the maintainer's (signed in as `kixelated`, who isn't
a member), so ✅ the platform packages publish as `@kixelated/crabx-<platform>` under the
maintainer's own scope (rejected: create and use an `@kixel-ai` organization; dispute
`@crabx` with npm). The launcher stays the unscoped `crabx`.

Done on 2026-11-24 (/quest-run, at the maintainer's request): crabx 0.1.0 is on crates.io,
published from a clean detached checkout of `main` at `f1d6151`; crates.io trusts
`kixel-ai/crabx`'s `release.yml` in the `crates-io` environment, added through the
crates.io API with the maintainer's token; and
[release.yml's run](https://github.com/kixel-ai/crabx/actions/runs/45302449030) attached all
six attested archives to `crabx-v0.1.0`, then failed at its npm job as expected.

## Waiting

- Maintainer: publish the npm packages, which verifies each archive's attestation first,
  then trust the workflow for each (npm 11.15.0 or later). An agent can't: both need the
  maintainer's npm login and 2FA. Check `npx crabx --version` prints `crabx 0.1.0`.

  ```sh
  npm login
  GH_TOKEN=$(gh auth token) python3 release.py publish 0.1.0
  for p in crabx @kixelated/crabx-linux-x64-gnu @kixelated/crabx-linux-arm64-gnu \
    @kixelated/crabx-darwin-x64 @kixelated/crabx-darwin-arm64 @kixelated/crabx-win32-x64 \
    @kixelated/crabx-win32-arm64; do
    npm trust github "$p" --file release.yml --repo kixel-ai/crabx --env npm --allow-publish --yes
    sleep 2
  done
  ```

- Maintainer: once release.yml has published a crates.io version, turn on crabx's "Require
  trusted publishing" (Settings → Trusted Publishing), so a leaked token can't publish. An
  agent can do this through the crates.io API with the maintainer's token, but it waits for
  proof that the workflow's publish works. Check `trustpub_only` is `true` at
  `https://crates.io/api/v1/crates/crabx`.
