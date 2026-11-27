# Commits

PRs into `main` land as squash commits, so the PR title becomes the commit subject.

- Use conventional-commit subjects (`feat: ...`, `fix(pipeline): ...`, `chore: ...`, `docs: ...`).
- Never commit compiled binaries or build output; source assets such as images and fonts are fine.

# PRs

Keep the body short and structured, not narrated.
Have at least these sections:

- **Problem**: a summary of the problem and why this PR is needed.
- **Approach**: a summary of the approach taken to solve the problem.
- **Impact**: a bullet point for every public API change, including the pipeline's inputs and the trust policy.
- **Alternatives**: any alternative approaches considered.
- **Follow-ups**: any issues encountered or quests created.

# Landing

CI runs no checks on pull requests; `check` reruns `just ci` after each push to `main`.
A PR lands once its local `quest review` rounds pass; queue the reviewed head with `quest merge <pr>`.
The admin bypass of `main`'s rulesets is for emergencies.

# Imported code

Each imported component keeps its own license and records its upstream source and revision, as [vendor/README.md](vendor/README.md) does.
