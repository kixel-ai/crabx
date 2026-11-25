# [M] Investigate scheduled End to end

## Goal

Kixel Quest measured what the Plan lists and filed it here on its own. For each finding:

1. Diagnose its cause, starting from the culprit's diff and sessions where it has them.
2. Have a second session, one that did not write the diagnosis, try to refute it, and diagnose again when it does.
3. Fix it, accept it with the reason recorded in this Plan, or improve on it further. A fix carries the measurement that shows it.

Then look past the finding: make the related improvements the diagnosis turns up where they fit here, and add the rest as follow-up quests.

## Plan

Each finding keeps the comment Kixel Quest finds it by, so a later run that sees it again adds its sighting here instead of filing it anew.

### `scheduled End to end`: failure

<!-- kixel.quest finding {"series":"scheduled End to end","culprit":null} -->

- Series: [`scheduled End to end`](https://github.com/kixel-ai/crabx/actions/runs/45684768636/attempts/1), in jobs.
- 2026-11-25: 1 jobs against a baseline of 0 jobs, +1 jobs, at [`3007fbc`](https://github.com/kixel-ai/crabx/commit/3007fbc5eee277c69bed006922d3880880658e4b). Run 45684768636 attempt 1 failed, failing test (windows-2025).
