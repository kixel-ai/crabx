# Crate release pipeline

The reusable workflow builds published binary crates for x86_64 and aarch64 GNU
Linux, macOS, and Windows. Linux uses cargo-zigbuild with glibc 2.28. Each archive
contains every binary, under binstall's default directory and filename. Builds
use all crate features so binaries with required features are included; mutually
exclusive features fail the build rather than silently omit binaries.

Run `crabx init` in a public crate repository to write its caller,
`.github/workflows/binaries.yml`:

```yaml
name: Binaries
on:
  push:
    branches: ['main']
    tags: ['v*', '*-v*']
  schedule:
    - cron: '23 7 * * *'
  workflow_dispatch:
permissions:
  contents: read
jobs:
  release:
    permissions:
      contents: write
      id-token: write
      attestations: write
    uses: kixel-ai/crabx/.github/workflows/crabx.yml@b206ab0a5a2ca06abfc4672d1a61b182b51fc6f3 # pipeline-v1.0.1
```

`crabx init` fills in origin's default branch and pins this release's immutable
commit, so Dependabot can bump it. It refuses to overwrite a different existing
file. The pipeline checks out its own scripts at the exact commit of the called
workflow, independently of the caller's checkout.

A crates.io version can't be replaced, so a release workflow can build its binaries
before publishing. `crabx-dry-run.yml` plans each tracked binary crate whose manifest
names the repository, at the calling commit and its manifest version, and builds it on
every target as the pipeline will once it is published. It uploads, attests, tags, and
releases nothing, so it needs only `contents: read` and runs from any branch or event;
it fails when no crate qualifies. Gate publishing on it, pinned to the same commit as
the caller above (from `pipeline-v1.1.0`):

```yaml
jobs:
  check:
    uses: kixel-ai/crabx/.github/workflows/crabx-dry-run.yml@<pipeline commit>
  publish:
    needs: check
    # ...cargo publish
```

crabx's own `release.yml` gates its crates.io publish this way, in the same run as its
pipeline caller; the dry run's build jobs take other names and upload nothing, so they
never collide with the release's.

Each run discovers tracked crate manifests, looks up crates.io versions published
since the caller workflow was added, and fills missing target archives. Yanked
versions, library-only crates, and crates whose crates.io or published manifest
names another repository are skipped; the log names each one skipped for its
repository. Missing
or dirty VCS metadata, invalid paths, and a mismatched `name-vVERSION` tag stop
the run; yanking such a version skips it. A failed build fails the run and holds
back its crate, while crates whose archives all built still publish. An existing
annotated or lightweight `name-vVERSION` tag must point to Cargo's recorded
commit; a `vVERSION` tag is used when it points there and otherwise left to
another crate. When neither is used, publishing creates `name-vVERSION` there.
Existing release assets are preserved. Publishing is serialized per release tag
across trigger refs. Each run publishes
one crate at a time, so crates sharing a tag cannot replace pending jobs within
that run. GitHub retains only one pending publisher per tag across runs; a newer
run backfills the same missing archives if it supersedes an older pending job.
No release is made latest merely because it was backfilled.

Build jobs have read-only tokens and no OIDC permission, and Cargo runs without
the runner's artifact, cache, and step-output variables. Each build job uploads
only its target's archive, under a name it cannot overwrite. Linux build tools
install only from hash-pinned wheels in `requirements.txt`. Only the publishing
job attests and writes releases; it never runs crate code. It reads the run's
unfiltered artifact and job lists and takes a target's archive only when exactly
one artifact holds that target's name and the latest attempt of its build job
succeeded. It downloads each artifact ZIP unextracted, checks its SHA-256 against
the artifact's digest, and stages the archive only when the ZIP holds that single
regular file. Re-running failed jobs reuses earlier attempts' archives, while
re-running a build that already uploaded holds back its crate until a fresh run.
Artifacts are scoped to the whole run, not to a job, so a build that subverts its
runner could replace another target's artifact in the same run; every crate of a
run comes from the same repository. Callers must grant the job's three
permissions; reading the run's artifacts and jobs needs no `actions`
permission in a public repository. Public GitHub.com repositories are the
supported boundary; private repositories and GitHub Enterprise Server are not.

The pipeline runs only on `push`, `schedule`, and `workflow_dispatch`; any other
caller event fails its first step with an error naming the three.
Tags made with `GITHUB_TOKEN` do not trigger workflows, which is why the default
branch push, daily schedule, and manual dispatch are included. GitHub disables
public schedules after 60 inactive days; the next push or dispatch backfills
versions published during that pause. A backfill exceeding GitHub's 256 matrix
jobs fails explicitly.

`.github/workflows/crabx-test.yml` builds the two-binary fixture on all six targets
with read-only permissions on PRs that change the pipeline, then runs the
publishing job's artifact checks on them, still read-only. A manual dispatch also attests the archives
without creating tags or releases, and bypasses crates.io discovery. Local tests
exercise discovery, archiving, and publishing with recorded API and Cargo
responses; the fixture workflow builds the real archives on all six targets.
Before tagging a pipeline release, dispatch it on the commit to tag and check each
archive with `gh attestation verify <archive> -R kixel-ai/crabx --signer-workflow
kixel-ai/crabx/.github/workflows/crabx.yml`. Then point `src/binaries.yml`,
`.github/workflows/release.yml`, the caller above, and the dry-run example at that commit and merge, so
the `Release` run on the merge push exercises the pin. Once it succeeds, push an
annotated tag (`git tag -a`) on the pinned commit, last because it is
irreversible; a crabx test keeps the first and last pins identical.

References: [binstall archive defaults](https://github.com/cargo-bins/cargo-binstall/blob/main/SUPPORT.md),
[reusable job identity](https://docs.github.com/en/actions/reference/workflows-and-actions/contexts#job-context),
and [attestation predicates](https://github.com/actions/attest).
