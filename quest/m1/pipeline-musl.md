# [M] Build static musl Linux archives in the pipeline

## Goal

Every crate released through crabx's pipeline gets `x86_64-unknown-linux-musl` and
`aarch64-unknown-linux-musl` archives (static, attested like the rest) beside its glibc ones, so
its binaries run on any Linux, including Alpine and hosts older than glibc 2.28. crabx and
binstall keep picking the glibc build on glibc hosts and fall back to the musl build elsewhere.

## Plan

Decided 2026-11-25 (maintainer, /quest-plan in kixel-ai/token-max's adoption of the pipeline):
✅ add musl beside glibc (rejected: switch Linux to musl only, which gives every crate musl's slow
allocator, servers like `moq-relay` included; token-max shipping glibc only). token-max moves from
cargo-dist to this pipeline to dogfood it, and the maintainer's rule is that a gap the pipeline
has becomes a crabx quest. token-max needs static builds: `doctor --ssh <host>` downloads the
release for the remote host's platform and runs it there, and its run script is the documented
path on Alpine.

- Add both targets to `TARGETS` in `pipeline/pipeline.py`, built with cargo-zigbuild on the same
  runners as their glibc counterparts. Check that the binaries come out static (no program
  interpreter) and fail the build if not, since a crate's build script can link dynamically.
- The fixture workflow (`crabx-test.yml`) builds all eight targets.
- Existing releases get the new archives only through the pipeline's normal backfill of missing
  target archives. Decide whether that's wanted for versions published before this lands, and
  bound it: a backfill over 256 matrix jobs fails. Recommended: backfill, since attested musl
  archives for old versions help the same hosts.
- Update `pipeline/README.md`'s opening paragraph and the target list in any docs that name six
  targets.
- crabx's own `install.sh` refuses hosts without glibc 2.28. Once a crabx release carries musl
  archives, it can install that build instead. Leave this out of scope here, since no crabx
  release has musl archives until [the pipeline release](/quest/m1/pipeline-v1.1.md) moves
  crabx's own pin.
