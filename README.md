<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/kixel-ai/crabx/raw/HEAD/assets/logo/logo-on-dark.svg">
    <img src="https://github.com/kixel-ai/crabx/raw/HEAD/assets/logo/logo-on-light.svg" alt="crabx" height="66">
  </picture>
</p>

# crabx

Run a crates.io CLI from its author's release assets:

```sh
crabx moq-cli --help
```

Install crabx once, from its release:

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://crabx.kixel.me/install.sh | sh
powershell -ExecutionPolicy Bypass -c "curl.exe --proto '=https' --tlsv1.2 -fsSL https://crabx.kixel.me/install.ps1 | Out-String | iex"
npm install -g @kixel/crabx
cargo install crabx
```

The installers fetch the newest stable release into `~/.local/bin`; set `CRABX_VERSION` or
`CRABX_INSTALL_DIR` to choose another. `install.ps1` also adds that directory to the user's
`Path`, which terminals opened afterwards pick up; `install.sh` only says when it is missing from `PATH`,
so add it there yourself before running `crabx` by name. Prebuilt crabx covers x86_64 and arm64 on glibc
Linux (2.28 or later), macOS, and Windows; elsewhere, such as on musl, use `cargo install`.
Then:

```sh
crabx moq-cli --help
crabx moq-cli@0.14.16 --help
crabx --bin moq-relay moq-relay --help
```

Options go before the crate. Everything after it, including flags, spaces, and
non-UTF-8 arguments on Unix, goes to the downloaded binary. A crate with several
binaries uses the one named after the crate, or its only binary. Otherwise choose
`--bin NAME`.
From crabx 0.2.0, `crabx install` puts crabx on PATH, in the installers' directory, copying itself there when it
runs from elsewhere, such as npm's cache. `crabx install CRATE[@VERSION]` also puts a launcher
named after the crate there, so `moq-cli --help` runs `crabx moq-cli --help`:

```sh
crabx install moq-cli
crabx install moq-cli@0.14.16
crabx install --bin moq-relay moq-relay
```

A launcher is a symlink to crabx on Unix and a hard link to `crabx.exe` on Windows, which crabx
recognizes by the name it was started under. It passes every argument to the crate, so an
unpinned launcher updates as `crabx CRATE` does, and a pinned one stays at its version.
Options go after `install` and before the crate. Installing resolves the crate, its release
and attestation, and its binary at once, so a typo or a crate with no usable release fails
then rather than on its first run. crabx records each launcher, with its version and options,
under its state directory (`~/.local/state/crabx` on Linux, the local application data
directory elsewhere); delete a launcher to uninstall it. It never overwrites a file it didn't
install there, nor installs a name already installed in another directory. Bare
`crabx install` also relinks Windows launchers to the crabx it installs; if one interrupted
there leaves a launcher missing, repeat the command that installed it, with its `@VERSION`
and options. On Windows it adds the directory to the user's `Path`; on Unix it only says when
PATH lacks it, so add the directory to PATH yourself before running `crabx` or a launcher by
name. Run a crate named
`install` as `crabx install@VERSION`.

Registry lookups and cache keys are case-insensitive. The explicit `--build` path
preserves the typed crate spelling for Cargo. Binary names passed through `--bin`
retain their spelling.

crabx uses binstalk's release discovery, archive layouts, and signature verification.
It never installs into `~/.cargo/bin`. Missing prebuilt assets are an error. Pass
`--build` explicitly to run `cargo install --locked` into crabx's cache instead.
`--allow-quickinstall` permits cargo-quickinstall's third-party builds after the
author's releases; those have a separate cache from the default trust policy.
Quickinstall downloads require its minisign signature; a missing signature is an error.

For GitHub repositories, crabx checks GitHub's attestations API for the downloaded
archive's SHA-256 before installing anything. Public Sigstore bundles verify their
certificate, SCT, signature, artifact digest, and transparency log against Sigstore's
production trust root. GitHub's timestamp-only bundles use GitHub's separate CA and
timestamp authority, verifying the certificate, signature, digest, and RFC 3161 time.
Both roots refresh through TUF. A malformed bundle, wrong repository, or verification/API
failure is an error. A release with no published attestation is allowed, as are author
releases hosted outside GitHub. Binstalk's configured minisign checks remain enabled. Optional
`GH_TOKEN` or `GITHUB_TOKEN` credentials raise GitHub's API rate limit for release
discovery and attestations; they are sent only to GitHub's API, `api.github.com`. crabx downloads
release archives without them, so releases from private GitHub repositories count as
missing; `--build` compiles those crates.

Workflow certificates must name the crate's repository and either that repository's
workflow or crabx's reusable pipeline, `.github/workflows/crabx.yml` in this repository. Source refs must be tags or branches, excluding
pull-request refs. A tag must match the asset's release tag or version. Branch-triggered
builds are accepted so the pipeline can backfill published crates. GitHub release
attestations must carry GitHub's release-service identity and sign the crate's
repository and matching release tag. The verified result
stays with the cached entry; a pinned, cached invocation makes no network request and
requires neither Cargo nor GitHub's CLI.

Caches live in the operating system's user cache directory, under
`crabx/v1/<host>/<trust-policy>/<crate>/<version>`. Entries are staged, verified, and
published atomically under a per-crate lock. An incomplete or corrupt entry fails
loudly. Build and quickinstall entries cannot satisfy the default author's-release
policy.
The next uncached installation removes abandoned `install-*` staging directories
under that lock. Malformed index or refresh state (`index.json`, `failure.json`) also
fails with its path; delete that file to reset update tracking. Pinned invocations do
not read either.

An unpinned invocation runs the newest stable, non-yanked release published at least 24
hours ago, by the `pubtime` crates.io's sparse index records, never a prerelease. A
hijacked release is usually yanked within hours, so the delay keeps most of them from ever
running. `--minimum-release-age DURATION` or `CRABX_MINIMUM_RELEASE_AGE` changes the
delay, as a whole number of `s`, `m`, `h`, `d`, or `w`, such as `7d`; `0s` turns it off.
A pinned `crate@version` ignores it. When no release is old enough, crabx names the newest
and when it qualifies; pin it or pass `--minimum-release-age 0s` to run it sooner. A
release whose index entry has no `pubtime` is an error, not old enough.

Each unpinned invocation picks from the crate's cached index with its own delay and the
current time; only a cache with no index fetches it first. It runs the newest installed
release that qualifies at once, or installs the newest qualifying release first when none
does. A detached refresh then checks the index for new releases at most once a day per
crate and trust policy, and installs the newest qualifying release for the next
invocation, so a new release usually first runs 24 to 48 hours after it is published.
A yank stops an installed release from running unpinned once a check sees it, and can
select an older release. Only one refresh per crate and policy runs at a time. Network
errors leave the cached release available. Each repeated error for a release is reported
once, on the next unpinned invocation. Failed refreshes retry after 30 seconds, doubling to
at most 30 minutes; a successful refresh resets that delay, and only a successful check
counts toward the daily limit. Conditional index requests retain their validator even when
installation fails. Installing a release also makes the requests binstalk or Cargo need,
such as binstalk's own read of the crate's index entry. Release requests have a
five-minute total timeout and a 30-second read timeout.

Unix uses `exec`, preserving the PID, arguments, standard streams, signals, and exit
status. Windows starts the program with inherited console and standard streams and
returns its exit code. crabx itself must run on glibc to download prebuilt Linux binaries;
musl builds of crabx direct users to `--build`. On glibc Linux, crabx runs a crate's
`-linux-gnu` build, or its static `-linux-musl` build when discovery finds no gnu one; a
dynamically linked musl build is an error naming `--build`.

`crabx init`, run in a public crate repository, writes the caller of
[crabx's release pipeline](pipeline/README.md) to
`.github/workflows/binaries.yml`, pinned to its current release, so each crates.io
version gets attested binaries crabx runs. It takes no options, reads origin's
default branch from git, and never overwrites a different existing file. Run a crate
named `init` as `crabx init@VERSION`.

Develop in the Nix dev shell with `just ci`, and `just run <crate> [args]`.
`.github/workflows/e2e.yml` runs each night: it builds this commit's crabx as a release does and runs it with
`tests/e2e.sh` against the live crates.io and GitHub on every target, then offline from its cache, and runs
`cargo test` on macOS and Windows and `install.ps1` on Windows. It packs the build's npm packages and, with
`tests/install.sh`, runs `crabx install` from them and a crate's launcher from a new shell on Linux, macOS, and
Windows. It also runs both documented install lines from crabx.kixel.me.

A release is a PR that bumps `version` in `Cargo.toml`. Once it lands, `.github/workflows/release.yml`
builds the commit on every target, publishes it to crates.io, runs [the pipeline](pipeline/README.md)
to attach attested binaries to the `crabx-vX.Y.Z` release, and publishes the newest stable version's
binaries to npm as `@kixel/crabx` and one `@kixel/crabx-<platform>` package per target, each verified against its
attestation first, then runs `tests/install.sh` on npm's version on every OS. Every step skips what is
already published, so a rerun finishes a failed release.
crates.io and npm trust only that workflow, in its `crates-io` and `npm` environments.
`.github/workflows/site.yml` serves `site/`, crabx's one-page site, and main's `install.sh` and
`install.ps1` at [crabx.kixel.me](https://crabx.kixel.me) through GitHub Pages.
crabx is GPL-3.0-only because it links binstalk.
