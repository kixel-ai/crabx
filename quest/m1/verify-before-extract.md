# [M] Verify signatures and attestations before extracting archives, and bound network and staging storage

## Goal

Every published or policy-required signature and attestation is verified before crabx decompresses or extracts an archive (while preserving the explicit allowance for unattested author releases), and every installation and refresh enforces finite size, pagination, and time budgets across registry metadata, sparse-index fetches, signatures, attestation responses, staged archives, and extracted files.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#758` (runner).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ ship this in `m1` for [0.2.0](/quest/m1/release-0.2.md), requiring [0.1.1](/quest/m0/release-0.1.1.md) so `0.1.1` stays musl-only.

Changes to make:

- Split download from extraction in `src/install.rs` and `vendor/binstalk-fetchers/`:
  - Stream the release archive to a temporary file inside `stage` while updating SHA-256 and any minisign `DataVerifier`, enforcing a finite per-installation archive byte budget as bytes arrive so a large or hostile download cannot fill the cache filesystem before verification runs.
  - Validate the minisign signature (when configured by the crate or required by quickinstall) and run `trust::verify` on the archive's SHA-256 first.
  - Preserve crabx's explicit default policy for author releases that publish no signature or attestation (such as `hyperfine@2.0.0` in nightly e2e): fail closed on any invalid/malformed/mismatched proof or API error, and allow extraction only after all published or policy-required proofs pass, recording `"author release (no published attestation)"` (never mislabeling an unattested archive as verified).
  - Decompress and extract from the local archive file into `stage/download` while enforcing a finite extracted-bytes budget (defending against decompression bombs), then remove the staged archive temp file.
- Bound every metadata, index, and verification download path:
  - Enforce bounded response sizes on **both** sparse-index and registry metadata paths: not only background refresh via `network::spool()`, but also cold installs and pinned cache misses that currently call `binstalk::registry::Registry::fetch_crate_matched` (bounding both the sparse-index response and the packaged `.crate` manifest download at the registry-fetch boundary so neither can exceed the install budget).
  - Cap `.sig` downloads in `gh_crate_meta.rs` and `quickinstall.rs` (e.g., 64 KiB) instead of unbounded `Download::into_bytes()`.
  - Add an explicit maximum byte limit parameter to `network::spool()` (e.g., 4-8 MiB for attestation pages and external bundles, appropriate cap for sparse index) and an absolute decompressed size cap in `trust::external_bundle()` alongside the 64x Snappy ratio check.
  - Cap the attestation pagination loop in `src/trust.rs` (e.g., at most 32 pages) and wrap `trust::verify()` in a total deadline.
  - Restrict `bundle_url` scheme check in `src/trust.rs` to `"https"` only (dropping the dead `"http"` arm).
- Add unit tests for rejecting oversized registry/index metadata on both cold-install and refresh paths, oversized archives, extracted-size bombs, oversized `.sig`/bundles, excessive pagination, verifying extraction does not run when signature or attestation verification fails, and confirming unsigned/unattested author releases still extract with the explicit unattested `source` label.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
