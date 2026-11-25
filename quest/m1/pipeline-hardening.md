# [M] Harden the reusable release pipeline against artifact and PyPI tampering

## Goal

The reusable pipeline enforces a strict cross-job boundary between its unprivileged build jobs and its privileged `publish` job (verifying that each archive came from its own successful target job), verifies SHA-256 hashes for every PyPI package installed in Linux builds, exercises the real artifact handoff in read-only PR CI, and bounds HTTP and job runtimes.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#756` (initial pipeline).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan all audit recommendations and land build-worker changes after [0.1.1](/quest/m0/release-0.1.1.md) is cut. Because `.github/workflows/release.yml`'s `check` job calls `./.github/workflows/crabx-build.yml` directly from `main`, this quest lives in `m1` and requires `release-0.1.1.md` so the 0.1.1 release stays on the proven build path until npm's trusted publishers validate. It is followed by [pipeline-v1.0.1](/quest/m1/pipeline-v1.0.1.md) to tag and bump the pin in `src/binaries.yml`.

Changes to make:

- Isolate build execution from Actions runtime upload/output surfaces and narrow uploads:
  - In `pipeline/pipeline.py` (`build()`), scrub GitHub Actions runtime environment variables (`ACTIONS_RUNTIME_TOKEN`, `ACTIONS_RESULTS_URL`, `ACTIONS_CACHE_URL`, `GITHUB_OUTPUT`, `GITHUB_ENV`, `GITHUB_PATH`, `GITHUB_STEP_SUMMARY`) from the `cargo` / `cargo-zigbuild` subprocess environment so an untrusted `build.rs` or proc-macro cannot upload spoofed artifacts or inject job outputs.
  - In `.github/workflows/crabx-build.yml`, narrow `actions/upload-artifact` from `archives/*.tgz` to the single exact archive `archives/${{ matrix.name }}-${{ matrix.target }}-v${{ matrix.version }}.tgz`.
- Bind each downloaded artifact to its producing matrix job before attesting or publishing:
  - In `.github/workflows/crabx.yml`'s `publish` job, download artifacts without `merge-multiple: true` so each target lands in its own directory.
  - In `pipeline/pipeline.py` (`provenance` / `built`), query GitHub's Actions API for the current run's jobs and artifacts to verify that:
    - the expected build matrix job for each `(crate, version, target)` succeeded (not failed or skipped),
    - each target has exactly one uploaded artifact (rejecting duplicate artifact names or unexpected artifacts), and
    - each artifact's recorded SHA-256 digest (and/or artifact ID bound to the expected matrix job) matches the downloaded `.tgz` file on disk, and each artifact directory holds only that target's single regular `.tgz` (no symlinks, no extra files).
  - Stage only the verified `.tgz` files into a clean directory passed to `actions/attest` (`subject-path`) and `publish`.
- Exercise the real artifact handoff in read-only PR CI:
  - Extend `.github/workflows/crabx-test.yml`'s `pull_request` path with a read-only downstream job (`permissions: contents: read`) that downloads the fixture's uploaded artifacts using the same pattern and runs `pipeline.py`'s layout/digest verification without attesting or publishing, so upload/download mismatches fail PR checks instead of manual releases.
- Lock Linux build-tool PyPI dependencies:
  - Pin the full transitive closure of `cargo-zigbuild` and `ziglang` with `--hash=sha256:...` in `pipeline/requirements.txt` and pass `--require-hashes` to `pip install` in `.github/workflows/crabx-build.yml`. Keep Dependabot's pip updates working or document how hashes refresh.
- Add `timeout=60` to `urllib.request.urlopen` calls in `pipeline/pipeline.py` (matching `release.py`), and set `timeout-minutes` on the jobs in `.github/workflows/crabx.yml` and `.github/workflows/crabx-build.yml`.
- Cover the per-target artifact validation (missing archive, extra `.tgz`, wrong filename, symlink, failed/missing producing job, duplicate artifact, digest mismatch) in `pipeline/test_pipeline.py`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut on the existing `crabx-build.yml` before build-worker changes land on `main`
