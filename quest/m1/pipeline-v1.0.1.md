# [XS] Tag pipeline-v1.0.1 and bump crabx's pinned pipeline commit

## Goal

The hardened reusable pipeline is tagged `pipeline-v1.0.1` (keeping `.github/workflows/crabx.yml`'s `v1` caller permission contract and `PIPELINE_SIGNER` identity unchanged) after verifying a manual fixture dispatch, and `src/binaries.yml`, `pipeline/README.md`, and `.github/workflows/release.yml` pin that tag's commit.

## Plan

Follows [pipeline/README.md](/pipeline/README.md)'s release checklist after [pipeline-hardening](/quest/m1/pipeline-hardening.md) lands:

1. Confirm [pipeline-hardening](/quest/m1/pipeline-hardening.md) kept `.github/workflows/crabx.yml` compatible with existing `v1` caller permissions (`contents: write`, `id-token: write`, `attestations: write`, without requiring `actions: read`) and preserved its signer path (`kixel-ai/crabx/.github/workflows/crabx.yml`).
2. Dispatch `.github/workflows/crabx-test.yml` manually on the commit to tag and verify each fixture archive with `gh attestation verify <archive> -R kixel-ai/crabx --signer-workflow kixel-ai/crabx/.github/workflows/crabx.yml`.
3. Tag `pipeline-v1.0.1` on the verified commit and update the pinned SHA and `# pipeline-v1.0.1` comment in `src/binaries.yml`, `pipeline/README.md`, and `.github/workflows/release.yml` (existing tests check they agree).

## Required

- [Pipeline hardening](/quest/m1/pipeline-hardening.md) - the pipeline fixes this tag ships (already ordered after 0.1.1)
