# [XS] Tag pipeline-v1.0.1 and bump crabx's pinned pipeline commit

## Goal

The hardened reusable pipeline is tagged `pipeline-v1.0.1` after verifying a manual fixture dispatch, and `src/binaries.yml`, `pipeline/README.md`, and `.github/workflows/release.yml` pin that tag's commit.

## Plan

Follows [pipeline/README.md](/pipeline/README.md)'s release checklist after [pipeline-hardening](/quest/m0/pipeline-hardening.md) lands:

1. Dispatch `.github/workflows/crabx-test.yml` manually on the commit to tag and verify each fixture archive with `gh attestation verify <archive> -R kixel-ai/crabx --signer-workflow kixel-ai/crabx/.github/workflows/crabx.yml`.
2. Tag `pipeline-v1.0.1` on that commit.
3. Update the pinned SHA and `# pipeline-v1.0.1` comment in `src/binaries.yml`, `pipeline/README.md`, and `.github/workflows/release.yml` (existing tests check they agree).

## Required

- [Pipeline hardening](/quest/m0/pipeline-hardening.md) - the pipeline fixes this tag ships
- [0.1.1](/quest/m0/release-0.1.1.md) - cut before `src/binaries.yml` changes on `main`, so 0.1.1 ships only the musl fallback
