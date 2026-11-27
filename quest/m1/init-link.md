# [S] Set up crabx's pipeline from a link on crabx.kixel.me

## Goal

A crate maintainer enters `owner/repo` on https://crabx.kixel.me. The site sends them to
GitHub's new-file editor for that repository, with `.github/workflows/binaries.yml`
prefilled exactly as `crabx init` would write it. One commit, or the PR GitHub offers to
open, opts the repository in. crabx runs no server and holds no credential for anyone's
repository.

## Plan

Decided 2026-11-26 (maintainer, /quest-plan): ✅ a prefilled link on the site (rejected: a
GitHub App; dropping it). This was planned as a GitHub App whose installation opens the PR
that `crabx init` makes. That App would need Contents, Workflows, and Pull requests write
on every repository that installs it, plus a hosted webhook. Its private key could push
workflows into every adopter's repository. GitHub's editor gets the same one-click result
without the key or the hosting. History: the App waited for crabx's first publish
(decided 2026-11-18), and that wait cleared with 0.1.0.

Recommendations for the implementer:

- Use [`src/binaries.yml`](/src/binaries.yml) as the only template. `.github/workflows/site.yml`
  should copy it into the deployed site, so the link and `crabx init` cannot drift. A check
  should also fail if they would differ.
- Fill in `BRANCH` with the repository's default branch, read client-side from GitHub's
  public REST API. Unauthenticated calls are limited per visitor, which is fine at one
  call per use. Before linking, refuse with a clear message when the repository is missing,
  private, or tracks no `Cargo.toml`, mirroring `crabx init`'s refusals. If the API fails,
  say so instead of guessing `main`.
- The link is `https://github.com/<owner>/<repo>/new/<branch>?filename=.github/workflows/binaries.yml&value=<encoded>`.
  Exercise it on the deployed site against a scratch repository and confirm GitHub keeps
  the file byte for byte. If the editor won't accept `value` or `filename` with a directory,
  stop and report rather than fall back to a different flow.
- Add a short README mention next to `crabx init`.

The PR launches something new for users to try, so it adds a follow-up for the maintainer
to try the link and promote it.
