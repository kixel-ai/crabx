# [M] Harden the reusable release pipeline against artifact and PyPI tampering

## Goal

The reusable pipeline narrows build uploads to each target's exact archive, scrubs Actions runtime credentials from `cargo`, verifies unfiltered artifact uniqueness, pre-extraction ZIP digests, strict single-file layouts, and producing job success (within existing `v1` caller permissions) in `publish`, requires hash-pinned binary wheels for every PyPI package installed on both Linux architectures, exercises the real artifact handoff in read-only PR CI, and bounds HTTP and job runtimes.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#756` (initial pipeline).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan all audit recommendations and land build-worker changes after [0.1.1](/quest/m0/release-0.1.1.md) is cut. Because `.github/workflows/release.yml`'s `check` job calls `./.github/workflows/crabx-build.yml` directly from `main`, this quest lives in `m1` and requires `release-0.1.1.md` so the 0.1.1 release stays on the proven build path until npm's trusted publishers validate. It is followed by [pipeline-v1.0.1](/quest/m1/pipeline-v1.0.1.md) to tag and bump the pin in `src/binaries.yml`.

Decided 2026-11-24 (maintainer, [PR #30](https://github.com/kixel-ai/crabx/pull/30) checkpoints):
- ✅ Keep the single dynamic matrix in `crabx-build.yml` and state the GitHub Actions run-scoped artifact boundary plainly rather than trying to prove cryptographic per-job artifact IDs through GitHub's REST API (rejected: restructuring `crabx-build.yml` into fixed per-target or per-crate jobs for per-job outputs).
- ✅ Keep `.github/workflows/crabx.yml` strictly within existing `v1` caller permissions (`contents: write`, `id-token: write`, `attestations: write`, and `contents: read` on PR CI) so `PIPELINE_SIGNER` and Dependabot SHA bumps stay unbroken, and use ZIP-wrapped uploads consistently with `skip-decompress: true` and pre-unpack digest verification (rejected: `archive: false` raw uploads; creating a separate `crabx-v2.yml` workflow path).

Changes to make:

- Preserve the `v1` caller permission contract:
  - Keep `.github/workflows/crabx.yml` compatible with existing callers (`contents: write`, `id-token: write`, `attestations: write`, without requiring `actions: read`) and keep the PR verifier at `permissions: contents: read`.
  - Use `actions/download-artifact` (which authenticates via the Actions runtime token without needing `actions: read`) for artifact downloads. Verify in read-only PR CI (with only `contents: read`) that public-repo reads of `actions/runs/{run_id}/artifacts` and `actions/runs/{run_id}/jobs` succeed without `actions: read`; if GitHub denies `actions/runs/{run_id}/jobs` without `actions: read`, keep the `v1` permission contract intact rather than adding `actions: read` to `crabx.yml`.
- Isolate `cargo` from Actions runtime env vars and narrow uploads:
  - In `pipeline/pipeline.py` (`build()`), scrub GitHub Actions runtime and file-command environment variables (`ACTIONS_RUNTIME_TOKEN`, `ACTIONS_RESULTS_URL`, `ACTIONS_CACHE_URL`, `GITHUB_OUTPUT`, `GITHUB_ENV`, `GITHUB_PATH`, `GITHUB_STEP_SUMMARY`) from the `cargo` / `cargo-zigbuild` subprocess environment so an untrusted `build.rs` or proc-macro has no direct access to Actions artifact or step-output APIs via its environment.
  - In `.github/workflows/crabx-build.yml`, narrow `actions/upload-artifact` from `archives/*.tgz` to the single exact archive `archives/${{ matrix.name }}-${{ matrix.target }}-v${{ matrix.version }}.tgz` with `overwrite: false` and default ZIP wrapping, keeping the artifact name `crabx-${{ matrix.name }}@${{ matrix.version }}@${{ matrix.target }}` so rerunning a failed `publish` job still reuses successful build artifacts from the prior attempt.
- Verify unfiltered artifact uniqueness, job success, pre-extraction ZIP digests, and strict layout in `publish`:
  - Query the **unfiltered** paginated workflow-run artifact inventory (`repos/{owner}/{repo}/actions/runs/{run_id}/artifacts`) before selecting any download (because `actions/download-artifact` deduplicates same-named artifacts by keeping only the latest record). Reject any run where more than one non-expired artifact shares an expected target name `crabx-<name>@<version>@<target>` or where any expected target is missing, and record each target's unique `artifact.id` and `artifact.digest`.
  - Query the run's jobs and require that every expected `(crate, version, target)` matrix build job's latest execution in the run completed with `conclusion == "success"` (so a failed build cannot be masked while rerunning only a failed `publish` job still sees the successful build jobs from the previous attempt).
  - Download each target into its own **explicit per-target directory** (for example, `archives/<target>/`) using `actions/download-artifact` with `skip-decompress: true` (so a lone artifact in a single-missing-target backfill lands in its explicit target directory and is never extracted before verification).
  - Verify the downloaded ZIP's SHA-256 against `artifact.digest` **before** unpacking it, then unpack and assert the ZIP holds only that target's single regular `.tgz` file (no symlinks, no extra files, no path traversal).
  - Stage only the verified `.tgz` files into a clean directory passed to `actions/attest` (`subject-path`) and `publish`.
- Exercise the real artifact handoff in read-only PR CI:
  - Extend `.github/workflows/crabx-test.yml`'s `pull_request` path with a read-only downstream job (`permissions: contents: read`) that runs the exact same unfiltered artifact inventory check, job-status check, per-target `skip-decompress: true` download, pre-extraction ZIP digest check, and archive validation on the fixture's uploaded artifacts (without attesting or publishing), so permission or upload/download/digest mismatches fail PR checks instead of manual releases.
- Lock Linux build-tool PyPI dependencies with binary wheels only on both Linux architectures (`x86_64` and `aarch64`):
  - Pin the full transitive closure of `cargo-zigbuild` and `ziglang` with `--hash=sha256:...` for both Linux architectures' wheels in `pipeline/requirements.txt` and pass `--require-hashes --only-binary=:all:` to `pip install` in `.github/workflows/crabx-build.yml`, so pip can never fall back to an sdist and install unhashed build dependencies inside an isolated build environment. Keep Dependabot's pip updates working or document how hashes refresh.
- Add `timeout=60` to `urllib.request.urlopen` calls in `pipeline/pipeline.py` (matching `release.py`), and set `timeout-minutes` on the jobs in `.github/workflows/crabx.yml` and `.github/workflows/crabx-build.yml`.
- Update [pipeline/README.md](/pipeline/README.md)'s release checklist to name `.github/workflows/release.yml` alongside `src/binaries.yml` and `pipeline/README.md` so all three pins live in one checklist.
- Cover the per-target artifact validation (single-missing-target backfill, missing archive, extra `.tgz`, wrong filename, symlink, failed/missing producing job, partial-rerun reuse of a prior attempt's successful build job, duplicate artifact in unfiltered inventory, pre-extraction ZIP digest mismatch) in `pipeline/test_pipeline.py`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut on the existing `crabx-build.yml` before build-worker changes land on `main`
