# [M] Port binstall's API-free release lookup into the fetchers fork

## Goal

crabx's vendored binstalk-fetchers carries cargo-binstall's post-0.10.54 lookup, which finds release tags with `git ls-refs` and release assets without GitHub's REST API, caching per crate, so an unauthenticated `crabx <crate>` spends no GitHub API quota on finding assets.

## Plan

Upstream landed it on cargo-binstall's `main` after the vendored revision `f027dceb`, unreleased as of 2026-11-23:

- [cargo-bins/cargo-binstall#2808](https://github.com/cargo-bins/cargo-binstall/pull/2808) - tags by `git ls-refs`, assets from github.com without the API
- [cargo-bins/cargo-binstall#2813](https://github.com/cargo-bins/cargo-binstall/pull/2813) - per-crate cache in the fetchers
- [cargo-bins/cargo-binstall#2805](https://github.com/cargo-bins/cargo-binstall/pull/2805), [#2806](https://github.com/cargo-bins/cargo-binstall/pull/2806), [#2807](https://github.com/cargo-bins/cargo-binstall/pull/2807) - fewer requests for quickinstall, missing tags, and candidate assets

They change binstalk-git-repo-api and binstalk-downloader too, which crabx takes from crates.io, so port them once those crates release: re-import binstalk-fetchers at the release's revision, reapply the local changes `vendor/README.md` lists, and bump the sibling crates to match.
Attestation queries still use the API, so measure the requests one cold run makes before and after.

The cost falls on users without a token: a shared address's 60 unauthenticated requests an hour can be gone before crabx starts. In `.github/workflows/e2e.yml`'s [run 45339444768](https://github.com/kixel-ai/crabx/actions/runs/45339444768/job/138264632187), before discovery used `GH_TOKEN`/`GITHUB_TOKEN`, a `macos-15` arm runner started at 0 of 60, so `crabx --allow-quickinstall cargo-expand@1.0.126` failed with `release discovery timed out`. The nightly now authenticates, so it no longer shows this; measure the port with an unauthenticated cold run instead.

Decided 2026-11-24 (maintainer, /quest-run): ✅ keep this quest (rejected: drop it).

Decided 2026-11-24 (maintainer, /quest-run): ✅ offer the fork's resolver fix (`vendor/README.md`'s "Wait for resolver channel capacity": upstream's `try_send` lets a queued error displace the one success) upstream to cargo-bins/cargo-binstall as a PR with its test, so the next import drops it (rejected: keep it local).
Decided 2026-11-25 (maintainer, /quest-run): ✅ post the drafted PR as written (rejected: edit it first; keep the fix local). It is [cargo-bins/cargo-binstall#2854](https://github.com/cargo-bins/cargo-binstall/pull/2854); the import drops the local "Wait for resolver channel capacity" change if the release holds it, and keeps it otherwise.

## Waiting

- cargo-bins/cargo-binstall: a binstalk-fetchers release after 0.10.54 that holds #2808 and #2813; check `https://crates.io/api/v1/crates/binstalk-fetchers` for its `max_stable_version`
