# [S] Try and promote crabx.kixel.me

## Goal

The maintainer has tried https://crabx.kixel.me, crabx's one-page site, and announced crabx
where they choose, such as X and the moq Discord.

## Plan

Start this once https://crabx.kixel.me/install.sh prints the installer. Until then, an agent
switches the Pages domain from crabx.kixel.ai, which the nightly `site` job in
`.github/workflows/e2e.yml` fails without (kixel.me's org verification and the `crabx` CNAME
in the kixel.me zone are done):

1. Once `site.yml`'s deploy on main passes, clear the custom domain and set it again, since
   GitHub issued kixel.ai's certificate only that way:
   `gh api -X PUT repos/kixel-ai/crabx/pages -F cname=null`, then `-f cname=crabx.kixel.me`.
2. Once `gh api repos/kixel-ai/crabx/pages --jq .https_certificate.state` reads `approved`,
   `gh api -X PUT repos/kixel-ai/crabx/pages -F https_enforced=true`. If it stays `new` for
   15 minutes, repeat step 1.
3. Delete the `crabx` CNAME in Cloudflare's kixel.ai zone. Keep the kixel.ai Pages
   verification, which other kixel-ai sites may use.

Decided 2026-11-24 (maintainer, [kixel-ai/crabx#23](https://github.com/kixel-ai/crabx/pull/23#issuecomment-7141565098)):
✅ keep this follow-up (rejected: drop).

## Waiting

- Maintainer: open https://crabx.kixel.me on desktop and phone, run its install line, and
  post about crabx where they choose; an agent can't speak for the maintainer in public.
  Done when the maintainer says so.
