# [S] Prevent silent attestation downgrade with a TOFU ratchet and --require-attestation

## Goal

Once crabx has verified a GitHub attestation for a crate's author release in any mode (including attested entries already present in either policy-versioned or legacy 0.1.x caches at migration time), no unattested author release for that crate can execute (unpinned runs select the newest eligible attested version while pinned runs of unattested versions fail loudly), and callers in CI or strict environments can pass `--require-attestation` (carried into background updates) to require verified attestations from the very first run.

## Plan

Found 2026-11-24 in a pre-release security audit. Because GitHub's attestations API returns 404 for any unknown SHA-256 and `src/trust.rs` accepts 404 as unattested, an attacker who gains `contents: write` on a repository that publishes attestations can swap a release archive with an unattested malicious binary and bypass verification silently.

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan both a zero-config TOFU attestation ratchet and an explicit `--require-attestation` flag in `m1`, without blocking [0.2.0](/quest/m1/release-0.2.md).

Decided 2026-11-24 (maintainer, [PR #30](https://github.com/kixel-ai/crabx/pull/30) checkpoint): ✅ seed the ratchet with a single one-time migration across both policy-versioned and legacy cache namespaces and record a fixed-size `<crate>.migrated` marker so warm and pinned runs stay O(1) (rejected: mtime-based rescanning of legacy 0.1.x directories for post-migration writes from a downgraded 0.1.x binary, which is unreliable on coarse-mtime filesystems and adds per-run complexity). Document clearly that the ratchet covers all attestations already cached at migration time and everything verified going forward, not later writes by a downgraded 0.1.x binary.

Recommendations:

- Make crate-level ratchet state monotonic across all install modes so a negative migration scan can never clear an active ratchet:
  - Store the positive attested marker in a dedicated write-once file outside mode-specific subdirectories (for example, `crabx/v1/<host>/attested/<crate>.attested`, never adding fields to legacy `record.json` or `refresh.json`), created atomically under a shared per-crate ratchet lock whenever any mode (`author`, `--allow-quickinstall` when an author release wins, or `--require-attestation`) verifies a GitHub attestation for an author release of that crate.
  - Store one-time migration completion in a separate fixed-size marker file (for example, `crabx/v1/<host>/attested/<crate>.migrated`) that never deletes or overrides `<crate>.attested`, and serialize migration and positive-marker updates under the shared per-crate ratchet lock so concurrent invocations across modes cannot race.
- Perform one-time initialization across both current and legacy cache namespaces, keeping warm and pinned runs O(1):
  - Because [attestation-binding](/quest/m1/attestation-binding.md) publishes 0.2.0 entries in policy-versioned namespaces (`author-v2`, `quickinstall-v2`) and keeps legacy 0.1.x `author/` and `quickinstall/` cache directories read-only and intact, both sets of cached records remain available even if 0.2.0 ships before this quest.
  - On any run, check `<crate>.attested` first (O(1) stat): if present, the ratchet is active.
  - Otherwise, if `<crate>.migrated` already exists (O(1) stat), skip scanning.
  - Otherwise, acquire the shared per-crate ratchet lock, re-check `<crate>.attested` and `<crate>.migrated`, and scan every cached version directory for that crate across both the current policy-versioned namespaces (`author-v2`, `quickinstall-v2`) and the legacy 0.1.x namespaces (`author`, `quickinstall`). If any entry's `record.json` records a GitHub attestation (`source` starting with `"GitHub attestations"`), create `<crate>.attested`; then create `<crate>.migrated`.
- Enforce the ratchet on both **downloads** and **warm-cache selection / execution**:
  - On an **unpinned** run when the ratchet is active and both attested `1.0.0` and unattested `2.0.0` are cached, select the newest eligible attested version (`1.0.0`) rather than failing while a usable verified release is cached (and fail the background update of unattested `2.0.0` with a diagnostic).
  - On a **pinned** run (such as `crate@2.0.0`) or when no attested cached version is eligible, refuse the unattested release loudly with an error naming the crate, version, and how to proceed if the author intentionally stopped attesting.
- Add `--require-attestation` (before the crate name, conflicting with `--build` and `--allow-quickinstall`):
  - Refuse any release without at least one verified GitHub attestation, using a separate trust-policy cache namespace (or enforcing that any cached `Record` served under `--require-attestation` carries verified attestation proof) so an unattested cached entry never satisfies `--require-attestation`.
  - Forward `--require-attestation` in `network::spawn_refresh` so detached background updates preserve the effective strict policy and update the matching cache namespace.
- Add regression tests covering: warm-cache ratchet selection (unpinned run picks attested `1.0.0` over cached unattested `2.0.0`, while pinned `crate@2.0.0` fails), monotonic state updates (a negative migration scan cannot clear an active `<crate>.attested` marker), one-time initialization scanning both policy-versioned (`author-v2`/`quickinstall-v2`) and legacy 0.1.x (`author`/`quickinstall`) caches, O(1) warm/pinned fast path once `<crate>.migrated` exists, cross-mode ratchet sharing, and `--require-attestation` handoff into `--refresh`.
- Document both the ratchet (including its one-time migration boundary) and `--require-attestation` in `README.md`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
- [Attestation binding](/quest/m1/attestation-binding.md) - keeps legacy 0.1.x cache directories read-only so upgrade history is preserved for the ratchet
