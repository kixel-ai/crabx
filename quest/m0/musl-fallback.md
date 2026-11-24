# [S] Run a crate's musl Linux build when it has no gnu one

## Goal

On glibc Linux, crabx tries `<arch>-unknown-linux-gnu` and then
`<arch>-unknown-linux-musl`, so a crate that ships only static musl Linux
builds, such as token-max, runs from its author's release. A crate with both
still gets its gnu build.

## Plan

Found 2026-11-24 (/quest-plan in kixel-ai/token-max): `npx @kixel/crabx
token-max --version` on x86_64 glibc Linux failed with `no usable prebuilt
release for token-max@0.0.1 on x86_64-unknown-linux-gnu`, though that release
has `token-max-x86_64-unknown-linux-musl.tar.xz`. `install::host_targets`
returns only `binstalk::TARGET` (plus the universal targets on macOS), while
cargo-binstall's own target detection falls back to musl on glibc. The gap
has been there since `host_targets` was first written, so no single PR caused it.

Recommendation: on glibc Linux, append the musl triple for the host's
architecture after the gnu one, keeping the order candidates are tried in, so
gnu wins wherever a crate ships both. A static musl binary runs on any glibc
host, so this keeps the glibc 2.28 floor. Verify with
`crabx token-max --version` once token-max 0.1.0 is out, since that release
carries a GitHub attestation crabx checks.

Today `install::fetch` tries every target under the crate's name before any
under its single binary's name, so with musl added, a musl archive named for
the crate would beat a gnu archive named for the binary. Try both gnu names
before either musl one, and leave every other platform's order alone, such
as macOS preferring a crate-named universal archive.

Cover it in `.github/workflows/e2e.yml`'s Linux runs with a pinned
`crabx token-max@0.1.0 --version`, a musl-only release with an attestation,
so a regression to gnu-only fails the nightly.
