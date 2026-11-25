# [M] Harden the reusable release pipeline against artifact and PyPI tampering

## Goal

The reusable pipeline narrows build uploads to each target's exact archive, scrubs Actions runtime credentials from `cargo`, verifies producing job success, uploaded artifact digests, and strict single-file layouts in `publish` (preserving partial reruns of `publish` and existing caller permissions), verifies SHA-256 hashes for every PyPI package installed in Linux builds, exercises the real artifact handoff in read-only PR CI, and bounds HTTP and job runtimes.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#756` (initial pipeline).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan all audit recommendations and land build-worker changes after [0.1.1](/quest/m0/release-0.1.1.md) is cut. Because `.github/workflows/release.yml`'s `check` job calls `./.github/workflows/crabx-build.yml` directly from `main`, this quest lives in `m1` and requires `release-0.1.1.md` so the 0.1.1 release stays on the proven build path until npm's trusted publishers validate. It is followed by [pipeline-v1.0.1](/quest/m1/pipeline-v1.0.1.md) to tag and bump the pin in `src/binaries.yml`.

Decided 2026-11-24 (maintainer, [PR #30](https://github.com/kixel-ai/crabx/pull/30) checkpoint): ✅ keep the single dynamic matrix in `crabx-build.yml` and state the GitHub Actions run-scoped artifact boundary plainly rather than trying to prove cryptographic per-job artifact IDs through GitHub's REST API (rejected: restructuring `crabx-build.yml` into fixed per-target or per-crate jobs for per-job outputs). GitHub's workflow-run artifacts API does not expose a producing `job_id`, and all six targets of a crate already compile the same crate's `build.rs` and `Cargo.lock`; the safeguards below stop `archives/*.tgz` globbing, failed-target masking, duplicate or unexpected files, unhashed PyPI installs, and stalled calls.

Changes to make:

- Preserve the caller permission contract (or explicitly bump to a new major tag if an API requires `actions: read`):
  - A reusable workflow cannot raise permissions its caller withheld: existing callers (`src/binaries.yml`, `release.yml`, and `crabx-test.yml`'s `workflow_dispatch`) grant `contents: write`, `id-token: write`, and `attestations: write` (without `actions: read`), and PR CI grants only `contents: read`.
  - Keep the artifact download and verification compatible with those existing caller permissions: use `actions/download-artifact` (which authenticates through the Actions runtime token without needing `actions: read`) downloaded per target without `merge-multiple: true`, and verify whether GitHub allows public-repo `actions/runs/{run_id}/jobs` and `.../artifacts` reads under `contents: read` in PR CI.
  - If GitHub requires `actions: read` for any job or artifact lookup used by `publish` or the PR verifier, do not break `v1` callers on Dependabot pin updates: add `actions: read` to `crabx.yml`, `crabx-test.yml` (both PR verifier and manual fixture), `release.yml`, `src/binaries.yml`, and `pipeline/README.md`, and coordinate with [pipeline-v1.0.1](/quest/m1/pipeline-v1.0.1.md) to ship the permission change under `pipeline-v2.0.0` (keeping the PR verifier read-only: `contents: read` plus `actions: read` at most).
- Isolate `cargo` from Actions runtime env vars and narrow uploads:
  - In `pipeline/pipeline.py` (`build()`), scrub GitHub Actions runtime and file-command environment variables (`ACTIONS_RUNTIME_TOKEN`, `ACTIONS_RESULTS_URL`, `ACTIONS_CACHE_URL`, `GITHUB_OUTPUT`, `GITHUB_ENV`, `GITHUB_PATH`, `GITHUB_STEP_SUMMARY`) from the `cargo` / `cargo-zigbuild` subprocess environment so an untrusted `build.rs` or proc-macro has no direct access to Actions artifact or step-output APIs via its environment.
  - In `.github/workflows/crabx-build.yml`, narrow `actions/upload-artifact` from `archives/*.tgz` to the single exact archive `archives/${{ matrix.name }}-${{ matrix.target }}-v${{ matrix.version }}.tgz` with `overwrite: false`, keeping the artifact name `crabx-${{ matrix.name }}@${{ matrix.version }}@${{ matrix.target }}` so rerunning a failed `publish` job still reuses successful build artifacts from the prior attempt.
- Verify build-job success, artifact uniqueness, uploaded digest, and strict layout in `publish`:
  - In `.github/workflows/crabx.yml`'s `publish` job, download artifacts without `merge-multiple: true` so each target lands in its own directory (never merging arbitrary files across targets).
  - In `pipeline/pipeline.py` (`provenance` / `built`), verify that:
    - every expected `(crate, version, target)` matrix build job's latest execution in the run completed with `conclusion == "success"` (so a failed build cannot be masked even if another job produced a file, while rerunning only a failed `publish` job still sees the successful build jobs from the previous attempt),
    - each target has a single unique non-expired artifact matching `crabx-<name>@<version>@<target>` (rejecting duplicates or unexpected artifact directories),
    - each target's downloaded artifact directory holds only that target's single regular `.tgz` file (no symlinks, no extra files, no path traversal), and
    - the uploaded artifact digest matches the exact uploaded bytes before unpacking when verifying via the Actions artifact API (ZIP wrapper SHA-256 before unzipping, or raw `.tgz` SHA-256 if using `actions/upload-artifact@v7` with `archive: false`).
  - Stage only the verified `.tgz` files into a clean directory passed to `actions/attest` (`subject-path`) and `publish`.
- Exercise the real artifact handoff in read-only PR CI:
  - Extend `.github/workflows/crabx-test.yml`'s `pull_request` path with a read-only downstream job (carrying only the exact read permissions callers grant) that runs the same artifact download, job-status check, digest check, and archive validation on the fixture's uploaded artifacts (without attesting or publishing), so permission or upload/download/digest mismatches fail PR checks instead of manual releases.
- Lock Linux build-tool PyPI dependencies:
  - Pin the full transitive closure of `cargo-zigbuild` and `ziglang` with `--hash=sha256:...` in `pipeline/requirements.txt` and pass `--require-hashes` to `pip install` in `.github/workflows/crabx-build.yml`. Keep Dependabot's pip updates working or document how hashes refresh.
- Add `timeout=60` to `urllib.request.urlopen` calls in `pipeline/pipeline.py` (matching `release.py`), and set `timeout-minutes` on the jobs in `.github/workflows/crabx.yml` and `.github/workflows/crabx-build.yml`.
- Cover the per-target artifact validation (missing archive, extra `.tgz`, wrong filename, symlink, failed/missing producing job, partial-rerun reuse of a prior attempt's successful build job, duplicate artifact, ZIP/raw digest mismatch) in `pipeline/test_pipeline.py`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut on the existing `crabx-build.yml` before build-worker changes land on `main`
