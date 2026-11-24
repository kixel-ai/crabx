# [L] Put crabx and crates on PATH with crabx install

## Goal

`crabx install` puts crabx itself on PATH, so `npx @kixel/crabx install` is a
one-line install with no shell script. `crabx install <crate>[@version]` also
puts `<crate>` on PATH as a launcher that runs it through crabx, so it keeps
updating itself and stays attestation-checked, or stays at a pinned version
such as `crabx install token-max@0.1.0`. Linux, macOS and Windows on x86_64
and arm64.
Non-goal: copying a frozen binary onto PATH, which is cargo-binstall's job and
which [the root quest](/quest/README.md) rules out.

## Plan

Decided 2026-11-24 (maintainer, /quest-plan in kixel-ai/token-max, where
max.kixel.me will offer `npx @kixel/crabx install token-max`):

- ✅ Both forms: bare `crabx install` for crabx itself and
  `crabx install <crate>` for a launcher (rejected: only bare
  `crabx install`, leaving `crabx <crate>` as the only way to run a crate;
  no `install` subcommand).
- ✅ The launcher is a link to crabx that crabx recognizes by the name it was
  started under (argv[0]), as busybox does: a symlink on Unix, and on
  Windows a hard link or copy of `crabx.exe` named after the crate (rejected:
  npm-style shim scripts, which start an extra shell, and whose `.cmd` asks
  "Terminate batch job?" on Ctrl-C). A pin lives in crabx's state, not in the
  link.
- ✅ `crate@version` works wherever a crate is named: `crabx token-max@0.1.0`
  already runs a pinned version, and `crabx install token-max@0.1.0` installs
  a launcher pinned to it.

Recommendations:

- Install into the directory `install.sh` and `install.ps1` use
  (`CRABX_INSTALL_DIR`, else `~/.local/bin`). Handle PATH as they do:
  Windows adds the directory to the user's `Path`, and Unix says when it is
  missing.
- When the running crabx isn't the one in that directory, as under `npx`,
  whose cache is temporary and can't be a link's target,
  `crabx install <crate>` first installs crabx itself.
- Keep a record of installed launchers and their pins in crabx's state.
  Bare `crabx install` refreshes every launcher, so upgrading crabx this way
  also upgrades the Windows copies.
- Refuse, naming the path, to overwrite a file in that directory that isn't
  crabx's own launcher, such as a `token-max` that token-max's installer put
  there.
- If crabx is started under a name other than `crabx` that has no record,
  that's an error. Started under a recorded name, crabx passes every option
  and argument through to the crate.
- Resolve the crate (and its attestation) at install time, so a typo or a
  crate with no usable release fails at once rather than on its first run.
- `install` is a crate on crates.io too; as with `init`, `crabx install@VERSION`
  runs it.
- Uninstalling means deleting the launcher; leave out an `uninstall` command
  unless the state record makes plain deletion unsafe.
- Offer `npx @kixel/crabx install` in the README and the site's install tabs.
- Cover it in `.github/workflows/e2e.yml` on all three OSes: `npx
  @kixel/crabx install <crate>`, then run the crate by name from a new shell.

## Required

- [Musl fallback](/quest/m0/musl-fallback.md) - token-max, this command's first consumer, ships only musl Linux builds
