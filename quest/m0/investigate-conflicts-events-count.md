# [M] Investigate conflicts/events (count), cost/by-model/model:claude-opus-5-5 (tokens), and 4 more

## Goal

Kixel Quest measured what the Plan lists and filed it here on its own. For each finding:

1. Diagnose its cause, starting from the culprit's diff and sessions where it has them.
2. Have a second session, one that did not write the diagnosis, try to refute it, and diagnose again when it does.
3. Fix it, accept it with the reason recorded in this Plan, or improve on it further. A fix carries the measurement that shows it.

Then look past the finding: make the related improvements the diagnosis turns up where they fit here, and add the rest as follow-up quests.

## Plan

Each finding keeps the comment Kixel Quest finds it by, so a later run that sees it again adds its sighting here instead of filing it anew.

### `conflicts/events (count)`: bottleneck

<!-- kixel.quest finding {"series":"conflicts/events (count)","culprit":null} -->

- Series: [`conflicts/events (count)`](https://kixel.quest/factory#bottlenecks-1469373889).
- 2026-11-25: 0 against a baseline of 0, +0. The largest share of conflicts, 13.04% of PRs over the 30 days to 2026-11-25; too few measurements for a trend.

### `cost/by-model/model:claude-opus-5-5 (tokens)`: bottleneck

<!-- kixel.quest finding {"series":"cost/by\u002dmodel/model:claude\u002dopus\u002d5\u002d5 (tokens)","culprit":null} -->

- Series: [`cost/by-model/model:claude-opus-5-5 (tokens)`](https://kixel.quest/factory#bottlenecks-1469373889), in tokens.
- 2026-11-25: 5712000 tokens against a baseline of 5712000 tokens, +0 tokens (+0%). The largest share of cost, 94.9% of its measure over the 30 days to 2026-11-25; too few measurements for a trend.

### `friction/split/category:waiting (tokens)`: bottleneck

<!-- kixel.quest finding {"series":"friction/split/category:waiting (tokens)","culprit":null} -->

- Series: [`friction/split/category:waiting (tokens)`](https://kixel.quest/factory#bottlenecks-1469373889), in tokens.
- 2026-11-25: 269419 tokens against a baseline of 269419 tokens, +0 tokens (+0%). The largest share of friction, 7.278% of its measure over the 30 days to 2026-11-25; too few measurements for a trend.

### `host/gate-wait/command:nix develop (h)`: bottleneck

<!-- kixel.quest finding {"series":"host/gate\u002dwait/command:nix develop (h)","culprit":null} -->

- Series: [`host/gate-wait/command:nix develop (h)`](https://kixel.quest/factory#bottlenecks-1469373889), in h.
- 2026-11-25: 0.03786 h against a baseline of 0.03786 h, +0 h (+0%). The largest share of host time, 91.51% of its measure over the 30 days to 2026-11-25; too few measurements for a trend.

### `rework/fixes (count)`: bottleneck

<!-- kixel.quest finding {"series":"rework/fixes (count)","culprit":null} -->

- Series: [`rework/fixes (count)`](https://kixel.quest/factory#bottlenecks-1469373889).
- 2026-11-25: 0 against a baseline of 0, +0. The largest share of rework, 8.696% of PRs over the 30 days to 2026-11-25; too few measurements for a trend.

### `ship/review (h)`: bottleneck

<!-- kixel.quest finding {"series":"ship/review (h)","culprit":null} -->

- Series: [`ship/review (h)`](https://kixel.quest/factory#bottlenecks-1469373889), in h.
- 2026-11-25: 0.03778 h against a baseline of 0.03778 h, +0 h (+0%). The largest share of time to ship, 43.37% of its phases over the 30 days to 2026-11-25; too few measurements for a trend.
