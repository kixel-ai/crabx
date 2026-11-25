# crabx: run a Rust CLI without compiling it

## Goal

Someone with no Rust toolchain runs a Rust CLI as `crabx <crate>[@version] args`, as fast
as npx on a warm cache, from a prebuilt binary for their platform, and never compiles
unless they pass `--build`. Any crate whose releases carry assets binstall finds works,
with quickinstall's third-party builds behind a flag. A repo opts into prebuilt builds of each crates.io version, for Linux, macOS, and Windows on x86_64 and arm64, by
adding one configless workflow. moq's CLIs (moq-cli, moq-relay) run through crabx first,
from moq's own releases.

Non-goals: hosting a builder, building repos that haven't opted in, builds of `main` or
any version not on crates.io, private repos, and replacing cargo-binstall or crates.io.

What stays here once every child has merged: nothing. The docs page is the site at
https://crabx.kixel.me, and the end-to-end check is `.github/workflows/e2e.yml`.

## Plan

Decided with the maintainer on 2026-11-12, in /quest-plan:

- ✅ Builds run in the author's own CI on GitHub-hosted runners, free for public repos,
  and publish to the author's GitHub releases with GitHub attestations. Rejected: a
  builder we host (it runs strangers' build.rs with our credentials, makes us the trust
  root, and GitHub's Actions terms forbid one repo's runners building unrelated
  projects); author CI now with a hosted builder later.
- ✅ Our own runner, built on the binstalk crate. Rejected: upstreaming a `run` mode into
  cargo-binstall (it installs to `~/.cargo/bin` and asks crates.io every run); adopting
  or forking [crgx](https://github.com/yfedoseev/crgx) (one maintainer; it lacks
  attestation checks, and turns quickinstall on by default, against the trust rule below)
  or [cgx](https://github.com/anelson/cgx) (it compiles by default and needs config).
- ✅ A runner installed once, and also published to npm so `npx crabx <crate>` needs
  nothing new. Rejected: runner only; each tool published to npm or PyPI on its own.
  Decided 2026-11-24 (maintainer, /quest-run): npm refuses the unscoped `crabx`, so ✅ the
  npm package is `@kixel/crabx`, installed once with `npm install -g @kixel/crabx`, and the
  docs drop `npx`/`bunx` runs (rejected: asking npm for `crabx`; another unscoped name).
  Superseded for the scoped package on 2026-11-24 (maintainer, /quest-plan in
  kixel-ai/token-max), so this governs over the line above: ✅ the docs
  offer the scoped `npx @kixel/crabx install` as a one-line install, and
  token-max's site runs `npx @kixel/crabx token-max doctor`.
- ✅ Anything binstall resolves from the author's releases, so crates that already attach
  release assets work on day one; the pipeline primarily exists to set those assets up. Rejected: opted-in repos
  only; any crates.io crate built on demand.
- ✅ Trust: crabx runs our pipeline's builds and an author's own release assets, and
  verifies an attestation wherever the repo publishes one (a bad one is an error).
  cargo-quickinstall, built and signed by a third party, is refused unless a flag allows
  it. Rejected: attested only; everything binstall does.
- ✅ Only crates.io versions are built. Rejected: a rolling `main` prerelease, GHCR OCI
  artifacts, and per-commit prereleases.
- ✅ Linux binaries target gnu through cargo-zigbuild against an old glibc floor, such
  as 2.28 (RHEL 8, Debian 10). Rejected: static musl (a slow allocator for moq-relay, and
  native deps often fail to build); both. On musl Linux, crabx fails loudly and points
  at `--build`.
- ✅ An unpinned run is stale-while-revalidate: it runs the cached latest and checks
  crates.io's sparse index in the background for the next run. Once cached, a pinned version
  runs with no network. Rejected: a check every run; a 24-hour TTL.
  Superseded 2026-11-24 (maintainer, /quest-plan in kixel-ai/token-max): the
  background check runs at most daily, and unpinned runs skip versions under 24
  hours old; see [the release-age quest](/quest/m1/release-age.md).
- ✅ The name `crabx`, free on crates.io, following npx, bunx, and uvx; npm's package is
  `@kixel/crabx`, as above.
  Rejected: xcrate, rusx, carx.
- ✅ A pipeline workflow and `crabx init` now, the GitHub App
  [later](/quest/m1/app.md). Rejected: the App in the first cut; no App.
- ✅ m2, the App in m3. Rejected: m1; m3. These were rust2's milestones; in this tree,
  first publish is m0 and the App m1.

Decided by the maintainer on 2026-11-22, in
[kixel-ai/rust2#948](https://github.com/kixel-ai/rust2/pull/948#issuecomment-7074616370):
✅ crabx moves from `kixel-ai/rust2` to this repository, keeping its history (rejected: a
fresh start; an interim crabx-only toolchain in rust2), since binstalk 0.31 needs Rust 1.100
while rust2 builds on moq's 1.98.1. Its history is rust2's, filtered to crabx's paths, with
bare `#N` references rewritten to `kixel-ai/rust2#N`.

Decided 2026-11-24 (maintainer, /quest-run): ✅ moq keeps its own release workflow, and no
quest switches it to the pipeline (rejected: planning the switch in moq-dev/moq; a moq quest
adding only the targets it lacks). `crabx moq-cli` and `crabx moq-relay` already run moq's
release assets. The switch would add macOS x64, Windows arm64, and a glibc 2.28 floor (moq
builds on Ubuntu 22.04), while moq's armv7, `.deb`/`.rpm`, and apt builds would stay its own.

Corrected 2026-11-24 (/quest-run): moq's release assets carry no GitHub attestation, so
crabx runs them unverified, as it does any release without one; the decision above said
they were attested. As of moq-relay 0.17.18 and moq-cli 0.14.18, `gh attestation verify -R
moq-dev/moq` returns 404 for their x86_64 Linux archives. The switch would also attest
them.

Decided 2026-11-24 (maintainer, [kixel-ai/crabx#24](https://github.com/kixel-ai/crabx/pull/24#issuecomment-7141564623)):
✅ moq's releases get attested by keeping moq's own release workflow and adding
`actions/attest-build-provenance` to it (rejected: reopening the pipeline switch; accepting
unattested moq releases), a small moq-only change that gives crabx's first consumer
verified provenance. moq's `main` already has that job
([moq-dev/moq#7224](https://github.com/moq-dev/moq/pull/7224)), but moq cuts its tags from
its `release` branch, which lacks it, so the remaining work is getting the job onto the
branch moq tags from and checking the next archives verify. That is moq-dev/moq's work,
planned in its own quest tree.
