# [M] Harden the reusable release pipeline against artifact and PyPI tampering

## Goal

The reusable pipeline enforces a strict cross-job boundary between its unprivileged build jobs and its privileged `publish` job, verifies SHA-256 hashes for every PyPI package installed in Linux builds, and bounds HTTP and job runtimes so stalled external calls fail promptly.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#756` (initial pipeline).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan all audit recommendations and land them without blocking [0.1.1](/quest/m0/release-0.1.1.md). This quest touches only `pipeline/` and `.github/workflows/crabx*.yml`, not the Rust crate, so it joins `m0` and is followed by [pipeline-v1.0.1](/quest/m1/pipeline-v1.0.1.md) to tag and bump the pin in `src/binaries.yml`.

Changes to make:

- Narrow `.github/workflows/crabx-build.yml`'s `actions/upload-artifact` path from `archives/*.tgz` to the single exact archive `archives/${{ matrix.name }}-${{ matrix.target }}-v${{ matrix.version }}.tgz`. Today an unprivileged `build.rs` or proc-macro on one target can write extra `.tgz` files (including another target's filename) into `archives/`, getting rogue archives attested or overriding another target's archive.
- In `.github/workflows/crabx.yml`'s `publish` job, download build artifacts without `merge-multiple: true` so each target lands in its own artifact directory. In `pipeline/pipeline.py` (`provenance` / `built`), verify that each expected target's directory holds only that target's single regular `.tgz` file (no symlinks, no extra files), reject any unexpected artifact directories, and stage only those verified files into the directory passed to `actions/attest` (`subject-path`) and `publish`.
- Pin the full transitive closure of `cargo-zigbuild` and `ziglang` with `--hash=sha256:...` in `pipeline/requirements.txt` and pass `--require-hashes` to `pip install` in `.github/workflows/crabx-build.yml`. Keep Dependabot's pip updates working or document how hashes refresh.
- Add `timeout=60` to `urllib.request.urlopen` calls in `pipeline/pipeline.py` (matching `release.py`), and set `timeout-minutes` on the jobs in `.github/workflows/crabx.yml` and `.github/workflows/crabx-build.yml`.
- Cover the per-target artifact validation (missing archive, extra `.tgz`, wrong filename, symlink) in `pipeline/test_pipeline.py`.
