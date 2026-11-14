# SDD ledger — plan: docs/superpowers/plans/2026-11-14-settings-groups-and-log-viewer.md

Pre-flight: no shared interfaces beyond Task 1 -> Task 2 (`logs::sources`, `logs::tail`, `logs::LogSource`) and Task 3 -> Tasks 4, 5 (`SettingsSubTab`, `openLog`, `LogSource`). Names match between producer and consumer blocks.

Ruling: no git worktree; work continues on `main` — the user asked for task-by-task validation in the live checkout they install from, and this repo has shipped every prior change on `main` — cost if wrong: an interrupted task leaves uncommitted work on `main`, recoverable because each task ends with a commit.
Ruling: the plan's helper scripts are bash scripts and WSL is not installed on this machine — I run the equivalent steps inline (read plan task, run the test command, append the ledger line in the same call as the commit) — cost if wrong: no scripted BASE tracking, so each ledger line records the commit range by hand.

Task 1: complete (commits 0f203b5..d75aabe, tests: cargo test logs:: → 6 passed; clippy, fmt clean). RED watched: classify returned None, tail returned empty; GREEN after implementing. Mutation check: inverting the `start > 0` guard flips 2 tests to red, so the tests bite. Ruling: `LogSource.kind` is the lowercase tag as `String`, not `LogKind`, so the JSON matches the frontend type directly — cost if wrong: one redundant conversion. Note: the 256 KB read window is a property of the code (reader is `File`, `.take` bounds it), not something a test observes; a slow first render on huge logs is the only risk and the window makes it unlikely.

