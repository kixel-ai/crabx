# [XS] Harden install.sh, install.ps1, and the published install commands

## Goal

The published bootstrap commands in `README.md` and `site/index.html`, along with `install.sh` and `install.ps1`, enforce HTTPS on every redirect hop (including Windows PowerShell 5.1), clean up all staging files on interruption, validate `CRABX_VERSION` against strict SemVer 2.0.0, and parse crates.io's index independent of JSON key order.

## Plan

Found 2026-11-24 in a pre-release security audit. Caused by `kixel-ai/rust2#826` (initial installers).

Decided 2026-11-24 (maintainer, /quest-plan): ✅ harden both installers in `m0` (users run them directly from `main` via GitHub Pages, and this does not touch the Rust crate or delay [0.1.1](/quest/m0/release-0.1.1.md)).

Changes to make:

- Harden the published bootstrap commands in `README.md`, `site/index.html`, installer header comments, and `.github/workflows/e2e.yml`'s deployed smoke tests (which should run those exact published commands):
  - Unix: add `--proto '=https' --tlsv1.2` to `curl` (`curl --proto '=https' --tlsv1.2 -fsSL https://crabx.kixel.me/install.sh | sh`) so an HTTPS-to-HTTP redirect on the initial fetch cannot execute plaintext.
  - Windows PowerShell: enforce HTTPS-only redirects and TLS 1.2 on the initial `install.ps1` fetch as well (or use a small bounded HTTPS-only redirect check in the published PowerShell launch command) so Windows PowerShell 5.1 cannot follow an HTTPS-to-HTTP downgrade before `install.ps1` runs.
- Strict SemVer 2.0.0 validation in both `install.sh` and `install.ps1` (specified directly rather than reusing `release.py`'s looser `VERSION` regex):
  - Require `MAJOR.MINOR.PATCH` where each core part is `0` or `[1-9][0-9]*`.
  - Optional prerelease `-(id)(\.(id))*` where each non-empty identifier is either `0`, `[1-9][0-9]*`, or an ASCII alphanumeric/hyphen identifier containing at least one non-digit (rejecting leading-zero numeric identifiers like `1.2.3-01` and empty segments).
  - Optional build metadata `+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*` (rejecting empty segments like `1.2.3+a..b` as well as `.` and `..`).
- In `install.sh`:
  - Pass `--proto '=https' --tlsv1.2` to every `curl` call so no redirect can downgrade to plaintext HTTP.
  - Replace predictable `"$dir/.crabx.$$"` with `staged=$(mktemp "$dir/.crabx.XXXXXX")`.
  - Trap `EXIT INT TERM HUP` (terminating on signals) and remove both `"$temp"` and `"${staged:-}"` if set, clearing `staged` right after the final `mv "$staged" "$dir/crabx"` so an interruption never leaves a stray staging file in `$dir` and never touches the installed binary.
  - Parse crates.io's sparse index lines without assuming `"vers"` precedes `"yanked":false` (for example, matching `"vers"` and `"yanked":false` independently in `awk`).
- In `install.ps1`:
  - Enforce HTTPS on every redirect hop for both the crates.io index request and the GitHub release archive download, explicitly handling redirects with `AllowAutoRedirect = $false` (or equivalent in `HttpWebRequest`) and verifying `$nextUri.Scheme -eq 'https'` on every hop (with a bounded hop limit), because Windows PowerShell 5.1's `WebClient` and `Invoke-WebRequest` can otherwise follow HTTPS-to-HTTP downgrades.
  - Dispose HTTP response/client resources cleanly.
  - Verify HTTPS-to-HTTP redirect rejection on Windows PowerShell 5.1 (in CI or a dedicated Windows PowerShell test).
- Extend `test_release.py`'s `Install` tests to cover reordered JSON keys, strict SemVer rejection cases (`1.2.3-01`, `1.2.3+a..b`, `.`, `..`), `--proto`/`--tlsv1.2` flags, cleanup of both `$temp` and the `$dir` staging file on interruption, and that `README.md`, `site/index.html`, and `e2e.yml` carry the hardened bootstrap commands.
