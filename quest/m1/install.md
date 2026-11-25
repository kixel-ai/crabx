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

- A launcher passes every argument to the crate, `--refresh` included, so
  the background refresh crabx starts from a launcher is marked by
  something no argument can produce, such as an environment variable, and
  that marker wins over dispatch by name. Otherwise a Windows copy named
  `token-max.exe` relaunching itself to refresh would run the crate again.
  Test that a refresh from a launcher runs no second crate invocation, and
  that `<crate> --refresh` reaches the crate.
- Reserve the runner's own name: `crabx install crabx` is an error pointing
  at `crabx crabx@VERSION`, with names compared case-insensitively, since
  Windows and macOS filesystems usually are.
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
- Resolve the crate (and its attestation) and its binary at install time, so
  a typo, a crate with no usable release, or a crate whose binary needs
  `--bin` fails at once rather than on its first run. `crabx install` takes
  `--bin` and records it with the launcher.
- A launcher knows only the name it was started under (on Unix,
  `current_exe` resolves the symlink to crabx), so key each pin and binary
  choice by launcher name. Installing a name that already has a launcher in
  another directory is an error naming that launcher.
- `install` is a crate on crates.io too; as with `init`, `crabx install@VERSION`
  runs it.
- Uninstalling means deleting the launcher; leave out an `uninstall` command
  unless the state record makes plain deletion unsafe.
- Offer `npx @kixel/crabx install` in the README and the site's install tabs.
- Test the PR's own build, not the registry's: the published 0.1.0 has no
  `install`. Pack the candidate npm packages with `release.py`, and in
  read-only PR CI install from those packages, run bare `crabx install` and
  check that a new shell without npm's bin directory on `PATH` resolves
  `crabx` to the install directory, run `crabx install <crate>`,
  and run the crate by name from a new shell, on all three OSes.
  [The release](/quest/m1/release-0.2.md) runs the registry command once it
  publishes. After its npm job, `release.yml` runs a read-only job on all
  three OSes that, for versions 0.2.0 and later (the first with `install`), installs the release's exact version
  (`@kixel/crabx@<version>`) whenever npm has it, whether this run published
  it or an earlier one did, so a rerun that skips publishing still checks it, and runs the crate by name from a new shell, so
  a packaging or PATH regression in a published package fails that release's
  run.
