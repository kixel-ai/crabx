# [S] Install crates with crabx's installers

## Goal

`curl --proto '=https' --tlsv1.2 -fsSL https://crabx.kixel.me/install.sh | sh -s -- token-max`
installs crabx as today and then runs `crabx install token-max`, which checks the crate's
attestation, and the PowerShell installer does the same on Windows. Any crate released through
crabx's pipeline thereby gets a one-line installer with no per-crate config. With no crate named,
both installers behave as today.

## Plan

Decided 2026-11-25 (maintainer, /quest-plan in kixel-ai/token-max's adoption of the pipeline):
✅ crabx's installers take crate names (rejected: token-max writing its own installers from its
run scripts, which checks no attestation; the pipeline publishing per-crate installer scripts as
release assets, as cargo-dist does, a larger change that adds assets to every crate). token-max
drops cargo-dist, whose `token-max-installer.sh`/`.ps1` max.kixel.me's `/install.sh` and
`/install.ps1` redirect to today, and points those lines here.

- `install.sh` takes crate specs as arguments and runs the installed crabx's `crabx install` once
  per spec, since `crabx install` takes one crate. It stops at the first failure, after crabx
  itself is installed, and the error names the spec that failed.
- `irm ... | iex` passes no arguments, so `install.ps1` takes them from an environment variable.
  Recommended: `CRABX_INSTALL` holding space-separated specs, accepted by `install.sh` too, plus
  the `& ([scriptblock]::Create((irm https://crabx.kixel.me/install.ps1))) token-max` form, which
  passes arguments. Document both in the README.
- Specs are passed as separate arguments to `crabx install`, never through a shell, so a spec
  can't inject a command.
- `tests/install.sh` and the Windows installer test cover a named crate, using a crate the e2e
  already runs.
- The site deploys `install.sh` and `install.ps1` from `main` (`site.yml`), so this needs no
  crabx release: `crabx install` shipped in 0.2.0.

Non-goal: installing on hosts without glibc 2.28, which waits on crabx's own musl release (see
[musl builds](/quest/m1/pipeline-musl.md)).
