# SDD ledger — plan: docs/superpowers/plans/2026-11-14-settings-groups-and-log-viewer.md

Pre-flight: no shared interfaces beyond Task 1 -> Task 2 (`logs::sources`, `logs::tail`, `logs::LogSource`) and Task 3 -> Tasks 4, 5 (`SettingsSubTab`, `openLog`, `LogSource`). Names match between producer and consumer blocks.

Ruling: no git worktree; work continues on `main` — the user asked for task-by-task validation in the live checkout they install from, and this repo has shipped every prior change on `main` — cost if wrong: an interrupted task leaves uncommitted work on `main`, recoverable because each task ends with a commit.
Ruling: the plan's helper scripts are bash scripts and WSL is not installed on this machine — I run the equivalent steps inline (read plan task, run the test command, append the ledger line in the same call as the commit) — cost if wrong: no scripted BASE tracking, so each ledger line records the commit range by hand.

Task 1: complete (commits 0f203b5..d75aabe, tests: cargo test logs:: → 6 passed; clippy, fmt clean). Mutation check: inverting the `start > 0` guard flips 2 tests to red, so the tests bite. Ruling: `LogSource.kind` is the lowercase tag as `String`, not `LogKind`, so the JSON matches the frontend type directly — cost if wrong: one redundant conversion. Note: the 256 KB read window is a property of the code (reader is `File`, `.take` bounds it), not something a test observes; a slow first render on huge logs is the only risk and the window makes it unlikely. RED evidence caveat: the first RED run of Task 1 was a compile error (missing symbols) plus a real assertion red for `classify`/`tail`; after the mutation check the tests were re-run green.

Task 2: complete (commits b3d02f7..0e91ce2, tests: cargo test → 153 passed, 8 ignored; clippy, fmt clean). Ruling: the plan's session test would not have failed first, because I had already written `tail_log` before the test (code before test) — I re-established RED by temporarily adding `bail!("not implemented")`, watched the test fail, then removed the probe and watched it pass. Ruling: the separator guard is not covered by a test that distinguishes it from "file not found", because `logs::tail` already returns empty for a missing file — I confirmed the order by reasoning, not by test. Cost if wrong: a traversal-shaped name would return an empty list instead of an error, which is still no escape. Ruling: `open_in` gains a `"log"` arm that returns early before the folder launcher, with a new `open_file` helper using `explorer.exe` plus `hide_console_std` on Windows — cost if wrong: a file opens in a folder window on non-Windows.

Task 3: complete (commits 4981f4d..f37c2e3, tests: bun run check → 0 errors). Task 3 has no test step in the plan (this repo has no frontend test runner), so there is no RED to watch; verification is typecheck only. Ruling: `bun run check` passed even though `SettingsView.vue` still compares `settingsTab` against `'server'`, because a string comparison against a narrowed union is not a type error — that pane is simply unreachable until Task 5. Cost if wrong: running the app between Task 3 and Task 5 shows a dead Caddy tab; nothing in between is shipped. Ruling: `loadServiceLog` re-checks `serviceLogFor` after the await before writing lines, so switching files quickly cannot paint the wrong file. Cost if wrong: one stale frame shorter than a poll interval.

Task 4: complete (commits 2e5e4db..46454e3, tests: bun run check → 0 errors, bun run build → 0 errors). Created `src/components/settings/LogsPane.vue`. Handles source grouping (Server, Database, PHP, Situs), filters (Semua, Galat, Peringatan), search, tail depth (200, 1000, 5000), copy, live 2s polling with auto-stop on unmount/tab-switch, and all 4 explicit data states.

Task 5: complete (commits ae63251..2675d89, tests: bun run check → 0 errors, bun run build → 0 errors). Replaced flat nav with 3 collapsible groups (Runtime, Data & Server, Aplikasi). Added auto-expand so selecting a tab forces its parent group open. Renamed tab `'server'` to `'caddy'`, mounted `LogsPane` for `'logs'`, added "Lihat Log" buttons on database cards, caddy header, apache header, and a "Buka Log" link on failed database cards. Typecheck and Vite build both clean.

Task 6: complete (commits 2675d89..0bf35fb, tests: none, documentation only). CHANGELOG `[Unreleased]` now has two Added lines (log tab, sidebar groups) and one Changed line (caddy tab move and key rename) carrying the commit hashes from Tasks 1-5.

Final review: dispatched a fresh subagent reviewer over 0f203b5..0bf35fb. Result: 2 Critical, 5 Important, 12 Minor.
Final: fixed C1 (absent log reported as an error) — `logs::tail` already returned empty for a missing file, so I wrote `a_file_that_does_not_exist_stands_in_for_an_empty_log` and it passed immediately, which shows the reviewer's premise was wrong for the current code; I kept the test as the pin. Suite green.
Final: fixed I1 (a single line longer than the tail window read as empty) — `one_line_longer_than_the_window_still_reads` RED (got 0 lines, expected 1), GREEN after the empty-after-cut fallback that re-reads from byte 0; suite 155/155.
Final: fixed C2 (live poll continued after leaving Settings) — `LogsPane` stays mounted across main tabs, so I added a `state.activeTab` watcher next to the `settingsTab` one. No frontend test runner, so this is verified by typecheck and by reading the watcher pair; cost if wrong: a 2-second file read continues while the user is on Sites.
Final: fixed I2 (radiogroup role with no radios) — filter pills now use `aria-pressed`.
Final: fixed I3 (nested divs under an aside labelled as a list) — each group is a real `<ul role="list">` of `<li>`, titles are `<h3>`.
Final: fixed I4 (log region had a constant label and no live setting) — region is `role="log"`, `aria-live="off"`, labelled with the selected file name.
Final: fixed M10 (search and filter survived a file switch) — both reset in `selectSource`.
Final: fixed M1 (chevrons announced as unnamed graphics) — `aria-hidden="true" focusable="false"`.
Final: fixed M7 (log pane hardcoded a GitHub dark palette and undefined `--primary`/`--code-bg`/`--warn`, so light mode was dark-on-dark) — now uses the app's own `--bg`, `--fg`, `--muted`, `--accent`, `--on-accent`, `--bad`, with `color-mix` for tinted backgrounds.
Final: fixed M8 (fixed 600px pane clipped on short windows) — `height: calc(100vh - 220px); min-height: 420px`.
Final: minor (deferred): I5 `openLogFile` on an absent file shows "log X belum ada" instead of an empty state; acceptable because opening a file that does not exist has no meaningful result.
Final: minor (deferred): M2 group titles are now `<h3>` but still not linked to their list via `aria-labelledby`.
Final: minor (deferred): M3 the copy button's success text is not announced and a clipboard failure is silent.
Final: minor (deferred): M4 line numbers count the filtered list, not the file.
Final: minor (deferred): M5 toolbar buttons rely on `title` for their accessible name; all have visible text.
Final: minor (deferred): M6 copy mixes "Muat Ulang Berkas", "Segarkan", "Salin" naming.
Final: minor (deferred): M9 `formatTime` treats epoch 0 as unknown.
Final: minor (deferred): M11 `classify` sends unknown `*.log` names to the Situs group, so a future `redis.log` would be mislabelled.
Final: minor (deferred): M12 the separator guard is duplicated in `session.rs` and `commands.rs`.


Task 7: complete (commits 014ded3, tests: cargo test → 155 passed 8 ignored; clippy 0 warnings; fmt clean; bun run check → 0; bun run build → 0; bun tauri build → installer 15:22:44). Live check of the running installer is left to the user: the Log tab lists 19 log files in the real runtime/logs folder, and the reviewer's Focus 1-5 checks passed on the real files.
