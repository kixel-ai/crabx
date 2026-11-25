# [M] Harden the reusable release pipeline against artifact and PyPI tampering

## Goal

The reusable pipeline narrows build uploads to each target's exact archive, scrubs Actions runtime credentials from `cargo`, verifies producing job success, unfiltered artifact uniqueness, pre-extraction uploaded digests, and strict single-file layouts in `publish` (preserving partial reruns of `publish`, single-target backfills, and existing `crabx.yml` caller permissions), verifies SHA-256 hashes for every PyPI package installed in Linux builds, exercises the real artifact handoff in read-only PR CI, and bounds HTTP and job runtimes.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#756` (initial pipeline).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan all audit recommendations and land build-worker changes after [0.1.1](/quest/m0/release-0.1.1.md) is cut. Because `.github/workflows/release.yml`'s `check` job calls `./.github/workflows/crabx-build.yml` directly from `main`, this quest lives in `m1` and requires `release-0.1.1.md` so the 0.1.1 release stays on the proven build path until npm's trusted publishers validate. It is followed by [pipeline-v1.0.1](/quest/m1/pipeline-v1.0.1.md) to tag and bump the pin in `src/binaries.yml`.

Decided 2026-11-24 (maintainer, [PR #30](https://github.com/kixel-ai/crabx/pull/30) checkpoint): ✅ keep the single dynamic matrix in `crabx-build.yml` and state the GitHub Actions run-scoped artifact boundary plainly rather than trying to prove cryptographic per-job artifact IDs through GitHub's REST API (rejected: restructuring `crabx-build.yml` into fixed per-target or per-crate jobs for per-job outputs). GitHub's workflow-run artifacts API does not expose a producing `job_id`, and all six targets of a crate already compile the same crate's `build.rs` and `Cargo.lock`; the safeguards below stop `archives/*.tgz` globbing, failed-target masking, duplicate or unexpected files, unhashed PyPI installs, and stalled calls.

Changes to make:

- Preserve the `crabx.yml` caller permission contract so Dependabot SHA bumps on downstream repositories can never break existing callers:
  - A reusable workflow cannot raise permissions its caller withheld, and Dependabot updates commit-SHA pins (`uses: kixel-ai/crabx/.github/workflows/crabx.yml@<sha>`) without honoring major-version comments. Therefore `.github/workflows/crabx.yml` must remain compatible with existing callers' permissions (`contents: write`, `id-token: write`, `attestations: write`, without `actions: read`), and PR CI's verifier must stay read-only (`contents: read`).
  - Verify in read-only PR CI (with only `contents: read`) that public-repo `actions/runs/{run_id}/jobs` and unfiltered `actions/runs/{run_id}/artifacts` reads work without `actions: read`, and use the Actions runtime token (`actions/download-artifact`) for artifact downloads so no caller needs `actions: read`.
  - If any required lookup turns out to require `actions: read`, do NOT break `.github/workflows/crabx.yml`: keep `crabx.yml` within the `v1` permission contract, or introduce a separate versioned workflow path (such as `.github/workflows/crabx-v2.yml`) for callers that opt in with `actions: read`.
- Isolate `cargo` from Actions runtime env vars and narrow uploads:
  - In `pipeline/pipeline.py` (`build()`), scrub GitHub Actions runtime and file-command environment variables (`ACTIONS_RUNTIME_TOKEN`, `ACTIONS_RESULTS_URL`, `ACTIONS_CACHE_URL`, `GITHUB_OUTPUT`, `GITHUB_ENV`, `GITHUB_PATH`, `GITHUB_STEP_SUMMARY`) from the `cargo` / `cargo-zigbuild` subprocess environment so an untrusted `build.rs` or proc-macro has no direct access to Actions artifact or step-output APIs via its environment.
  - In `.github/workflows/crabx-build.yml`, narrow `actions/upload-artifact` from `archives/*.tgz` to the single exact archive `archives/${{ matrix.name }}-${{ matrix.target }}-v${{ matrix.version }}.tgz` with `overwrite: false`, keeping the artifact name `crabx-${{ matrix.name }}@${{ matrix.version }}@${{ matrix.target }}` so rerunning a failed `publish` job still reuses successful build artifacts from the prior attempt.
- Verify build-job success, unfiltered artifact uniqueness, pre-extraction digests, and strict layout in `publish`:
  - Check uniqueness against the **unfiltered** Actions workflow-run artifact inventory (`repos/{owner}/{repo}/actions/runs/{run_id}/artifacts`, paginated) **before** selecting any download, because `actions/download-artifact` deduplicates same-named artifacts by keeping only the latest record. Reject any run where more than one non-expired artifact shares an expected target name or where any expected target is missing, and resolve the exact single numeric `artifact.id` and `artifact.digest` for each expected target.
  - Query the run's jobs and require that every expected `(crate, version, target)` matrix build job's latest execution in the run completed with `conclusion == "success"` (so a failed build cannot be masked even if another job produced a file, while rerunning only a failed `publish` job still sees the successful build jobs from the previous attempt).
  - Download each target into its own **explicit per-target directory** (for example, `archives/<target>/`) by its verified `artifact.id` rather than a wildcard pattern: `actions/download-artifact` writes a lone artifact directly into `path` even with `merge-multiple: false`, so an explicit per-target destination avoids breaking single-missing-target backfills.
  - Verify `artifact.digest` against the **exact uploaded bytes before extraction** on every download path (including the runtime-token `actions/download-artifact` path, which otherwise streams a ZIP into the extractor before checking its digest):
    - either upload raw single files (`archive: false` on `actions/upload-artifact@v7`) and verify `artifact.digest` directly against each downloaded `.tgz`, or
    - keep ZIP wrapping, download without extracting (`skip-decompress: true` on `actions/download-artifact` or raw ZIP fetch), verify the downloaded ZIP's SHA-256 against `artifact.digest` **before** unpacking, and then unpack and assert the ZIP holds only that target's single regular `.tgz` file (no symlinks, no extra files, no path traversal).
  - Stage only the verified `.tgz` files into a clean directory passed to `actions/attest` (`subject-path`) and `publish`.
- Exercise the real artifact handoff in read-only PR CI:
  - Extend `.github/workflows/crabx-test.yml`'s `pull_request` path with a read-only downstream job (`permissions: contents: read`) that runs the exact same unfiltered artifact inventory check, job-status check, per-target download, pre-extraction digest check, and archive validation on the fixture's uploaded artifacts (without attesting or publishing), so permission or upload/download/digest mismatches fail PR checks instead of manual releases.
- Lock Linux build-tool PyPI dependencies:
  - Pin the full transitive closure of `cargo-zigbuild` and `ziglang` with `--hash=sha256:...` in `pipeline/requirements.txt` and pass `--require-hashes` to `pip install` in `.github/workflows/crabx-build.yml`. Keep Dependabot's pip updates working or document how hashes refresh.
- Add `timeout=60` to `urllib.request.urlopen` calls in `pipeline/pipeline.py` (matching `release.py`), and set `timeout-minutes` on the jobs in `.github/workflows/crabx.yml` and `.github/workflows/crabx-build.yml`.
- Cover the per-target artifact validation (single-missing-target backfill, missing archive, extra `.tgz`, wrong filename, symlink, failed/missing producing job, partial-rerun reuse of a prior attempt's successful build job, duplicate artifact in unfiltered inventory, pre-extraction ZIP/raw digest mismatch) in `pipeline/test_pipeline.py`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut on the existing `crabx-build.yml` before build-worker changes land on `main`
