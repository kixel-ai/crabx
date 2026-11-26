# [M] Investigate bugs/escaped/found:check (count), cost/by-model/model:claude-opus-5-5 (USD), and 1 more

## Goal

Kixel Quest measured what the Plan lists and filed it here on its own. For each finding:

1. Diagnose its cause, starting from the culprit's diff and sessions where it has them.
2. Have a second session, one that did not write the diagnosis, try to refute it, and diagnose again when it does.
3. Fix it, accept it with the reason recorded in this Plan, or improve on it further. A fix carries the measurement that shows it.

Then look past the finding: make the related improvements the diagnosis turns up where they fit here, and add the rest as follow-up quests.

## Plan

Each finding keeps the comment Kixel Quest finds it by, so a later run that sees it again adds its sighting here instead of filing it anew.

### `bugs/escaped/found:check (count)`: bottleneck

<!-- kixel.quest finding {"series":"bugs/escaped/found:check (count)","culprit":null} -->

- Series: [`bugs/escaped/found:check (count)`](https://kixel.quest/factory#bottlenecks-1469373889).
- 2026-11-26: 0 against a baseline of 0, +0. Worsening: its last 7 days ran worse than the 30 before, median 0 over 7 days against 0 over 30, one-sided Mann-Whitney U, p = 0.004505, under 0.01 over the 2 series tested; 95% of its measure.

### `cost/by-model/model:claude-opus-5-5 (USD)`: bottleneck

<!-- kixel.quest finding {"series":"cost/by\u002dmodel/model:claude\u002dopus\u002d5\u002d5 (USD)","culprit":null} -->

- Series: [`cost/by-model/model:claude-opus-5-5 (USD)`](https://kixel.quest/factory#bottlenecks-1469373889), in USD.
- 2026-11-26: 3.409 USD against a baseline of 3.409 USD, +0 USD (+0%). The largest share of cost, 92.7% of its measure over the 30 days to 2026-11-26; too few measurements for a trend.

### `friction/split/category:waiting (h)`: bottleneck

<!-- kixel.quest finding {"series":"friction/split/category:waiting (h)","culprit":null} -->

- Series: [`friction/split/category:waiting (h)`](https://kixel.quest/factory#bottlenecks-1469373889), in h.
- 2026-11-26: 0.001205 h against a baseline of 0.001205 h, +0 h (+0%). The largest share of friction, 39.64% of its measure over the 30 days to 2026-11-26; too few measurements for a trend.
