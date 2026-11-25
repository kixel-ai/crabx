# [S] Prevent silent attestation downgrade with a TOFU ratchet and --require-attestation

## Goal

Once crabx has verified a GitHub attestation for a crate, a later version whose release archive has no attestation fails loudly instead of silently running unverified, and callers in CI or strict environments can pass `--require-attestation` to require verified attestations on the very first run.

## Plan

Found 2026-11-24 in a pre-release security audit. Because GitHub's attestations API returns 404 for any unknown SHA-256 and `src/trust.rs` accepts 404 as unattested, an attacker who gains `contents: write` on a repository that publishes attestations can swap a release archive with an unattested malicious binary and bypass verification silently.

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan both a zero-config TOFU attestation ratchet and an explicit `--require-attestation` flag in `m1`, without blocking [0.2.0](/quest/m1/release-0.2.md).

Recommendations:

- Store the ratchet marker in a separate cache file under the crate's `author` cache root (never adding fields to existing `record.json` or `refresh.json`, which 0.1.0 rejects with `deny_unknown_fields`).
- When a crate's `author` cache has recorded a verified attestation (either via that marker or an existing cached `record.json`), refuse an unattested download with a clear error naming the crate, version, and how to proceed if the author intentionally stopped attesting (such as removing the ratchet file or passing an explicit opt-out, if added).
- Add `--require-attestation` (before the crate name, conflicting with `--build` and `--allow-quickinstall`) that refuses any release without at least one verified GitHub attestation, using a separate cache policy namespace (or checking that the cached `Record` was attested) so an unattested cached entry never satisfies `--require-attestation`.
- Document both in `README.md`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
