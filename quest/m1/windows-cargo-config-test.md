# [XS] Fix the Windows path in cargo.rs's checkout configuration test

## Goal

`cargo::tests::rejects_what_a_checkout_controls_or_configures` passes on Windows, so e2e's `test (windows-2025)` job is green again.

## Plan

Found 2026-11-25 by e2e on [PR #46](https://github.com/kixel-ai/crabx/pull/46). Caused by [PR #47](https://github.com/kixel-ai/crabx/pull/47) (build-cwd), whose CI never ran e2e because e2e's path filter doesn't include `src/`.

The test's last assertion (`src/cargo.rs`, near line 409) expects `work.join("opt/.cargo/config")`, which Windows displays as `...\work\opt/.cargo/config`. The error names `home.ancestors()`' `...\work\opt` joined with `.cargo/config`, which displays as `...\work\opt\.cargo/config`. Build the expected path the same way, `work.join("opt").join(".cargo/config")`, as the loop above it does with `home.parent()`. Verify on the PR's e2e `test (windows-2025)` job by touching a file in e2e's path filter, or by dispatching e2e on the branch.
