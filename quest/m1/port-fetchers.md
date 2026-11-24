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

## Waiting

- cargo-bins/cargo-binstall: a binstalk-fetchers release after 0.10.54 that holds #2808 and #2813; check `https://crates.io/api/v1/crates/binstalk-fetchers` for its `max_stable_version`
