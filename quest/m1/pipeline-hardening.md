# [M] Harden the reusable release pipeline against artifact and PyPI tampering

## Goal

The reusable pipeline enforces a strict cross-job boundary between its unprivileged build jobs and its privileged `publish` job (binding every downloaded artifact to its producing target job and verifying the uploaded artifact digest before unpacking), verifies SHA-256 hashes for every PyPI package installed in Linux builds, exercises the real artifact handoff in read-only PR CI, and bounds HTTP and job runtimes.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#756` (initial pipeline).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan all audit recommendations and land build-worker changes after [0.1.1](/quest/m0/release-0.1.1.md) is cut. Because `.github/workflows/release.yml`'s `check` job calls `./.github/workflows/crabx-build.yml` directly from `main`, this quest lives in `m1` and requires `release-0.1.1.md` so the 0.1.1 release stays on the proven build path until npm's trusted publishers validate. It is followed by [pipeline-v1.0.1](/quest/m1/pipeline-v1.0.1.md) to tag and bump the pin in `src/binaries.yml`.

Changes to make:

- Isolate build execution from Actions runtime upload/output surfaces and make every target artifact name attempt-scoped and immutable:
  - In `pipeline/pipeline.py` (`build()`), scrub GitHub Actions runtime and file-command environment variables (`ACTIONS_RUNTIME_TOKEN`, `ACTIONS_RESULTS_URL`, `ACTIONS_CACHE_URL`, `GITHUB_OUTPUT`, `GITHUB_ENV`, `GITHUB_PATH`, `GITHUB_STEP_SUMMARY`) from the `cargo` / `cargo-zigbuild` subprocess environment (and isolate `cargo` from runner files where feasible) so an untrusted `build.rs` or proc-macro cannot upload spoofed artifacts or inject job outputs.
  - In `.github/workflows/crabx-build.yml`, narrow `actions/upload-artifact` from `archives/*.tgz` to the single exact archive `archives/${{ matrix.name }}-${{ matrix.target }}-v${{ matrix.version }}.tgz`, name each artifact with the current run attempt (`crabx-${{ github.run_attempt }}-${{ matrix.name }}@${{ matrix.version }}@${{ matrix.target }}`), keep `overwrite: false`, and name the upload step explicitly (e.g., `Upload archive`).
- Establish trusted job-to-artifact provenance and verify the uploaded bytes' digest before unpacking:
  - Because GitHub's workflow-run artifact list API does not include a producing `job_id` and a matrix job cannot export per-leg `outputs` across N x 6 entries, enforce job-to-artifact binding via two complementary mechanisms (or switch to a per-job trusted output / job-scoped handoff if GitHub supports it):
    1. In `.github/workflows/crabx.yml`'s `publish` job, query `repos/{owner}/{repo}/actions/runs/{run_id}/attempts/{run_attempt}/jobs` and require that the expected matrix build job for every `(crate, version, target)` AND its `Upload archive` step finished with `conclusion == "success"`. Because `overwrite: false` rejects duplicate artifact names in the same run, a spoofed upload by another job causes the real target job's `Upload archive` step to fail with HTTP 409; also require that the artifact's `created_at` timestamp falls within the target job's `Upload archive` step execution window (allowing 1-second timestamp granularity) and that exactly one non-expired artifact exists for that attempt-scoped name.
    2. Download each artifact by its verified numeric `artifact.id` (instead of a run-wide wildcard `actions/download-artifact` pattern with `merge-multiple: true`), and verify GitHub's recorded `artifact.digest` against the **exact uploaded bytes** before unpacking:
       - With `actions/upload-artifact`'s default ZIP wrapping, fetch the artifact ZIP by `artifact.id`, verify its SHA-256 against `artifact.digest` **before** unzipping, then unpack and assert the ZIP contains only that target's single regular `.tgz` file (no symlinks, no extra files, no path traversal).
       - Alternatively, if using `actions/upload-artifact@v7`'s raw single-file upload (`archive: false`), verify `artifact.digest` directly against the downloaded raw `.tgz` and adapt artifact naming/download accordingly.
  - Stage only the verified `.tgz` files into a clean directory passed to `actions/attest` (`subject-path`) and `publish`.
- Exercise the real artifact handoff in read-only PR CI:
  - Extend `.github/workflows/crabx-test.yml`'s `pull_request` path with a read-only downstream job (`permissions: contents: read`) that runs the exact same artifact lookup, job/step verification, digest check, and archive extraction code path on the fixture's uploaded artifacts (without attesting or publishing), so upload/download/digest mismatches fail PR checks instead of manual releases.
- Lock Linux build-tool PyPI dependencies:
  - Pin the full transitive closure of `cargo-zigbuild` and `ziglang` with `--hash=sha256:...` in `pipeline/requirements.txt` and pass `--require-hashes` to `pip install` in `.github/workflows/crabx-build.yml`. Keep Dependabot's pip updates working or document how hashes refresh.
- Add `timeout=60` to `urllib.request.urlopen` calls in `pipeline/pipeline.py` (matching `release.py`), and set `timeout-minutes` on the jobs in `.github/workflows/crabx.yml` and `.github/workflows/crabx-build.yml`.
- Cover the per-target artifact validation (missing archive, extra `.tgz`, wrong filename, symlink, failed/missing producing job or upload step, artifact created outside upload step window, duplicate artifact, wrong attempt, ZIP/raw digest mismatch) in `pipeline/test_pipeline.py`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut on the existing `crabx-build.yml` before build-worker changes land on `main`
