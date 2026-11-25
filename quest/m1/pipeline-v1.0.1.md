# [XS] Tag pipeline-v1.0.1 and bump crabx's pinned pipeline commit

## Goal

The hardened reusable pipeline is tagged `pipeline-v1.0.1` (keeping `.github/workflows/crabx.yml`'s `v1` caller permission contract and `PIPELINE_SIGNER` identity unchanged) and every pinned caller in this repository points at that tag's commit.

## Plan

Follow the single release checklist in [pipeline/README.md](/pipeline/README.md) (which lists `src/binaries.yml`, `pipeline/README.md`, and `.github/workflows/release.yml`) to verify the manual fixture dispatch, tag `pipeline-v1.0.1`, and bump the pinned commit.
