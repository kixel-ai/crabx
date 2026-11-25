# [S] Prevent silent attestation downgrade with a TOFU ratchet and --require-attestation

## Goal

Once crabx has verified a GitHub attestation for a crate's author release in any mode, no unattested author release for that crate can execute (whether freshly downloaded or already warm in the cache), and callers in CI or strict environments can pass `--require-attestation` (carried into background updates) to require verified attestations from the very first run.

## Plan

Found 2026-11-24 in a pre-release security audit. Because GitHub's attestations API returns 404 for any unknown SHA-256 and `src/trust.rs` accepts 404 as unattested, an attacker who gains `contents: write` on a repository that publishes attestations can swap a release archive with an unattested malicious binary and bypass verification silently.

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan both a zero-config TOFU attestation ratchet and an explicit `--require-attestation` flag in `m1`, without blocking [0.2.0](/quest/m1/release-0.2.md).

Recommendations:

- Store the crate-level ratchet marker in a shared per-crate location outside mode-specific subdirectories (for example, under `crabx/v1/<host>/attested/<crate>`, never adding fields to `record.json` or `refresh.json`, which 0.1.0 rejects with `deny_unknown_fields`), updated whenever any mode (`author`, `--allow-quickinstall` when an author release wins, or `--require-attestation`) verifies a GitHub attestation for an author release of that crate.
- Enforce the ratchet on both **downloads** and **warm-cache hits** (including cache selection for unpinned runs):
  - If version `2.0.0` entered the cache unattested and a later pinned run verifies an attestation on `1.0.0`, a subsequent run must refuse to execute the cached unattested `2.0.0` (failing loudly with an error naming the crate, version, and how to proceed if the author intentionally stopped attesting).
- Add `--require-attestation` (before the crate name, conflicting with `--build` and `--allow-quickinstall`):
  - Refuse any release without at least one verified GitHub attestation, using a separate trust-policy cache namespace (or enforcing that any cached `Record` served under `--require-attestation` carries verified attestation proof) so an unattested cached entry never satisfies `--require-attestation`.
  - Forward `--require-attestation` in `network::spawn_refresh` so detached background updates preserve the effective strict policy and update the right cache namespace.
- Add regression tests covering: warm-cache ratchet enforcement (cached unattested 2.0.0 rejected after verifying attested 1.0.0), cross-mode ratchet sharing (`--require-attestation` or `--allow-quickinstall` author verification activating the ratchet for default runs), and `--require-attestation` handoff into `--refresh`.
- Document both in `README.md`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
