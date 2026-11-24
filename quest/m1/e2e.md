# [M] Run crabx end to end on every target, nightly

## Goal

A scheduled workflow runs crabx's built binary against the live crates.io and GitHub on all six prebuilt targets, from a job with no Rust toolchain on `PATH`, so the author-release path that no offline test reaches is checked: registry lookup, release discovery, archive download and SHA-256, attestation verification, installation, and execution.
The same workflow runs `cargo test` on macOS and Windows and `install.ps1` on Windows, since the merge gate runs only on Linux.
A failure fails the run loudly; nothing retries.

## Plan

- Its own workflow on `schedule` and `workflow_dispatch`, plus `pull_request` filtered to its own path so the PR adding it runs it, the way `crabx-test.yml` does; read-only `GITHUB_TOKEN`, GitHub-hosted runners (the repository is public).
- One job builds crabx per target; a second, with no `cargo` or `rustup` on `PATH` and `CARGO` unset, runs pinned versions of: a crate with a GitHub release attestation (the `jdx/communique` release `tests/data/github-attestation.json` came from is one), a crate built by crabx's pipeline once one is published (crabx itself after [the first publish](/quest/m0/first-publish.md)), a crate whose releases carry no attestation, and the refusals: a crate with no prebuilt asset for the host without `--build`, and one only quickinstall serves without `--allow-quickinstall`. Pins keep a third party's new release from changing the result.
- Then each run repeats the pinned invocation with the network blocked, which must still succeed from the cache.
- This takes over the end-to-end check [crabx's root quest](/quest/README.md) kept for last: nothing in it needs the other children.
- The cost audit owns CI run time; the job runs nightly, outside the merge gate.

## Waiting

- Maintainer: whether the install path also gets a hermetic test in the merge gate. Options: nightly live runs only (recommended: they check the real registry, GitHub API, and Sigstore contracts, and a hermetic test needs an injectable API origin, which the fixed `api.github.com` checks in `trust.rs` deliberately forbid); also fake registry and GitHub servers behind a test-only origin, which catches regressions before merge at the cost of mirroring both APIs by hand.
