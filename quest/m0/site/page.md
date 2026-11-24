# [M] Serve crabx's landing page at crabx.kixel.me

## Goal

https://crabx.kixel.me serves a one-page site for people who run Rust CLIs and for
crate authors, along with `install.sh` and `install.ps1`. crabx.kixel.ai is retired, and
nothing in the repository names it.

## Plan

Decided with the maintainer on 2026-11-23, in /quest-plan:

- ✅ GitHub Pages, deployed from main by the existing workflow, in the `github-pages`
  environment. Rejected: a Cloudflare Worker like token-max's. It would allow response
  headers such as a CSP, but every change would need a hand deploy with wrangler from
  desktop, or a Cloudflare token in CI.
- ✅ Retire crabx.kixel.ai with no redirect. Rejected: a redirect Worker in the
  repository; a Cloudflare redirect rule. The maintainer accepted that the install lines
  in crabx 0.1.0's published crates.io README will break.
- ✅ Content for both audiences:
  - a hero with `npx crabx <crate>` and the installers, in macOS/Linux and Windows tabs;
  - a terminal showing a real run, such as `crabx moq-relay --help`;
  - a short section on trust and attestations;
  - a section for crate authors on `crabx init`.

  Rejected: users only, with a link to the pipeline README.
- ✅ The header is token-max's: see [the epic](/quest/m0/site/README.md#plan).

Recommendations:

- Static HTML and CSS under `site/`, with no generator, and with any script optional.
  Rename `.github/workflows/installers.yml` to `site.yml`. It copies `site/`, `install.sh`,
  and `install.ps1` into the Pages artifact, and deploys on changes to any of them.
  Extend `test_release.py`'s check that every installer URL users see names a file the
  job copies.
- Follow token-max's page on a dark background, accented with the logo's orange
  (`#e8590c`) and cyan (`#22d3ee`). Self-host any fonts, add a
  `<meta>` CSP limited to the page's own origin, add the avatar as a small webp, and add
  an `og.png` of the page's top.
- Get the terminal's run from the built binary, and keep it free of private names.
- Domain, with the agent Chrome or the Cloudflare API, which desktop's wrangler login
  may reach:
  - Verify kixel.me for Pages in the kixel-ai organization, as kixel.ai was.
  - Add a DNS-only `crabx` CNAME in the kixel.me zone, pointing to `kixel-ai.github.io`.
  - Set the custom domain and enforce HTTPS. When kixel.ai moved, GitHub issued the
    certificate only after the domain was removed and set again.
  - Delete the `crabx` CNAME in the kixel.ai zone.

  Add a `Maintainer:` item to `## Waiting` only for a step neither route can take.
- Replace every crabx.kixel.ai mention outside `quest/` with crabx.kixel.me: the README,
  both installers' usage lines, and the workflow's comments.
- The PR adds a follow-up quest for the maintainer to try and promote the site.

## Required

- [Logo](/quest/m0/site/logo.md) - the favicon and lockup the page shows
