# [M] Draw crabx's logo

## Goal

crabx has an SVG mark and a wordmark lockup for light and dark backgrounds. The mark is
the favicon and the README leads with the lockup. Each file is drawn in code, with no
font, `<text>`, or raster image.

## Plan

Decided with the maintainer on 2026-11-23, in /quest-plan: ✅ SVG concepts drawn in code,
from which the maintainer picks one on this quest's PR. That is how
[kixel-ai/token-max#128](https://github.com/kixel-ai/token-max/pull/128) made token-max's
logo. Rejected: image-model concepts redrawn as SVG; a mark the maintainer supplies.

Recommendations:

- Draw about ten concepts from different ideas, such as a crab, an x, running without
  compiling, and a warm cache. Render them on one comparison sheet in the PR, at full size
  and at a true 16 px on light and dark tab colors. The maintainer's pick goes under
  `## Waiting` until they answer.
- After the pick, delete the other concepts and any generator script. Keep the mark as
  the root `favicon.svg`, also the T3 Code project icon, and the two lockups under
  `assets/logo/`. Lead the README with them in a `<picture>` that switches for light and
  dark.
- One mark file for both backgrounds: use cut-outs and colors that clear 3:1 contrast on
  white and on GitHub's dark background.
- Keep `assets/` out of the crate's package (`cargo package --list`).
