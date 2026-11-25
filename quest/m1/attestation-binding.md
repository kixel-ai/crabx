# [S] Bind branch-ref attestations to the crate, version, and runner

## Goal

Branch-triggered GitHub attestations verify signed crate, version, and runner-environment identity (not only the repository and workflow path), GitHub downloads discovered through vanity redirects or `pkg-url` never bypass attestation verification, and upgrading to 0.2.0 never executes any legacy 0.1.x `author` cache entry whose origin or binding policy was not verified under the new rules.

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
- Close GitHub origin redirect and `pkg-url`-only verification gaps (`vendor/binstalk-fetchers/src/lib.rs`, `src/trust.rs`):
  - Never allow a GitHub release download (`https://github.com/<owner>/<repo>/releases/download/...`) to skip attestation verification.
  - Do not let an arbitrary redirect from a non-GitHub `Cargo.toml` `repository` URL silently establish a trusted GitHub attestation repository or drop verification: if `Cargo.toml`'s declared `repository` is non-GitHub and redirects to GitHub (making the GitHub repository unverified by the crates.io manifest), fail closed as an ambiguous GitHub origin unless `pkg-url` itself explicitly declares that same GitHub repository.
  - When `Cargo.toml` omits `repository` and `pkg-url` explicitly declares `https://github.com/<owner>/<repo>/releases/download/...`, query and verify GitHub attestations for `<owner>/<repo>`.
- Handle pre-0.2.0 cached entries safely without breaking 0.1.0 cache compatibility:
  - Because existing `record.json` stores only a `source` string and rejects unknown fields (`deny_unknown_fields`), record a separate sidecar verification marker alongside `record.json` for every `author` entry verified under the 0.2.0 origin and binding policy.
  - When loading a cached `author` entry that lacks the new verification marker (including both legacy `"GitHub attestations ..."` entries and legacy `"author release (repository does not support GitHub attestations)"` or `"author release (no published attestation)"` entries whose GitHub origin check may have been skipped in 0.1.x), treat the entry as requiring revalidation/reinstallation under the current policy rather than executing it directly.
- Add offline unit tests in `src/trust.rs` and `src/cache.rs` covering wrong crate, wrong version, self-hosted runner, non-pipeline branch ref with reused `{name}-{target}` filename, vanity-domain redirect to GitHub, `pkg-url`-only GitHub releases, and pre-0.2.0 cached entry revalidation (both legacy GitHub-attested entries and legacy `"repository does not support GitHub attestations"` entries).

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - cut before this lands, so 0.1.1 ships only the musl fallback
