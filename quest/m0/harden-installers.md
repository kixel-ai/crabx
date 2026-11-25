# [XS] Harden install.sh and install.ps1

## Goal

`install.sh` and `install.ps1` enforce HTTPS on redirects, use unpredictable staging paths and full signal cleanup, validate `CRABX_VERSION` as strict semver, and parse crates.io's index independent of JSON key order.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#826` (initial installers).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ harden both installers in `m0` (users run them directly from `main` via GitHub Pages, and this does not touch the Rust crate or delay [0.1.1](/quest/m0/release-0.1.1.md)).

Changes to make:

- In `install.sh`:
  - Pass `--proto '=https' --tlsv1.2` to every `curl` call so a redirect cannot downgrade to plaintext HTTP.
  - Replace predictable `"$dir/.crabx.$$"` with `mktemp "$dir/.crabx.XXXXXX"`.
  - Trap `EXIT INT TERM HUP` (not only `EXIT`) so Ctrl-C on POSIX `sh` (`dash`) cleans up `$temp`.
  - Validate `CRABX_VERSION` against strict semver (matching `release.py`'s `VERSION` pattern) so `.` or `..` are rejected.
  - Parse crates.io's sparse index lines without assuming `"vers"` precedes `"yanked":false` (for example, matching each field independently in `awk`).
- In `install.ps1`:
  - Validate `CRABX_VERSION` against the same strict semver pattern, and dispose the `WebClient` (or use `Invoke-WebRequest`) cleanly.
- Extend `test_release.py`'s `Install` tests to cover reordered JSON keys, `.`/`..` versions, and the `--proto`/`--tlsv1.2` flags.
- Note: [PR #23](https://github.com/kixel-ai/crabx/pull/23) (`quest/m0/site/page.md`) also updates URLs in `install.sh` and `install.ps1`; rebase or merge against `main` if it lands first.
