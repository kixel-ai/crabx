# [S] Fix the flaky launcher signal test

## Goal

`test_release.py`'s `Launcher.test_signals_reach_the_binary` passes every run, so `just ci` fails only on real defects.

## Plan

It failed 2 of 4 local runs under `quest gate nix develop --command just ci` on 2026-11-24, at the `("node", "INT")` case: the fake binary printed `ready` but never `got INT`, so its SIGINT went unhandled until the 20 s alarm killed it. A standalone loop of `node` `process.execve` into the same Python script with SIGINT passed 200 of 200, so the race likely needs the suite's load or an earlier case's state. Reproduce it under load first (such as the test alone in a loop beside `cargo test`), then fix the cause in the launcher or the test, not with a retry or longer alarm.
