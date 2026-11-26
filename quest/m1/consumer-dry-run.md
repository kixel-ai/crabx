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

crabx's own `release.yml` already has this gate: its `check` job builds the release commit on
every target through `crabx-build.yml` with `upload: false` before the crates.io publish. Expose
that path to consumers instead of adding a second one.

Decided 2026-11-25 (maintainer, review of this PR): ✅ a dry run never uploads artifacts
(rejected: leaving it to the implementer). A dry run and the real build after the publish can
share one workflow run and build the same crate, version and targets, so an upload would take
the release's artifact names and fail the real upload after crates.io already has the version.

Recommendations:

- A `workflow_call` input on `crabx.yml`, such as `dry-run: true`, skips crates.io discovery and
  plans the caller's tracked binary crates at the commit being run and at their manifest
  versions, then calls `crabx-build.yml` on every target with `upload: false`.
- The dry run attests nothing and needs no `id-token` or `contents: write`, so it can't mint an
  attestation crabx would accept for an unpublished build. That also lets it run from any branch.
- `crabx init`'s caller stays configless. A consumer that wants the gate calls the pipeline with
  the input from its own release workflow before publishing. Document the pattern in
  `pipeline/README.md`, and move crabx's own `check` job onto it if that comes out simpler.
