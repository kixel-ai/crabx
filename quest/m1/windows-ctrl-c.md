# [S] Wait for the crate on Windows Ctrl-C

## Goal

On Windows, Ctrl-C in a console reaches crabx and the crate it runs, and
crabx keeps waiting until the crate exits, then exits with the crate's code,
so the prompt never returns while the crate is still shutting down. A
launcher from `crabx install` behaves the same.

## Plan

Today crabx's Windows path runs the crate as a child and waits on it with
the default console handler, so Ctrl-C ends crabx at once with
`STATUS_CONTROL_C_EXIT` while the crate may still be printing or cleaning
up. `npm/crabx.js` already ignores SIGINT on Windows for this reason.

- Install a console control handler that returns TRUE for Ctrl-C and
  Ctrl-Break while crabx waits. Don't use `SetConsoleCtrlHandler(NULL, TRUE)`:
  children inherit that, so the crate would ignore Ctrl-C too.
- Check it in e2e.yml's Windows legs with a crate that sleeps after
  Ctrl-C, or explain in the PR why the console can't be driven there.

Decided 2026-11-25 (maintainer, /quest-plan): ✅ no `Hosts`; it runs
anywhere, with e2e.yml's Windows legs as its check (rejected: keep `Hosts:
windows`; drop the quest). No host `every-host` lists runs Windows, so a
`windows` host requirement left the quest unstartable.
