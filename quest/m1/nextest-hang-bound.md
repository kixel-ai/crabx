# [XS] Bound hung tests with nextest

## Goal

A deadlocked test in crabx ends after a bounded time instead of holding a run for as long as it likes. `just test` and CI run `cargo nextest run`, and the repository has a `.config/nextest.toml` with a hang bound. The dev shell provides nextest. Once the config exists, kixel-ai/quest's `quest hook` denies plain `cargo test` here and points agents at the recipe.

## Plan

Decided 2026-11-26 (maintainer, /quest-run in kixel-ai/quest, from [kixel-ai/quest#924](https://github.com/kixel-ai/quest/pull/924)): ✅ keep it for crabx. On 2026-11-25 two deadlocked tests in moq ran 40 and 49 minutes under plain `cargo test`, which has no hang bound, and crabx's `just test` runs `cargo test --locked` (rejected: leaving crabx on `cargo test`, whose tests are fast today; dropping the quest).

- Use moq's bound: `slow-timeout = { period = "30s", terminate-after = 2 }`. The tests run in well under a second, and `tests/execute.rs` and `tests/launcher.rs` spawn child processes, which is where a hang would come from.
- Add `cargo-nextest` to the dev shell in `flake.nix`, and switch `just test` to `cargo nextest run --locked`. The crate has no library, so no `cargo test --doc` step is needed; add one if a library target appears.
- `.github/workflows/e2e.yml`'s nightly `test` job runs `cargo test --locked` on macOS and Windows with no Nix shell. Switch it to nextest, installed the way the repository's workflows install their other tools, so a hang there is bounded too.
- `quest/m2/audit-tests.md` and `audit-cost.md` name `cargo test` in their findings; update them only if they say what the recipe runs.
