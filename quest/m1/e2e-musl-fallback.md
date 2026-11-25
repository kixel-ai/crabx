# [XS] Restore the musl-fallback end-to-end check

## Goal

`tests/e2e.sh` installs a real crate that ships only static musl Linux builds on glibc Linux and
checks its trust source and that the installed binary is static, as it did with `token-max@0.0.1`
before [attestation binding](https://github.com/kixel-ai/crabx/pull/45) refused that release: its
only attestation is the author's own workflow signing from `refs/heads/main`, which names no
release. The check must run against a pipeline-signed release, since a branch-ref attestation
from any other signer now fails.

## Plan

Decided 2026-11-25 (maintainer, in
[#45](https://github.com/kixel-ai/crabx/pull/45#issuecomment-7170514240)): ✅ republish
`token-max` (kixel-ai/token-max) through crabx's pipeline so its attestations are pipeline-signed
and Linux ships musl-only, then restore the check against that release (rejected: dropping the
check; accepting an author's branch attestation with signed tag evidence). The republish is
token-max's work, not this tree's: it changes token-max's `release.yml` and
`docs/releasing.md` and cuts its release, so it belongs in kixel-ai/token-max's quest tree,
where `/quest-plan` adds it as a quest (see its `quest/m1/release-0.1.md`). This quest only
restores the check once that release exists, so it waits on it.

Then in `tests/e2e.sh`:

- add `pinned "$1" token-max@<version> author "GitHub attestations (1) sha256:"` on
  `*-linux-gnu` (with `bare=1` restored if its `--version` prints only the version, and the
  `bare` handling in `pinned`);
- restore the `interpreter token-max/<version>` check that the install is a static build;
- drop the `refused token-max@0.0.1` case, or keep it as the check that an author's
  branch-signed attestation is refused if another crate can stand in for it.

## Waiting

- kixel-ai/token-max: publish a release built by crabx's pipeline whose Linux assets are musl
  only, so its attestations are pipeline-signed. Check with `gh release view -R kixel-ai/token-max`
  and `gh attestation verify` showing the crabx pipeline's workflow as signer.
