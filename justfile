set shell := ["bash", "-euo", "pipefail", "-c"]

# List the available development commands.
default:
    @just --list

# Lint the runner, its local fork of binstalk's fetchers, the release scripts, the installer, the workflows, and the quest tree.
check:
    cargo clippy --locked --all-targets -- -D warnings
    cargo fmt --all --check
    ruff check --select F
    shellcheck install.sh
    shfmt -d install.sh
    actionlint
    taplo format --check
    nixfmt --check flake.nix
    quest check

# Exercise cache execution, trust policy, the release's packages, and the pipeline without the network.
test *args:
    cargo test --locked {{ args }}
    python3 -I test_release.py
    python3 -I pipeline/test_pipeline.py

# Run what CI runs: lint, then test.
ci: check test

# Run crabx from the development checkout.
run *args:
    cargo run --locked -- {{ args }}

# Apply formatters without changing program behavior.
fix:
    cargo fmt --all
    shfmt -w install.sh
    taplo format
    nixfmt flake.nix
