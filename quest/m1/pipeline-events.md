# [S] Fail the pipeline on events it doesn't support

## Goal

A caller workflow triggered by an event other than push, schedule, or workflow_dispatch fails `crabx.yml` with an error naming those three, instead of skipping every job or running a build nobody needs.

## Plan

`crabx.yml`'s `discover` job runs on push, schedule, workflow_dispatch, and pull_request, and skips silently on anything else, such as `release` or `merge_group`.
pull_request has been accepted since the pipeline was created in kixel-ai/rust2#756, though nothing calls the pipeline on it: `crabx init`'s caller has no pull_request trigger, and `crabx-test.yml` runs PR fixtures through `crabx-build.yml` directly.
A caller that adds pull_request gets no check of the PR. Discovery rebuilds every published version that lacks assets on every target, from crates.io's commits, and publish is skipped.

Decided 2026-11-24 (maintainer, /quest-run): keep this quest (rejected: drop it).

Decided 2026-11-24 (maintainer, /quest-run): `crabx.yml` fails with an error naming push, schedule, and workflow_dispatch on any other event, and drops its pull_request handling from `discover` and `publish` (rejected: keep pull_request as a read-only dry run; keep the silent skip).
Remove `discover`'s job-level event `if:`, which would otherwise skip the job before any step runs, and make its first step fail on any event outside the three.
This fails closed, as AGENTS.md requires for malformed input.
Callers that `crabx init` wrote are unaffected; a caller with other triggers drops them.
This ships in the next pipeline release, so callers pinned to pipeline-v1.0.0 are unaffected until they bump.
