# [XS] Tag hardened pipeline release and bump crabx's pinned pipeline commit

## Goal

The hardened reusable pipeline is tagged `pipeline-v1.0.1` (preserving the `crabx.yml` caller permission contract so Dependabot SHA bumps on downstream repositories keep working) after verifying a manual fixture dispatch, and `src/binaries.yml`, `pipeline/README.md`, and `.github/workflows/release.yml` pin that tag's commit.

## Plan

Follows [pipeline/README.md](/pipeline/README.md)'s release checklist after [pipeline-hardening](/quest/m1/pipeline-hardening.md) lands:

1. Confirm [pipeline-hardening](/quest/m1/pipeline-hardening.md) kept `.github/workflows/crabx.yml` compatible with existing `v1` caller permissions (`contents: write`, `id-token: write`, `attestations: write`, without requiring `actions: read`), because Dependabot updates commit-SHA pins on `crabx.yml` across major tags and would break downstream callers if `crabx.yml` added a new required permission. (If a separate opt-in workflow path such as `.github/workflows/crabx-v2.yml` was introduced instead, update `src/binaries.yml`, `pipeline/README.md`, `release.yml`, and `crabx-test.yml` to that new path and tag `pipeline-v2.0.0`, leaving `crabx.yml` unbroken for existing callers.)
2. Dispatch `.github/workflows/crabx-test.yml` manually on the commit to tag and verify each fixture archive with `gh attestation verify <archive> -R kixel-ai/crabx --signer-workflow kixel-ai/crabx/.github/workflows/crabx.yml`.
3. Tag the verified commit and update the pinned SHA and tag comment in `src/binaries.yml`, `pipeline/README.md`, and `.github/workflows/release.yml` (existing tests check they agree).

## Required

- [Pipeline hardening](/quest/m1/pipeline-hardening.md) - the pipeline fixes this tag ships (already ordered after 0.1.1)
