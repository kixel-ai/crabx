# [XS] Restore the musl-fallback end-to-end check

## Goal

On glibc Linux, `tests/e2e.sh` runs a real crate that ships only static musl Linux builds, checks
its trust source, and checks that the installed binary is static, as it did with `token-max@0.0.1`
before [attestation binding](https://github.com/kixel-ai/crabx/pull/45) refused that release.

## Plan

Decided 2026-11-25 (maintainer, /quest-plan): ✅ use `ripgrep@14.1.1` now (rejected: keep waiting
for token-max to republish through crabx's pipeline; drop the check). The earlier plan waited on
token-max shipping a pipeline-signed, musl-only release, but crabx's pipeline builds glibc Linux
archives (and, once [musl builds](/quest/m1/pipeline-musl.md) land, musl beside them), so that
wait would never clear. On x86_64, ripgrep 14.1.1 ships only an `x86_64-unknown-linux-musl`
archive, with no attestation. On 2026-11-25, crabx 0.2.0 ran it on x86_64 glibc Linux from its static build and
recorded `author release (no published attestation)`.

In `tests/e2e.sh`, on `x86_64-unknown-linux-gnu`:

- `pinned "$1" ripgrep@14.1.1 author "author release (no published attestation)\""`;
- a check that the installed `rg` is a static build (no program interpreter), the inverse of the
  `fnox` check. `interpreter` opens `bin/<crate name>`, but ripgrep's binary is `rg`, so let it
  take the executable's name separately from the cache entry;
- keep `refused token-max@0.0.1`: that release stays branch-signed, so it remains the check that
  an author's branch-signed attestation is refused.

Only x86_64: ripgrep ships an `aarch64-unknown-linux-gnu` archive, so on aarch64 the check would
not exercise the fallback.
