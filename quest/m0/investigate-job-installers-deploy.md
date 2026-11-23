# [M] Investigate job Installers / deploy

## Goal

Kixel Quest measured what the Plan lists and filed it here on its own. For each finding:

1. Diagnose its cause, starting from the culprit's diff and sessions where it has them.
2. Have a second session, one that did not write the diagnosis, try to refute it, and diagnose again when it does.
3. Fix it, accept it with the reason recorded in this Plan, or improve on it further. A fix carries the measurement that shows it.

Then look past the finding: make the related improvements the diagnosis turns up where they fit here, and add the rest as follow-up quests.

## Plan

Each finding keeps the comment Kixel Quest finds it by, so a later run that sees it again adds its sighting here instead of filing it anew.

### `job Installers / deploy`: flake

<!-- kixel.quest finding {"series":"job Installers / deploy","culprit":null} -->

- Series: [`job Installers / deploy`](https://github.com/kixel-ai/crabx/actions/runs/45279414310/attempts/1), in %.
- A flake: fix it at the root cause now, or disable it until it is fixed and record that here. Never retry it, wait it out, or loosen what it checks. This quest stays open until it is fixed and enabled again.
- 2026-11-23: 50 % against a baseline of 0 %, +50 %, at [`6905ead`](https://github.com/kixel-ai/crabx/commit/6905ead4b9c6a98ebbaa6517a148a95fc1197429). Failed in run 45279414310 attempt 1 and passed in run 45279414310 attempt 2 of Installers, failing 1 of the 2 times it ran on its commit.
