# [S] Release pipeline v1.1.0 with musl builds

## Goal

The annotated tag `pipeline-v1.1.0` names the pipeline commit that builds static musl Linux
archives, and `crabx init`, crabx's own `release.yml`, and `pipeline/README.md`'s caller pin that
commit, so consumers (kixel-ai/token-max first) can adopt it.

## Plan

Follow the release steps at the end of `pipeline/README.md`: dispatch the pipeline on the commit
to tag, verify each archive, all eight targets included, with `gh attestation verify`, move the
three pins and merge, let the merge's `Release` run exercise the pin, then push the tag last.
Adding targets is backward compatible for callers, so this is a minor release.

kixel-ai/token-max's move to the pipeline waits on this tag; its quest checks for it with
`git ls-remote --tags https://github.com/kixel-ai/crabx pipeline-v1.1.0`.

## Required

- [Musl builds](/quest/m1/pipeline-musl.md) - the targets this release ships
