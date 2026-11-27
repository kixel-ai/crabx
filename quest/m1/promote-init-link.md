# [XS] Try the setup form on crabx.kixel.me and promote it

## Goal

The maintainer has used the form on https://crabx.kixel.me (under "For maintainers") to
open GitHub's editor for a real repository, and has written whatever copy announces it.
Copy is the maintainer's, so this quest drafts none.

## Plan

The form sends a repository to GitHub's new-file editor with
`.github/workflows/binaries.yml` prefilled, with no server or credential. The PR that added
it exercised the link against the editor from a local copy of the site, because the
deployed site only builds from main; this is the first run on the deployed page.

- Enter a public crate repository without the file, such as one of moq's, and check that the
  editor opens on the default branch with the file in it.
- Decide where to promote it and what the surrounding site text should say: the section
  now carries only a functional label above the field.

## Waiting

- Maintainer (action): open https://crabx.kixel.me, enter `owner/repo` of a public Rust
  repository, confirm GitHub's editor shows `binaries.yml` ready to commit, then write the
  announcement copy and the site's text for the form. An agent cannot judge the copy, and
  the deployed page only exists after the PR merges.
