# [S] Try and promote crabx.kixel.me

## Goal

The maintainer has tried https://crabx.kixel.me, crabx's one-page site, and announced crabx
where they choose, such as X and the moq Discord.

## Plan

The site is live: on 2026-11-25 Pages switched to crabx.kixel.me with HTTPS enforced,
`curl -fsSL https://crabx.kixel.me/install.sh | sh` installed crabx 0.1.0 in a fresh `HOME`,
and the `crabx` CNAME in Cloudflare's kixel.ai zone was deleted (kixel.ai's Pages
verification stays for other kixel-ai sites).

Decided 2026-11-24 (maintainer, [kixel-ai/crabx#23](https://github.com/kixel-ai/crabx/pull/23#issuecomment-7141565098)):
✅ keep this follow-up (rejected: drop).

Decided 2026-11-24 (maintainer, [kixel-ai/crabx#23](https://github.com/kixel-ai/crabx/pull/23#issuecomment-7147829397)):
✅ accept the minutes-long gap between merge and the domain switch, when main's install lines
name crabx.kixel.me before it serves them, and delete the kixel.ai CNAME (rejected: keep
crabx.kixel.ai as a Cloudflare 301 redirect). ✅ accept that crabx 0.1.0's published npm and
crates.io READMEs keep the dead crabx.kixel.ai install line until the next release
republishes them.

## Waiting

- Maintainer (action): open https://crabx.kixel.me on desktop and phone, run its install
  line, and post about crabx where they choose. An agent can smoke-test the site but can't
  speak for the maintainer in public, so no automation covers the post. Done when the
  maintainer says they posted.
