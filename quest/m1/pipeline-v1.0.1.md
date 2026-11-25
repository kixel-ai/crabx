# [XS] Tag hardened pipeline release and bump crabx's pinned pipeline commit

## Goal

The hardened reusable pipeline is tagged (`pipeline-v1.0.1` if caller permissions stay unchanged, or `pipeline-v2.0.0` if `actions: read` was added to callers) after verifying a manual fixture dispatch, and `src/binaries.yml`, `pipeline/README.md`, and `.github/workflows/release.yml` pin that tag's commit and any required caller permissions.

## Plan

Follows [pipeline/README.md](/pipeline/README.md)'s release checklist after [pipeline-hardening](/quest/m1/pipeline-hardening.md) lands:

1. Confirm whether [pipeline-hardening](/quest/m1/pipeline-hardening.md) preserved the `v1` caller permission contract (`contents: write`, `id-token: write`, `attestations: write`) or required adding `actions: read`.
   - If permissions are unchanged, tag `pipeline-v1.0.1` so Dependabot pin updates work cleanly for existing `v1` callers.
   - If `actions: read` is required, ensure `src/binaries.yml`, `pipeline/README.md`, `.github/workflows/release.yml`, and `.github/workflows/crabx-test.yml` (manual fixture and PR verifier) all include `actions: read` first, and tag `pipeline-v2.0.0` so existing `v1` callers are not silently broken by a patch-level Dependabot bump.
2. Dispatch `.github/workflows/crabx-test.yml` manually on the commit to tag and verify each fixture archive with `gh attestation verify <archive> -R kixel-ai/crabx --signer-workflow kixel-ai/crabx/.github/workflows/crabx.yml`.
3. Tag the verified commit and update the pinned SHA and tag comment in `src/binaries.yml`, `pipeline/README.md`, and `.github/workflows/release.yml` (existing tests check they agree).

## Required

- [Pipeline hardening](/quest/m1/pipeline-hardening.md) - the pipeline fixes this tag ships (already ordered after 0.1.1)
