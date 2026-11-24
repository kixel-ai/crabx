crabx runs a Rust CLI from its author's release, without compiling it, and ships the reusable pipeline that builds those releases.
The [README](README.md) says how it works, the [pipeline README](pipeline/README.md) how a crate opts in, and the [quest tree](quest/README.md) what is under way.

# Required

- Trust fails closed: a malformed attestation, a signer outside the crate's repository and crabx's pipeline, or an API or verification failure is an error, never a warning or a fallback to running the binary.
- An unsupported platform, a missing prebuilt asset, or malformed input is an error naming what to do instead, such as `--build`; never a silent compile or a different binary.
- Everything is configless, with opinionated names and defaults: `crabx init` writes one workflow, and a crate needs no annotations or config beyond it. Each config key a consumer needs is a gap to close.
- Build jobs never hold publishing credentials, and pull request CI stays read-only: only jobs in the `crates-io`, `npm`, and `github-pages` environments, which only `main` may deploy to, publish to a registry or Pages, and only the pipeline's `publish` job, which runs no crate code, writes releases and attestations.
- moq is the first consumer: its CLIs, moq-cli and moq-relay, run through crabx first.

# Development

PRs target `main`.
Run `just ci` before every push; it is what CI's `check` job runs.
Outside the dev shell, such as in Pullfrog's job, which puts the shell's tools on `PATH` but not its variables, run `just` as `nix develop --command just ...`.

Review rounds run one local reviewer, `quest review --harness codex`, or `--harness claude` when Codex changed the branch last. Pullfrog's review on the PR is the second reviewer, and its approval is what lands it.

Quests: before any work, run `quest context` and follow it, or read https://github.com/moq-dev/quest/blob/main/docs/context.md where `quest` is not installed, unless this session already holds its rules.
