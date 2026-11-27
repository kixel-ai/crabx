# [S] Release pipeline v1.1.0 with musl builds and consumer dry runs

## Goal

The annotated tag `pipeline-v1.1.0` names the pipeline commit that builds static musl Linux
archives and offers consumers a dry run, and `crabx init`, crabx's own `release.yml`, and `pipeline/README.md`'s caller pin that
commit, and `pipeline/README.md`'s dry-run example names it in place of `<pipeline commit>`,
so consumers (kixel-ai/token-max first) can adopt it.

## Plan

Follow the release steps at the end of `pipeline/README.md`: dispatch the pipeline on the commit
to tag, verify each archive, all eight targets included, with `gh attestation verify`, move the
four pins and merge, let the merge's `Release` run exercise the pin, then push the tag last.
Adding targets and the `crabx-dry-run.yml` workflow is backward compatible for callers, so this is a minor release.

kixel-ai/token-max's move to the pipeline waits on this tag; its quest checks for it with
`git ls-remote --tags https://github.com/kixel-ai/crabx pipeline-v1.1.0`.

## Required

- [Musl builds](/quest/m1/pipeline-musl.md) - the targets this release ships
