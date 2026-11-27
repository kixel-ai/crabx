crabx runs a Rust CLI from its author's release without compiling it, and ships the reusable pipeline that builds those releases. See [README.md](README.md), [pipeline/README.md](pipeline/README.md), and [quest/README.md](quest/README.md).

# Required

- Trust fails closed: a malformed attestation, a signer outside the crate's repository and crabx's pipeline, or an API or verification failure is an error, never a warning or fallback to running the binary.
- A release with no usable prebuilt binary for the platform compiles from crates.io with Cargo, saying so on stderr, never a different binary; only a trust failure or `--require-attestation` refuses instead. On glibc Linux, the same release's static musl build comes before compiling. Malformed input is an error naming what to do instead.
- Everything is configless with opinionated names and defaults: `crabx init` writes one workflow, and a crate needs no extra config. Each config key a consumer needs is a gap to close.
- Build jobs never hold publishing credentials: only jobs in `crates-io`, `npm`, and `github-pages` environments (which only `main` may deploy to) publish, and only the pipeline's `publish` job (running no crate code) writes GitHub releases and attestations.
- `moq` (`moq-cli`, `moq-relay`) is the first consumer and runs through crabx first.

# Development

PRs target `main`. Run `just ci` before pushing; outside the dev shell, run `nix develop --command just ...`.

Quests: before any work, run `quest context` and follow it, or read https://github.com/kixel-ai/quest/blob/main/docs/context.md where `quest` is not installed, unless this session already holds its rules.
