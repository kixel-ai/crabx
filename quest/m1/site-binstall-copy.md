# [XS] Mention cargo-binstall author-release compatibility on crabx.kixel.me

## Goal

The landing page at https://crabx.kixel.me tells users and crate authors that crabx works out of the box with crates that publish `cargo-binstall`-compatible author releases, without overpromising silent third-party or source-compile fallbacks.

## Plan

Decided 2026-11-24 (maintainer, /quest-plan): ✅ wait until both the site ([PR #23](https://github.com/kixel-ai/crabx/pull/23)) and the [musl fallback](/quest/m0/musl-fallback.md) land, then add a concise note in `site/index.html` (under the "Find" step or "Ship your crate's binaries" section) stating that crabx works out of the box with crates publishing `cargo-binstall`-compatible author releases, without silently falling back to third-party builds or compiling from source.

Avoid claiming "any project that uses `cargo binstall`", since crabx intentionally disables quickinstall and source-compile fallbacks by default and does not support private repositories or custom registries.

## Required

- [Landing page](/quest/m0/site/page.md) - creates `site/index.html`
- [Musl fallback](/quest/m0/musl-fallback.md) - makes musl-only `cargo-binstall` crates work on glibc Linux before advertising compatibility
