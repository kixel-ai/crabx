# [M] Verify signatures and attestations before extracting archives, and bound network reads

## Goal

crabx never passes unverified archive bytes to decompressors or extractors, and every network response (signatures, attestation pages, external bundles, pagination, and sparse index) has bounded size and total time.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#758` (runner).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ ship this in `m1` for [0.2.0](/quest/m1/release-0.2.md), requiring [0.1.1](/quest/m0/release-0.1.1.md) so `0.1.1` stays musl-only.

Changes to make:

- Split download from extraction in `src/install.rs` and `vendor/binstalk-fetchers/`:
  - Stream the archive to a temporary file inside `stage` while updating SHA-256 and any minisign `DataVerifier`.
  - Validate the minisign signature (for author minisign and quickinstall) and run `trust::verify` on the archive's SHA-256 first.
  - Only after verification succeeds, decompress and extract the verified local file into `stage/download`, so C decompressors (`liblzma-sys`, `bzip2-sys`, `zstd-sys`) and archive extractors never see unverified bytes on signed/attested releases, and remove the downloaded archive temp file afterwards.
- Bound all network reads and loops:
  - Cap `.sig` downloads in `gh_crate_meta.rs` and `quickinstall.rs` (e.g., 64 KiB) instead of unbounded `Download::into_bytes()`.
  - Add a maximum byte limit parameter to `network::spool()` (e.g., 4-8 MiB for attestation pages and external bundles, appropriate cap for sparse index) and an absolute decompressed size cap in `trust::external_bundle()` alongside the 64x Snappy ratio check.
  - Cap the attestation pagination loop in `src/trust.rs` (e.g., at most 32 pages) and wrap `trust::verify()` in a total deadline.
  - Restrict `bundle_url` scheme check in `src/trust.rs` to `"https"` only (dropping the dead `"http"` arm).
- Add unit tests for rejecting oversized `.sig`/bundles, excessive pagination, and verifying that extraction does not run when signature or attestation verification fails.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
