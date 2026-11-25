# [M] Verify signatures and attestations before extracting archives, and bound network and staging storage

## Goal

Every published or policy-required signature and attestation is verified before crabx decompresses or extracts an archive (keeping GitHub attestation verification strictly on author releases and minisign + signed-filename verification on quickinstall, while preserving the explicit allowance for unattested author releases), and every installation and refresh enforces finite size, pagination, and time budgets across registry metadata, expanded `.crate` manifest reads, signatures, attestation responses, staged archives, and extracted files.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#758` (runner).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ ship this in `m1` for [0.2.0](/quest/m1/release-0.2.md), requiring [0.1.1](/quest/m0/release-0.1.1.md) so `0.1.1` stays musl-only.

Changes to make:

- Split download from extraction in `src/install.rs` and `vendor/binstalk-fetchers/`:
  - Stream the release archive to a temporary file inside `stage` while updating SHA-256 and any minisign `DataVerifier`, enforcing a finite per-installation archive byte budget as bytes arrive so a large or hostile download cannot fill the cache filesystem before verification runs.
  - Run the path-appropriate cryptographic verification before extracting:
    - For **quickinstall** (`fetcher.is_third_party()`): verify quickinstall's required minisign signature and signed filename against the streamed bytes before extracting; do **not** call `trust::verify` on quickinstall (which has no GitHub receipt and must continue supporting crates such as `cargo-expand@1.0.126` under `--allow-quickinstall`).
    - For **author releases** (`!fetcher.is_third_party()`): validate any configured minisign signature and run `trust::verify` on the archive's SHA-256 before extracting, preserving crabx's explicit default policy for author releases that publish no signature or attestation (such as `hyperfine@2.0.0` in nightly e2e): fail closed on any invalid/malformed/mismatched proof or API error, allow extraction only after all published or policy-required proofs pass, and record `"author release (no published attestation)"` (never mislabeling an unattested archive as verified).
  - Decompress and extract from the local archive file into `stage/download` while enforcing a finite extracted-bytes budget (defending against decompression bombs) and a finite entry budget checked before creating each file or directory, implicit parent directories included (so an archive of many empty files cannot exhaust the cache volume's inodes), then remove the staged archive temp file.
- Bound every metadata, index, expanded `.crate` manifest, and verification read path:
  - Enforce bounded response AND decompressed/in-memory budgets on **both** sparse-index and registry metadata paths: not only background refresh via `network::spool()`, but also cold installs and pinned cache misses currently using `binstalk::registry::Registry::fetch_crate_matched`. Because binstalk's registry reader decompresses the `.crate` tar stream before checking the archive checksum and reserves memory from the tar entry's declared `Cargo.toml` size, bound the compressed download size, verify the crates.io index SHA-256 before parsing, and enforce finite limits on decompressed tar traversal and `Cargo.toml` buffering before reserving memory.
  - Cap `.sig` downloads in `gh_crate_meta.rs` and `quickinstall.rs` (e.g., 64 KiB) instead of unbounded `Download::into_bytes()`.
  - Add an explicit maximum byte limit parameter to `network::spool()` (e.g., 4-8 MiB for attestation pages and external bundles, appropriate cap for sparse index) and an absolute decompressed size cap in `trust::external_bundle()` alongside the 64x Snappy ratio check.
  - Cap the attestation pagination loop in `src/trust.rs` (e.g., at most 32 pages) and wrap `trust::verify()` in a total deadline.
  - Restrict `bundle_url` scheme check in `src/trust.rs` to `"https"` only (dropping the dead `"http"` arm).
- Add unit tests for rejecting oversized compressed and expanded `.crate` / `Cargo.toml` metadata on cold-install and refresh paths, oversized archives, extracted-size bombs, archives exceeding the entry budget, oversized `.sig`/bundles, excessive pagination, verifying extraction does not run when minisign or GitHub attestation verification fails, and confirming both quickinstall (minisign-verified without `trust::verify`) and unsigned/unattested author releases extract with their expected `source` labels.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
