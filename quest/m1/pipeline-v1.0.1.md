# [XS] Push the pipeline-v1.0.1 tag

## Goal

The hardened reusable pipeline is tagged `pipeline-v1.0.1` at the commit every pinned caller in this repository already points at (`b206ab0a5a2ca06abfc4672d1a61b182b51fc6f3`), keeping `.github/workflows/crabx.yml`'s `v1` caller permission contract and `PIPELINE_SIGNER` identity unchanged.

## Plan

The fixture dispatch on that commit (run 45689012079) built all six targets, and each archive passed `gh attestation verify … --signer-workflow kixel-ai/crabx/.github/workflows/crabx.yml`. `src/binaries.yml`, `.github/workflows/release.yml`, and `pipeline/README.md` pin that commit with a `# pipeline-v1.0.1` comment. Only the tag push remains; it is outward-facing and irreversible, so an agent takes it at the merge pick, then deletes this quest.

## Waiting

- Maintainer: push the release tag at the merge pick, which an agent may take once the bump PR has merged: `git tag pipeline-v1.0.1 b206ab0a5a2ca06abfc4672d1a61b182b51fc6f3 && git push origin pipeline-v1.0.1`, then check that `git ls-remote origin refs/tags/pipeline-v1.0.1^{}` prints that commit. No workflow runs on the tag; the `Release` run on the merge push is the pin's own check, so confirm it succeeded first.
