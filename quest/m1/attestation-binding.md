# [S] Bind branch-ref attestations to the crate, version, and runner

## Goal

Branch-triggered GitHub attestations verify the exact crate, version, and runner environment (not only the repository and workflow path), and repository redirects or `pkg-url`-only GitHub releases cannot bypass or misbind attestation checks.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#758` (runner) and `kixel-ai/rust2#781` (pipeline provenance).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ ship this in `m1` for [0.2.0](/quest/m1/release-0.2.md), requiring [0.1.1](/quest/m0/release-0.1.1.md) so `0.1.1` stays musl-only.

Changes to make:

- In `src/trust.rs`:
  - When the verified workflow signer matches `PIPELINE_SIGNER`, parse the verified DSSE in-toto statement's `predicate.buildDefinition` and require:
    - `externalParameters.crate` matches the requested crate name (ASCII case-insensitive),
    - `externalParameters.version` matches the requested version, and
    - `internalParameters.github.runner_environment == "github-hosted"`.
  - For non-pipeline workflow attestations signed from a `refs/heads/*` ref, check that the in-toto `subject` entry matching the artifact's SHA-256 has a filename matching the downloaded asset's filename when `name` is present, preventing cross-crate or cross-version archive substitution within a repository.
- Review `Receipt.repository` sourcing (`vendor/binstalk-fetchers/src/lib.rs` `get_repo_info` and `src/trust.rs`):
  - Do not let a non-GitHub `repository` URL that redirects to GitHub establish a trusted GitHub attestation repository from an arbitrary redirect.
  - When `Cargo.toml` omits `repository` but `pkg-url` downloads from `https://github.com/<owner>/<repo>/releases/download/...`, still query and verify GitHub attestations for `<owner>/<repo>`.
- Add offline unit tests in `src/trust.rs` covering wrong crate, wrong version, self-hosted runner, cross-version swap on a branch ref, non-GitHub redirect, and `pkg-url`-only GitHub releases.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
