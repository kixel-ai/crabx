# [M] Investigate job Release / npm-check (macos-15), job Release / npm-check (ubuntu-24.04), and 1 more

## Goal

Kixel Quest measured what the Plan lists and filed it here on its own. For each finding:

1. Diagnose its cause, starting from the culprit's diff and sessions where it has them.
2. Have a second session, one that did not write the diagnosis, try to refute it, and diagnose again when it does.
3. Fix it, accept it with the reason recorded in this Plan, or improve on it further. A fix carries the measurement that shows it.

Then look past the finding: make the related improvements the diagnosis turns up where they fit here, and add the rest as follow-up quests.

## Plan

Each finding keeps the comment Kixel Quest finds it by, so a later run that sees it again adds its sighting here instead of filing it anew.

### `job Release / npm-check (macos-15)`: flake

<!-- kixel.quest finding {"series":"job Release / npm\u002dcheck (macos\u002d15)","culprit":null} -->

- Series: [`job Release / npm-check (macos-15)`](https://github.com/kixel-ai/crabx/actions/runs/45713771792/attempts/1), in %.
- A flake: fix it at the root cause now, or disable it until it is fixed and record that here. Never retry it, wait it out, or loosen what it checks. This quest stays open until it is fixed and enabled again.
- 2026-11-25: 50 % against a baseline of 0 %, +50 %, at [`05721b3`](https://github.com/kixel-ai/crabx/commit/05721b38ceec131d0c00c4e8be1374bd0472be10). Failed in run 45713771792 attempt 1 and passed in run 45713771792 attempt 2 of Release, failing 1 of the 2 times it ran on its commit.

### `job Release / npm-check (ubuntu-24.04)`: flake

<!-- kixel.quest finding {"series":"job Release / npm\u002dcheck (ubuntu\u002d24.04)","culprit":null} -->

- Series: [`job Release / npm-check (ubuntu-24.04)`](https://github.com/kixel-ai/crabx/actions/runs/45713771792/attempts/1), in %.
- A flake: fix it at the root cause now, or disable it until it is fixed and record that here. Never retry it, wait it out, or loosen what it checks. This quest stays open until it is fixed and enabled again.
- 2026-11-25: 50 % against a baseline of 0 %, +50 %, at [`05721b3`](https://github.com/kixel-ai/crabx/commit/05721b38ceec131d0c00c4e8be1374bd0472be10). Failed in run 45713771792 attempt 1 and passed in run 45713771792 attempt 2 of Release, failing 1 of the 2 times it ran on its commit.

### `job Release / npm-check (windows-2025)`: flake

<!-- kixel.quest finding {"series":"job Release / npm\u002dcheck (windows\u002d2025)","culprit":null} -->

- Series: [`job Release / npm-check (windows-2025)`](https://github.com/kixel-ai/crabx/actions/runs/45713771792/attempts/1), in %.
- A flake: fix it at the root cause now, or disable it until it is fixed and record that here. Never retry it, wait it out, or loosen what it checks. This quest stays open until it is fixed and enabled again.
- 2026-11-25: 50 % against a baseline of 0 %, +50 %, at [`05721b3`](https://github.com/kixel-ai/crabx/commit/05721b38ceec131d0c00c4e8be1374bd0472be10). Failed in run 45713771792 attempt 1 and passed in run 45713771792 attempt 2 of Release, failing 1 of the 2 times it ran on its commit.
