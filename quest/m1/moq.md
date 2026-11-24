# [S] Run moq's CLIs from its pipeline builds

## Goal

`crabx moq-relay` and `crabx moq-cli` run binaries crabx's pipeline built and attested in
moq-dev/moq's own CI, on all six targets, so moq is crabx's first consumer as
[AGENTS.md](/AGENTS.md) requires.

## Plan

Switching moq's release workflows is moq's work, in its own quest tree, as
[the root plan](/quest/README.md#plan) decided; this quest keeps that wait visible here and
checks the result.
kixel-ai/rust2#719 listed it as a follow-up for `/quest-plan` in moq, but no moq quest or
commit names crabx as of 2026-11-23.
Today `crabx moq-relay` finds the archives moq's `release-binary.yml` attaches through the
`[package.metadata.binstall]` that moq-dev/moq#8586 added, with no attestation.

Once a moq release carries the pipeline's archives, run both CLIs through crabx on each
target and confirm crabx verified their attestation; fix in crabx whatever that turns up.

## Waiting

- moq-dev/moq: a quest switching moq-cli's and moq-relay's release builds to crabx's
  pipeline at `pipeline-v1.0.0` or later. An agent advances it by running `/quest-plan` in
  moq-dev/moq; check `git grep -il crabx origin/main -- quest` in a moq checkout, then for
  a moq release whose assets carry GitHub attestations
  (`gh attestation verify <archive> -R moq-dev/moq`).
