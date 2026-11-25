# [S] Prevent silent attestation downgrade with a TOFU ratchet and --require-attestation

## Goal

Once crabx has verified a GitHub attestation for a crate's author release in any mode (including attested entries already present in either `crabx/v2/` or legacy `crabx/v1/` caches at migration time), no unattested author release for that crate can execute (unpinned runs select the newest eligible attested version while pinned runs of unattested versions fail loudly), and callers in CI or strict environments can pass `--require-attestation` (carried into background updates) to require verified attestations from the very first run.

## Plan

Found 2026-11-24 in a pre-release security audit. Because GitHub's attestations API returns 404 for any unknown SHA-256 and `src/trust.rs` accepts 404 as unattested, an attacker who gains `contents: write` on a repository that publishes attestations can swap a release archive with an unattested malicious binary and bypass verification silently.

Decided 2026-11-24 (maintainer, /quest-plan): ✅ plan both a zero-config TOFU attestation ratchet and an explicit `--require-attestation` flag in `m1`, without blocking [0.2.0](/quest/m1/release-0.2.md).

Decided 2026-11-24 (maintainer, [PR #30](https://github.com/kixel-ai/crabx/pull/30) checkpoint): ✅ seed the ratchet with a single one-time migration across both `crabx/v2/` and legacy `crabx/v1/` cache namespaces and record a fixed-size `<crate>.migrated` marker so warm and pinned runs stay O(1) (rejected: mtime-based rescanning of legacy directories for post-migration writes from pre-ratchet binaries, which is unreliable on coarse-mtime filesystems and adds per-run complexity). Document clearly that the one-time scan covers all attestations already cached at migration time and everything verified going forward, excluding later writes by any pre-ratchet version (including both 0.1.x and pre-ratchet 0.2.0).

Recommendations:

- Make crate-level ratchet state monotonic and serialized across all install modes so neither a negative migration scan nor a concurrent unattested download/warm hit can bypass an active ratchet:
  - Store the positive attested marker in a dedicated write-once file outside mode-specific subdirectories (for example, `crabx/v2/<host>/attested/<crate>.attested`), created atomically under a shared per-crate ratchet lock whenever any mode (`author`, `--allow-quickinstall` when an author release wins, or `--require-attestation`) verifies a GitHub attestation for an author release of that crate.
  - Store one-time migration completion in a separate fixed-size marker file (`crabx/v2/<host>/attested/<crate>.migrated`) that never deletes or overrides `<crate>.attested`.
  - Acquire the shared per-crate ratchet lock for **final release admission** on both paths: (1) before publishing/admitting a freshly downloaded unattested release (re-checking `<crate>.attested` under the lock so a concurrent activation in another mode rejects it), and (2) before admitting a warm cache hit to execution (re-checking `<crate>.attested` under the lock and releasing the lock before `exec` / waiting on the launched binary).
- Perform one-time initialization across the actual 0.2.0 (`crabx/v2/`) and legacy (`crabx/v1/`) cache layouts, keeping warm and pinned runs O(1):
  - Because [attestation-binding](/quest/m1/attestation-binding.md) moves 0.2.0 caches to `crabx/v2/<host>/<mode>/<crate>/<version>` and keeps legacy `crabx/v1/<host>/<mode>/<crate>/<version>` directories intact, both cache layouts remain available even if 0.2.0 ships before this quest.
  - On any run, check `<crate>.attested` first (O(1) stat): if present, the ratchet is active.
  - Otherwise, if `<crate>.migrated` already exists (O(1) stat), skip scanning.
  - Otherwise, acquire the shared per-crate ratchet lock, re-check `<crate>.attested` and `<crate>.migrated`, and scan every cached version directory for that crate across both `crabx/v2/<host>/{author,quickinstall}/<crate>` and `crabx/v1/<host>/{author,quickinstall}/<crate>`. If any entry's `record.json` records a GitHub attestation (`source` starting with `"GitHub attestations"`), create `<crate>.attested`; then create `<crate>.migrated`.
- Enforce the ratchet on both **downloads** and **warm-cache selection / execution**:
  - On an **unpinned** run when the ratchet is active and both attested `1.0.0` and unattested `2.0.0` are cached, select the newest eligible attested version (`1.0.0`) rather than failing while a usable verified release is cached (and fail the background update of unattested `2.0.0` with a diagnostic).
  - On a **pinned** run (such as `crate@2.0.0`) or when no attested cached version is eligible, refuse the unattested release loudly with an error naming the crate, version, and how to proceed if the author intentionally stopped attesting.
- Add `--require-attestation` (before the crate name, conflicting with `--build` and `--allow-quickinstall`):
  - Refuse any release without at least one verified GitHub attestation, using a separate trust-policy cache namespace under `crabx/v2/<host>/attested-only/` (or enforcing that any cached `Record` served under `--require-attestation` carries verified attestation proof) so an unattested cached entry never satisfies `--require-attestation`.
  - Forward `--require-attestation` in `network::spawn_refresh` so detached background updates preserve the effective strict policy and update the matching cache namespace.
- Add regression tests (updating cache path assertions in `tests/execute.rs` and `tests/e2e.sh` as needed for `crabx/v2/`) covering: warm-cache ratchet selection (unpinned run picks attested `1.0.0` over cached unattested `2.0.0`, while pinned `crate@2.0.0` fails), monotonic state updates and serialized final admission (concurrent activation blocks an in-flight unattested download and a warm cache hit), one-time initialization scanning both `crabx/v2/` and legacy `crabx/v1/` `author` and `quickinstall` caches, O(1) warm/pinned fast path once `<crate>.migrated` exists, cross-mode ratchet sharing, and `--require-attestation` handoff into `--refresh`.
- Document both the ratchet (including its one-time migration boundary for all pre-ratchet versions: 0.1.x and pre-ratchet 0.2.0) and `--require-attestation` in `README.md`.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
- [Attestation binding](/quest/m1/attestation-binding.md) - moves 0.2.0 caches to `crabx/v2/` while keeping legacy `crabx/v1/` directories read-only so upgrade history is preserved for the ratchet
