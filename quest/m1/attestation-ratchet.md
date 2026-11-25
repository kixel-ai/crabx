# [S] Prevent silent attestation downgrade with a TOFU ratchet and --require-attestation

## Goal

Once crabx has verified a GitHub attestation for a crate's author release in any mode (including legacy 0.1.x cache entries present before or added after upgrade), no unattested author release for that crate can execute (unpinned runs select the newest eligible attested version while pinned runs of unattested versions fail loudly), and callers in CI or strict environments can pass `--require-attestation` (carried into background updates) to require verified attestations from the very first run.

## Plan

Found 2026-11-24 in a pre-release security audit. Because GitHub's attestations API returns 404 for any unknown SHA-256 and `src/trust.rs` accepts 404 as unattested, an attacker who gains `contents: write` on a repository that publishes attestations can swap a release archive with an unattested malicious binary and bypass verification silently.

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan both a zero-config TOFU attestation ratchet and an explicit `--require-attestation` flag in `m1`, without blocking [0.2.0](/quest/m1/release-0.2.md).

Recommendations:

- Make crate-level ratchet state monotonic across all install modes so a negative migration scan can never clear an active ratchet:
  - Store the positive attested marker in a dedicated write-once file outside mode-specific subdirectories (for example, `crabx/v1/<host>/attested/<crate>.attested`, never adding fields to legacy `record.json` or `refresh.json`), created atomically under a shared per-crate ratchet lock whenever any mode (`author`, `--allow-quickinstall` when an author release wins, or `--require-attestation`) verifies a GitHub attestation for an author release of that crate.
  - Store negative migration state in a separate file (for example, `crabx/v1/<host>/attested/<crate>.migrated.json`) that never deletes or overrides `<crate>.attested`, and serialize migration and positive-marker updates under the shared per-crate ratchet lock so concurrent invocations across modes cannot race.
- Perform O(1) warm-run checks while still catching legacy 0.1.x attested entries written before or after upgrade:
  - Because [attestation-binding](/quest/m1/attestation-binding.md) publishes 0.2.0 entries in a policy-versioned cache namespace and keeps legacy 0.1.x `author/` and `quickinstall/` cache directories read-only and intact, legacy attested `record.json` files remain available even if 0.2.0 runs before this quest ships.
  - When `<crate>.attested` already exists, warm and pinned runs immediately know the ratchet is active in O(1) without scanning legacy directories.
  - Otherwise, record in `<crate>.migrated.json` the already-scanned legacy crate directories' snapshot (each legacy `author/<crate>` and `quickinstall/<crate>` directory's mtime and scanned version directory names). On subsequent runs when `<crate>.attested` is absent, if the legacy crate directories do not exist or their mtimes match the recorded snapshot, skip opening any `record.json` (O(1) stat); if a concurrent or rolled-back 0.1.x process later adds a new legacy version directory (changing the directory mtime/listing), scan only the newly added legacy entries, activating `<crate>.attested` immediately if any records `"GitHub attestations"` or updating `<crate>.migrated.json` otherwise.
- Enforce the ratchet on both **downloads** and **warm-cache selection / execution**:
  - On an **unpinned** run when the ratchet is active and both attested `1.0.0` and unattested `2.0.0` are cached, select the newest eligible attested version (`1.0.0`) rather than failing while a usable verified release is cached (and fail the background update of unattested `2.0.0` with a diagnostic).
  - On a **pinned** run (such as `crate@2.0.0`) or when no attested cached version is eligible, refuse the unattested release loudly with an error naming the crate, version, and how to proceed if the author intentionally stopped attesting.
- Add `--require-attestation` (before the crate name, conflicting with `--build` and `--allow-quickinstall`):
  - Refuse any release without at least one verified GitHub attestation, using a separate trust-policy cache namespace (or enforcing that any cached `Record` served under `--require-attestation` carries verified attestation proof) so an unattested cached entry never satisfies `--require-attestation`.
  - Forward `--require-attestation` in `network::spawn_refresh` so detached background updates preserve the effective strict policy and update the matching cache namespace.
- Add regression tests covering: warm-cache ratchet selection (unpinned run picks attested `1.0.0` over cached unattested `2.0.0`, while pinned `crate@2.0.0` fails), monotonic state updates (a negative migration scan cannot clear an active `<crate>.attested` marker), post-migration legacy invalidation (a 0.1.x attested entry added after `<crate>.migrated.json` was written activates the ratchet on the next run), O(1) fast path when legacy dirs are unchanged, upgrade initialization from preserved read-only legacy 0.1.x attested `1.0.0`, cross-mode ratchet sharing, and `--require-attestation` handoff into `--refresh`.
- Document both in `README.md`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
- [Attestation binding](/quest/m1/attestation-binding.md) - keeps legacy 0.1.x cache directories read-only so upgrade history is preserved for the ratchet
