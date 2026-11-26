# [S] Dry-run the pipeline on a consumer's unpublished source

## Goal

A crate repository using crabx's pipeline can run it on its own commit, before publishing that
version to crates.io, and see every target build. Nothing is attested, tagged or released.
Its release workflow then publishes to crates.io only after that run passes, so a version whose
binaries can't build never becomes immutable on crates.io.

## Plan

Decided 2026-11-25 (maintainer, /quest-plan review of kixel-ai/token-max#267): ✅ add a consumer
dry run to the pipeline (rejected: accepting a post-publish build failure and fixing it forward
with a new version). The no-publish mode exists only for crabx's own fixture caller today
(`crabx.yml`'s `mode` step), so token-max, the first outside consumer, can't check its release
commit before publishing.

Recommendations:

- A `workflow_call` input, such as `dry-run: true`, skips crates.io discovery and builds the
  caller's tracked binary crates at the commit being run, at their manifest versions, on every
  target. It's the same build path as a release.
- The dry run attests nothing and needs no `id-token` or `contents: write`, so it can't mint an
  attestation crabx would accept for an unpublished build. That also lets it run from any branch.
- `crabx init`'s caller stays configless. A consumer that wants the gate calls the pipeline with
  the input from its own release workflow, as crabx's `release.yml` will. Document the pattern in
  `pipeline/README.md`.
- Use it in crabx's own `release.yml` before its crates.io publish, so crabx's releases are
  gated the same way.
