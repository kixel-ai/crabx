# [XS] Mention cargo-binstall author-release compatibility on crabx.kixel.me

## Goal

The landing page at https://crabx.kixel.me tells users and crate authors that crabx uses `cargo-binstall`'s release discovery and archive conventions for author releases (subject to crabx's trust policy, without overpromising silent third-party or source-compile fallbacks), published only once a release with the musl fallback is available through the advertised install channels.

## Plan

Decided 2026-11-24 (maintainer, /quest-plan): ✅ wait until [0.1.1](/quest/m0/release-0.1.1.md) (which ships the musl fallback to crates.io, GitHub Releases, and npm) is live, then add a concise note in `site/index.html` (under the "Find" step or "Ship your crate's binaries" section) stating that crabx uses `cargo-binstall`'s release discovery and archive layouts for author releases, subject to crabx's trust policy (for example, failing closed on invalid or untrusted attestations and disabling quickinstall and source-compile fallbacks by default).

Avoid claiming "any project that uses `cargo binstall`", since crabx intentionally disables quickinstall and source-compile fallbacks by default, rejects non-conforming attestations (such as non-pipeline branch attestations lacking signed release or tag evidence), and does not support private repositories or custom registries.

## Required

- [0.1.1](/quest/m0/release-0.1.1.md) - publishes the musl fallback through all advertised install channels before claiming musl-capable binstall compatibility on the site
