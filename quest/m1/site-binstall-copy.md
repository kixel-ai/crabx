# [XS] Mention cargo-binstall author-release compatibility on crabx.kixel.me

## Goal

The landing page at https://crabx.kixel.me tells users and crate authors that crabx works out of the box with crates that publish `cargo-binstall`-compatible author releases (without overpromising silent third-party or source-compile fallbacks), published only once a release with the musl fallback is available through the advertised install channels.

## Plan

Decided 2026-11-24 (maintainer, /quest-plan): ✅ wait until both the site ([PR #23](https://github.com/kixel-ai/crabx/pull/23)) and [0.1.1](/quest/m0/release-0.1.1.md) (which ships the musl fallback to crates.io, GitHub Releases, and npm) are live, then add a concise note in `site/index.html` (under the "Find" step or "Ship your crate's binaries" section) stating that crabx works out of the box with crates publishing `cargo-binstall`-compatible author releases, without silently falling back to third-party builds or compiling from source.

Avoid claiming "any project that uses `cargo binstall`", since crabx intentionally disables quickinstall and source-compile fallbacks by default and does not support private repositories or custom registries.

## Required

- [Landing page](/quest/m0/site/page.md) - creates `site/index.html`
- [0.1.1](/quest/m0/release-0.1.1.md) - publishes the musl fallback through all advertised install channels before claiming musl-capable binstall compatibility on the site
