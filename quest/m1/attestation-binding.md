# [S] Bind branch-ref attestations to the crate, version, and runner

## Goal

Branch-triggered GitHub attestations verify signed crate, version, and runner-environment identity (not only the repository and workflow path), GitHub release downloads (whether via declared `repository`, redirected `repository`, explicit `pkg-url`, initial derived archive URL, or archive GET redirects) always enforce matching GitHub origin and attestation checks, and 0.2.0 publishes verified author-release entries in a policy-versioned cache namespace while leaving legacy 0.1.x cache directories intact and never executing unverified legacy entries.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#758` (runner) and `kixel-ai/rust2#781` (pipeline provenance).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ ship this in `m1` for [0.2.0](/quest/m1/release-0.2.md), requiring [0.1.1](/quest/m0/release-0.1.1.md) so `0.1.1` stays musl-only.

Changes to make:

- Signed version and runner binding on branch-ref attestations (`src/trust.rs`):
  - When the verified workflow signer matches `PIPELINE_SIGNER`, parse the verified DSSE in-toto statement's `predicate.buildDefinition` and require:
    - `externalParameters.crate` matches the requested crate name (ASCII case-insensitive),
    - `externalParameters.version` matches the requested version, and
    - `internalParameters.github.runner_environment == "github-hosted"`.
  - For non-pipeline author workflows: because binstalk's `NOVERSION_FILENAMES` templates allow reusing `{name}-{target}` across release tags, an in-toto `subject[].name` check alone does not prove version binding on a branch ref. Restrict branch-ref (`refs/heads/*`) acceptance to `PIPELINE_SIGNER` (which carries signed `externalParameters.{crate, version}`, matching README's stated reason for accepting branch refs: pipeline backfill), or if a non-pipeline branch ref must be accepted, require signed release/tag evidence and document the exact boundary in `README.md`.
- Enforce consistent GitHub origin checks across declared `repository`, resolved `repository` redirects, initial archive URL (`pkg-url` or derived default URL), and archive GET redirects (`vendor/binstalk-fetchers/`, `src/trust.rs`):
  - Record the declared `Cargo.toml` `repository`, the resolved `repository` URL (after any redirect), the initial archive URL, and the archive GET's redirect chain (including intermediate `github.com/<owner>/<repo>/releases/download/...` hops before GitHub's CDN redirect) in `Receipt`.
  - Never allow a GitHub release download to skip attestation verification or query the wrong repository:
    - If `Cargo.toml` declares GitHub repository `A`, and `pkg-url`, the initial archive URL, or any redirect hop downloads from a release in a different GitHub repository `B`, fail closed on the conflicting GitHub repository identities (preventing a query against `A` from returning 404 while leaving `B`'s published proofs unchecked).
    - If `Cargo.toml`'s declared `repository` is non-GitHub (or omitted) and either the `repository` URL resolves to any GitHub host (including a GitHub project page that binstalk then uses to derive the initial archive URL) or the initial archive URL / archive GET redirect chain hits a GitHub release URL, fail closed as an ambiguous GitHub origin unless `pkg-url` itself explicitly declares that same GitHub repository.
    - When `Cargo.toml` omits `repository` and `pkg-url` explicitly declares `https://github.com/<owner>/<repo>/releases/download/...` (with no redirect to a different GitHub repo), query and verify GitHub attestations for `<owner>/<repo>`.
- Isolate 0.2.0 verified cache entries from legacy 0.1.x cache directories without breaking 0.1.0 rollback or concurrent 0.1.0 runs:
  - Publish entries verified under the 0.2.0 origin and binding policy in a policy-versioned cache namespace (for example, `author-v2` and `quickinstall-v2` under `crabx/v1/<host>/`, or a new cache schema version) instead of renaming over or deleting non-empty legacy 0.1.x directories.
  - This guarantees: (1) atomic `fs::rename` never collides with a non-empty legacy directory, (2) concurrent or rolled-back 0.1.0 processes keep their legacy cache entries intact, (3) 0.2.0 never executes any legacy 0.1.x author-release entry (from either `author/` or `quickinstall/`, regardless of its old `source` label) without verifying it under the new policy, and (4) legacy `0.1.x` attested `record.json` files stay preserved read-only on disk for [attestation-ratchet](/quest/m1/attestation-ratchet.md) to initialize attestation history from.
- Add offline unit tests in `src/trust.rs` and `src/cache.rs` covering wrong crate, wrong version, self-hosted runner, non-pipeline branch ref with reused `{name}-{target}` filename, manifest GitHub repo `A` vs. `pkg-url`/redirect GitHub repo `B` conflict, non-GitHub `repository` redirecting to a GitHub project page (with no `pkg-url`) and archive GET redirect to GitHub, `pkg-url`-only GitHub releases, and isolation from legacy 0.1.x `author/` and `quickinstall/` cache entries.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
