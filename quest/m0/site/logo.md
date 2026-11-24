# [S] Finish crabx's logo

## Goal

crabx has wordmark lockups for light and dark backgrounds, built around the mark in
[`favicon.svg`](/favicon.svg), and the README leads with them. Each file is drawn in
code, with no font, `<text>`, or raster image.

## Plan

Decided with the maintainer on 2026-11-23, in /quest-plan:

- ✅ SVG concepts drawn in code, picked by the maintainer. This is how
  [kixel-ai/token-max#128](https://github.com/kixel-ai/token-max/pull/128) made token-max's
  logo. Rejected: image-model concepts redrawn as SVG; a mark the maintainer supplies.
- ✅ A front-on orange crab (`#e8590c`) with a light cyan (`#22d3ee`) x painted on its
  shell. It's in [`favicon.svg`](/favicon.svg), checked in by the planning PR. The
  maintainer picked it over three rounds of concepts.
  - Round 1 rejected: crossed claws, a pincer prompt, a plain crab, a lone pincer, a box
    with claws, a crab running sideways, a hex nut, a claw holding a play button, and an
    x-shaped crab.
  - Round 1 also had a claw-shaped c beside an x. The maintainer asked for more
    variations, because "it needs to actually look like a claw. Currently it looks more
    like C++". Round 2 redrew it as a pincer: bare, with teeth, with a joint, on a wrist,
    gripping the x, and in one color. The maintainer then asked for "a crab + x instead
    of a claw", or "a crab on top of an x".
  - Round 3 rejected: the crab beside an x, in front of a big x, standing on an x,
    perched where an x crosses, and an all-orange version. The maintainer picked the crab
    with an x on its shell, but "the blue x inside, not a cutout".
  - X color rejected: the earlier cyan `#0891b2`, which nearly matches the shell's
    brightness and blurs at 16 px; sky blue `#38bdf8`; deep blue `#1e3a8a`.

Recommendations:

- Draw the wordmark "crabx" as stroked paths, with "crab" in `currentColor` or a per-theme
  ink and the "x" in the mark's cyan, so the name repeats the shell's x.
  Put `logo-on-light.svg` and `logo-on-dark.svg` under `assets/logo/`.
- Lead the README with them in a `<picture>` that switches for light and dark.
- Small changes to the mark are fine, such as evening out the legs or the claws. Check it
  again at a true 16 px on light and dark tab colors.
- `assets/` and `favicon.svg` stay out of the crate's `include` list in `Cargo.toml`.
