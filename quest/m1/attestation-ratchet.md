# [S] Prevent silent attestation downgrade with a TOFU ratchet and --require-attestation

## Goal

Once crabx has verified a GitHub attestation for a crate's author release in any mode (including preserved legacy 0.1.x cache history), no unattested author release for that crate can execute (unpinned runs select the newest eligible attested version while pinned runs of unattested versions fail loudly), and callers in CI or strict environments can pass `--require-attestation` (carried into background updates) to require verified attestations from the very first run.

## Plan

Found 2026-11-24 in a pre-release security audit. Because GitHub's attestations API returns 404 for any unknown SHA-256 and `src/trust.rs` accepts 404 as unattested, an attacker who gains `contents: write` on a repository that publishes attestations can swap a release archive with an unattested malicious binary and bypass verification silently.

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan both a zero-config TOFU attestation ratchet and an explicit `--require-attestation` flag in `m1`, without blocking [0.2.0](/quest/m1/release-0.2.md).

Recommendations:

- Store the crate-level ratchet state in a shared per-crate file outside mode-specific subdirectories (for example, under `crabx/v1/<host>/attested/<crate>.json`, never adding fields to legacy `record.json` or `refresh.json`), updated whenever any mode (`author`, `--allow-quickinstall` when an author release wins, or `--require-attestation`) verifies a GitHub attestation for an author release of that crate.
- Perform one-time migration from preserved legacy cache history without O(versions) work on every warm run:
  - Because [attestation-binding](/quest/m1/attestation-binding.md) publishes 0.2.0 entries in a policy-versioned cache namespace and keeps legacy 0.1.x `author/` and `quickinstall/` cache directories read-only and intact, legacy attested `record.json` files remain available even if 0.2.0 runs before this quest ships.
  - When the shared crate-level ratchet file is absent, perform a one-time scan of both the current and legacy read-only cache namespaces for that crate (`author`, `author-v2`, `quickinstall`, `quickinstall-v2` where `source` starts with `"GitHub attestations"`):
    - If any entry records a verified GitHub attestation (revalidating or treating legacy attested records as activating the ratchet so an unattested newer version cannot sneak through), write `{"attested": true}` to the shared file.
    - If no historical attestation exists, write `{"attested": false}` to record migration completion so subsequent warm and pinned runs do an O(1) read instead of rescanning every cached version, upgrading the file to `{"attested": true}` if a later install verifies an attestation.
- Enforce the ratchet on both **downloads** and **warm-cache selection / execution**:
  - On an **unpinned** run when the ratchet is active and both attested `1.0.0` and unattested `2.0.0` are cached, select the newest eligible attested version (`1.0.0`) rather than failing while a usable verified release is cached (and fail the background update of unattested `2.0.0` with a diagnostic).
  - On a **pinned** run (such as `crate@2.0.0`) or when no attested cached version is eligible, refuse the unattested release loudly with an error naming the crate, version, and how to proceed if the author intentionally stopped attesting.
- Add `--require-attestation` (before the crate name, conflicting with `--build` and `--allow-quickinstall`):
  - Refuse any release without at least one verified GitHub attestation, using a separate trust-policy cache namespace (or enforcing that any cached `Record` served under `--require-attestation` carries verified attestation proof) so an unattested cached entry never satisfies `--require-attestation`.
  - Forward `--require-attestation` in `network::spawn_refresh` so detached background updates preserve the effective strict policy and update the matching cache namespace.
- Add regression tests covering: warm-cache ratchet selection (unpinned run picks attested `1.0.0` over cached unattested `2.0.0`, while pinned `crate@2.0.0` fails), one-time migration completion for unattested crates (no repeated scans on subsequent warm/pinned runs), upgrade initialization from preserved read-only legacy 0.1.x attested `1.0.0` rejecting unattested `2.0.0`, cross-mode ratchet sharing, and `--require-attestation` handoff into `--refresh`.
- Document both in `README.md`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
- [Attestation binding](/quest/m1/attestation-binding.md) - keeps legacy 0.1.x cache directories read-only so upgrade history is preserved for the ratchet
