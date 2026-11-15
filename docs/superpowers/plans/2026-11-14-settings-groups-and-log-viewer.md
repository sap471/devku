# Settings Sidebar Groups and Service Log Viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put the Settings tabs under collapsible sidebar groups and give every service log file an in-app viewer with live refresh.

**Architecture:** A new `logs` module in Rust owns log-source classification and byte-bounded tail reads; `Session` delegates to it and exposes `log_sources`. The frontend gets a `LogsPane.vue` component under Settings, driven by a small set of store functions, and `SettingsView.vue` swaps its flat nav for grouped nav.

**Tech Stack:** Rust (Tauri 2, anyhow, serde), Vue 3 `<script setup>`, TypeScript, bun.

**Spec:** `docs/superpowers/specs/2026-11-14-settings-sidebar-and-log-viewer-design.md`

## Global Constraints

- Package manager is `bun`; never npm. Rust checks: `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check` (all with `--manifest-path src-tauri/Cargo.toml`). Frontend: `bun run check` and `bun run build`.
- No engine binary downloads during tests. All tests here are pure filesystem work in temp dirs.
- Text and UI copy in Indonesian (UI) and English (docs/code comments). No em dashes anywhere.
- Comments explain why, never what.
- Every behavioral change gets a `CHANGELOG.md` line under `[Unreleased]` with the commit hash.
- WCAG AA contrast; every data view has empty, loading, error, ready states.
- No `rust-version` bump, no new crates.

## Review Focus

- A log file larger than the tail window (1 MB `wordpress.log`, 302 KB `caddy.log`) must render without loading the whole file into memory, and must not start with a half-truncated line.
- A log file that does not exist yet (`php-<version>.log` is 0 bytes on this machine) must show an empty state, not an error.
- Clicking "Lihat Log" on a failed card must land on that exact file, and must work while a site log is open in the Sites tab.
- A collapsed sidebar group must never hide the active tab after a jump from another pane.
- The 2-second live poll must stop when the pane unmounts or the tab changes, so it cannot keep reading a file in the background.

---

### Task 1: Log classification and byte-bounded tail in Rust

**Files:**
- Create: `src-tauri/src/logs.rs`
- Modify: `src-tauri/src/lib.rs:16` (add `pub mod logs;` keeping alphabetical order between `location` and `path_env`)
- Test: inline `#[cfg(test)] mod tests` in `src-tauri/src/logs.rs`

**Interfaces:**
- Produces:
  - `pub enum LogKind { Server, Database, Php, Site }` with `pub fn tag(self) -> &'static str` returning `"server" | "database" | "php" | "site"`.
  - `pub fn classify(file_name: &str) -> Option<LogKind>`
  - `pub struct LogSource { pub name: String, pub kind: LogKind, pub bytes: u64, pub modified: Option<u64> }` (serde `Serialize`, field names snake_case as written)
  - `pub fn sources(log_dir: &Path) -> Vec<LogSource>` sorted by `modified` descending, ties broken by name ascending
  - `pub fn tail(path: &Path, lines: usize) -> Result<Vec<String>>`
  - `pub const TAIL_WINDOW: u64 = 256 * 1024;`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("devku-logs-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn classifies_every_file_the_supervisor_and_sites_write() {
        assert_eq!(classify("caddy.log"), Some(LogKind::Server));
        assert_eq!(classify("apache.log"), Some(LogKind::Server));
        assert_eq!(classify("apache-error.log"), Some(LogKind::Server));
        assert_eq!(classify("postgres.log"), Some(LogKind::Database));
        assert_eq!(classify("mariadb.log"), Some(LogKind::Database));
        assert_eq!(classify("mysql.log"), Some(LogKind::Database));
        assert_eq!(classify("php-8.5.12-1.log"), Some(LogKind::Php));
        assert_eq!(classify("apache-wordpress-access.log"), Some(LogKind::Site));
        assert_eq!(classify("wordpress.log"), Some(LogKind::Site));
        assert_eq!(classify("notes.txt"), None);
    }

    #[test]
    fn lists_only_log_files_with_size_and_time() {
        let dir = temp("sources");
        std::fs::write(dir.join("caddy.log"), "a\n").unwrap();
        std::fs::write(dir.join("postgres.log"), "b\nc\n").unwrap();
        std::fs::write(dir.join("ignore.txt"), "x").unwrap();

        let found = sources(&dir);
        let names: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"caddy.log"));
        assert!(names.contains(&"postgres.log"));
        let pg = found.iter().find(|s| s.name == "postgres.log").unwrap();
        assert_eq!(pg.kind.tag(), "database");
        assert_eq!(pg.bytes, 4);
        assert!(pg.modified.is_some());
    }

    #[test]
    fn missing_log_directory_is_empty_not_an_error() {
        let dir = temp("missing");
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(sources(&dir).is_empty());
        assert!(tail(&dir.join("postgres.log"), 10).unwrap().is_empty());
    }

    #[test]
    fn tail_returns_the_last_lines_in_order() {
        let dir = temp("tail");
        let path = dir.join("app.log");
        std::fs::write(&path, "1\n2\n3\n4\n5\n").unwrap();
        assert_eq!(tail(&path, 2).unwrap(), vec!["4".to_string(), "5".to_string()]);
        assert_eq!(tail(&path, 99).unwrap().len(), 5);
    }

    #[test]
    fn tail_over_the_window_drops_the_partial_first_line() {
        let dir = temp("window");
        let path = dir.join("big.log");
        // Each line is 100 bytes; the file is larger than TAIL_WINDOW.
        let line = format!("{}\n", "x".repeat(99));
        let count = (TAIL_WINDOW / 100) as usize + 500;
        std::fs::write(&path, line.repeat(count)).unwrap();

        let got = tail(&path, 10).unwrap();
        assert_eq!(got.len(), 10);
        assert!(got.iter().all(|l| l.len() == 99));
    }

    #[test]
    fn tail_never_returns_more_lines_than_requested_on_wide_files() {
        let dir = temp("wide");
        let path = dir.join("big.log");
        let line = format!("{}\n", "y".repeat(399));
        std::fs::write(&path, line.repeat(4000)).unwrap();
        assert_eq!(tail(&path, 5).unwrap().len(), 5);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml logs::`
Expected: compile error, `unresolved import` / `cannot find function classify`.

- [ ] **Step 3: Implement `src-tauri/src/logs.rs`**

Create the module with the types above. `classify` matches on exact names first (`caddy.log`, `apache.log`, `apache-error.log`, `postgres.log`, `mariadb.log`, `mysql.log`), then prefix rules (`php-` → `Php`, `apache-` and everything else ending in `.log` → `Site`), and returns `None` for anything not ending in `.log`.

`tail` opens the file, takes `metadata.len()`, and when it is over `TAIL_WINDOW` seeks to `len - TAIL_WINDOW` and discards bytes until the first `b'\n'` before reading. It always splits the remaining buffer with `split('\n')`, drops the trailing empty piece, and returns the last `lines` entries as `String`s with `\r` trimmed from each.

`sources` reads the directory, keeps entries where `classify` is `Some`, and builds `modified` from `metadata.modified()` as seconds since `UNIX_EPOCH` (`None` when unavailable).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml logs::`
Expected: `6 passed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/logs.rs src-tauri/src/lib.rs
git commit -m "feat(logs): classify log files and tail them without loading whole files"
```

---

### Task 2: Expose log sources and bounded tails through the session and commands

**Files:**
- Modify: `src-tauri/src/session.rs:1308-1319` (replace `tail_log` body), add `log_sources` next to it
- Modify: `src-tauri/src/commands.rs` (new `log_sources` command below `tail_log` at ~line 1468; new `"log"` arm inside `open_in`'s `match kind.as_str()`)
- Modify: `src-tauri/src/lib.rs:134` (register `commands::log_sources` after `commands::open_in`)
- Test: existing `#[cfg(test)] mod tests` in `src-tauri/src/session.rs`

**Interfaces:**
- Consumes: `logs::sources`, `logs::tail`, `logs::LogSource` from Task 1.
- Produces:
  - `Session::log_sources(&self) -> Vec<crate::logs::LogSource>`
  - `Session::tail_log(&self, name: &str, lines: usize) -> Result<Vec<String>>` (unchanged signature, new behavior)
  - Tauri command `log_sources() -> Result<Vec<logs::LogSource>, String>`
  - `open_in` accepts `kind: "log"` with `id` = log file name without `.log`, opening `runtime/logs/<id>.log` in the OS default app; returns the path.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/session.rs`'s test module. The module has no helper; build the session the same way `sync_workspace_registers_detected_folders_once` does: a temp root, `Paths { root }`, `paths.ensure()`, `Config::default().save(&paths)`, then `Session::new(paths.clone(), None)`.

```rust
#[test]
fn log_sources_and_tail_read_only_from_the_logs_folder() {
    let root = std::env::temp_dir().join("devku-session-logs");
    let _ = std::fs::remove_dir_all(&root);
    let paths = Paths { root: root.clone() };
    paths.ensure().unwrap();
    Config::default().save(&paths).unwrap();
    std::fs::create_dir_all(paths.logs()).unwrap();
    std::fs::write(paths.logs().join("postgres.log"), "one\ntwo\n").unwrap();
    std::fs::write(paths.logs().join("secret.txt"), "x").unwrap();

    let session = Session::new(paths.clone(), None).unwrap();
    let found = session.log_sources();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "postgres.log");
    assert_eq!(session.tail_log("postgres", 1).unwrap(), vec!["two".to_string()]);

    // A name with a path separator must not escape the logs folder.
    assert!(session.tail_log("../config", 5).is_err());

    let _ = std::fs::remove_dir_all(&root);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml log_sources_and_tail`
Expected: FAIL, `no method named log_sources`.

- [ ] **Step 3: Implement**

In `session.rs`, `log_sources` returns `crate::logs::sources(&self.paths.logs())`. `tail_log` first rejects `name` that contains `/`, `\`, or `..` with `bail!("nama log tidak valid")`, then returns `crate::logs::tail(&path, lines)`.

In `commands.rs`, add:

```rust
#[tauri::command]
pub fn log_sources(state: State<'_, AppState>) -> Result<Vec<crate::logs::LogSource>, String> {
    Ok(state.session.blocking_lock().as_ref().map(|s| s.log_sources()).unwrap_or_default())
}
```

If `blocking_lock` is not usable inside the async command wrapper, mirror the existing `tail_log` pattern instead: make it `pub async fn` and await the lock. Follow whichever the existing `tail_log` uses.

Add the `"log"` arm to `open_in`: keep `open_in`'s existing return type (`Result<(), String>`, confirmed at `commands.rs:1392`). Resolve `id` (required), reject `/`, `\`, and `..`, build `session.paths().logs().join(format!("{id}.log"))`, and when it is missing return `Err(format!("log {id} belum ada"))` like the `"db"` arm does. Opening a file rather than a folder cannot use `launcher::open`: run `std::process::Command::new("explorer.exe").arg(&path)` and spawn it after calling `crate::subprocess::hide_console_std(&mut command)` (the pattern `launcher.rs` already uses) so no console window flashes; on other platforms reuse `crate::launcher::open(path.parent(), "explorer")`. No return value.

Register the command in `lib.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all tests pass, including the new one (`cargo test` previously reported 146 passed).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/session.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(logs): expose log sources and bounded tails to the UI"
```

---

### Task 3: Store types, state, and loaders

**Files:**
- Modify: `src/types.ts` (add `LogSource` after `DownloadProgress` at line 97)
- Modify: `src/store/devku.ts:31` (`SettingsSubTab`), `:76` (initial tab), state block around `:84`, new functions next to `loadLog` at `:500`

**Interfaces:**
- Consumes: commands `log_sources`, `tail_log`.
- Produces:
  - `export interface LogSource { name: string; kind: "server" | "database" | "php" | "site"; bytes: number; modified: number | null }`
  - `export type SettingsSubTab = "php" | "database" | "caddy" | "apache" | "system" | "logs"`
  - state: `logSources: LogSource[]`, `logSourcesLoading: boolean`, `logSourcesError: string | null`, `serviceLogFor: string | null`, `serviceLogLines: string[]`, `serviceLogError: string | null`
  - `loadLogSources(): Promise<void>`
  - `loadServiceLog(name: string, lines: number): Promise<void>`
  - `openLog(name: string): void`
  - `openLogFile(name: string): Promise<void>`

- [ ] **Step 1: Add the type and state, then wire the loaders**

Implement `loadLogSources` setting `logSourcesLoading` around the invoke, clearing `logSourcesError` on success and storing the message on failure. `loadServiceLog` calls `invoke<string[]>("tail_log", { name, lines })` and writes `serviceLogFor`, `serviceLogLines`, `serviceLogError` only if `serviceLogFor` still equals `name` after the await, so a fast switch does not show the wrong file. `openLog` sets `state.activeTab = "settings"`, `state.settingsTab = "logs"`, `state.serviceLogFor = name`, then `void loadLogSources()` and `void loadServiceLog(name, 200)`. `openLogFile` calls `invoke("open_in", { kind: "log", id: name.replace(/\.log$/, ""), target: "explorer" })` and writes `state.notice` on failure.

Change the initial `settingsTab` from `"php"` to `"php"` (unchanged value, new type permits it).

- [ ] **Step 2: Verify it typechecks**

Run: `bun run check`
Expected: FAIL is acceptable here only if the failure names a `"server"` string in `SettingsView.vue`; that file is fixed in Task 5. Note the error text so Task 5's step can confirm it clears.

- [ ] **Step 3: Commit**

```bash
git add src/types.ts src/store/devku.ts
git commit -m "feat(store): log source types, state, and loaders"
```

---

### Task 4: LogsPane component

**Files:**
- Create: `src/components/settings/LogsPane.vue`
- Test: none (no frontend test runner in this repo); verified by `bun run check`, `bun run build`, and a live check in Task 7.

**Interfaces:**
- Consumes: `state.logSources`, `state.logSourcesLoading`, `state.logSourcesError`, `state.serviceLogFor`, `state.serviceLogLines`, `state.serviceLogError`, `loadLogSources`, `loadServiceLog`, `openLogFile` from Task 3.
- Produces: default-exported component with no props, mounted by `SettingsView.vue` for `settingsTab === "logs"`.

- [ ] **Step 1: Build the pane**

Layout, top to bottom:

1. `pane-header` with title "Log Layanan" and a description naming the folder (`state.snapshot?.home_path` `/ runtime/logs`).
2. A source list, grouped under headings "Server", "Database", "PHP", "Situs". Each row is a button showing the file name (monospace), the size (`formatBytes`), and the modified time (local `HH:MM` on the same day, otherwise `YYYY-MM-DD`). Rows are `aria-current="true"` when selected.
3. Toolbar for the selected file: filter buttons "Semua" / "Galat" / "Peringatan", a search input with `aria-label="Cari di log"`, a line-count select (200 / 1000 / 5000), a Live toggle, a "Segarkan" button, and "Buka file".
4. Viewport: `<pre class="log-view">` with one `<div>` per line, `white-space: pre-wrap`. Lines matching `/FATAL|PANIC|ERROR|Level Log\s*:\s*Fatal/" ], "level":"error"|\[error\]|AH\d+/` get class `log-error`; lines matching `/WARNING|Warning|\[warn\]/` get `log-warn`.

State handling: when `logSourcesLoading` show a "Memuat daftar log..." block; when `logSourcesError` show the error text plus a "Coba lagi" button calling `loadLogSources()`; when there are no sources show "Belum ada file log. Log muncul setelah layanan dijalankan."; when the selected file has zero lines show "File ini kosong. Layanan belum menulis apa pun sejak terakhir dibersihkan."

Auto-select the most recently modified source when `state.serviceLogFor` is null or not in the list, via `watch` on `logSources`, calling `loadServiceLog`.

Live: a `setInterval` of 2000 ms kept in a local ref; `onMounted` starts it only when a file is selected, `onBeforeUnmount` clears it, and a `watch` on `state.settingsTab` stops it when the tab is not `"logs"`. Use the same guard flag pattern as `SiteDetail.vue` (`liveBusy`) so overlapping ticks are skipped.

- [ ] **Step 2: Verify it typechecks and builds**

Run: `bun run check` then `bun run build`
Expected: both succeed, or `bun run check` reports only the `"server"` string error that Task 5 removes.

- [ ] **Step 3: Commit**

```bash
git add src/components/settings/LogsPane.vue
git commit -m "feat(settings): service log pane with filters, live poll, and source list"
```

---

### Task 5: Grouped sidebar and tab rename in SettingsView

**Files:**
- Modify: `src/components/settings/SettingsView.vue:173-245` (sidebar), `:668-728` (`server` pane becomes `caddy`), `:248-1089` (mount `LogsPane` for `logs`), scoped CSS after `:1123`
- Test: none; verified by `bun run check`, `bun run build`, and live check in Task 7.

**Interfaces:**
- Consumes: `SettingsSubTab` with `"caddy"` and `"logs"` from Task 3; `LogsPane.vue` from Task 4.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Replace the flat nav with three groups**

Groups and their tabs: `runtime` → `php`; `data` → `database`, `caddy`, `apache`; `app` → `system`, `logs`. Each group is a `<button class="nav-group" :aria-expanded="openGroups[group]" :aria-controls="'group-' + group">` with a chevron that rotates when open, followed by a `<div :id="'group-' + group" v-show="openGroups[group]" class="nav-group-items">` holding the existing `nav-item` buttons.

`openGroups` is `reactive({ runtime: true, data: true, app: true })`. `setTab(tab)` sets `state.settingsTab` and also forces the owning group open, so a jump from elsewhere never lands on a hidden tab.

Active tab buttons get `aria-current="page"`.

- [ ] **Step 2: Rename the Caddy tab value**

Replace every `'server'`/`"server"` tab comparison and `setTab('server')` with `'caddy'`. The pane's section stays where it is; only the condition changes.

- [ ] **Step 3: Mount the Log tab**

Add `<section v-else-if="state.settingsTab === 'logs'" class="tab-pane tab-pane-wide"><LogsPane /></section>` as the last pane, import the component, and add a `.tab-pane-wide { max-width: none; }` rule so the log lines get the full width.

- [ ] **Step 4: Add "Lihat Log" to service cards**

On each database card foot (next to "Hapus"), a quiet button "Lihat Log" calling `openLog(db.engine)` from the store. On the Caddy pane header and the Apache pane header, the same button calling `openLog('caddy')` and `openLog('apache')` respectively. On a card whose `status === 'failed'`, the existing error row gets a "Buka log" link button in place of the plain text, same `openLog` call.

- [ ] **Step 5: Verify**

Run: `bun run check` then `bun run build`
Expected: both succeed with no errors. The `"server"` error noted in Task 3 no longer appears.

- [ ] **Step 6: Commit**

```bash
git add src/components/settings/SettingsView.vue
git commit -m "feat(settings): grouped sidebar with Log tab and log shortcuts on service cards"
```

---

### Task 6: Changelog

**Files:**
- Modify: `CHANGELOG.md` under `[Unreleased]`

- [ ] **Step 1: Add three lines**

Under **Added**: the Log tab entry naming the sources it lists and that reads tails without loading whole files; the sidebar groups entry naming the three groups. Under **Changed**: the Caddy tab value renamed from `server` to `caddy` (internal only, not user-facing, so mention only if it changes behavior; if not, omit).

Each line ends with the commit hashes from Tasks 1-5.

- [ ] **Step 2: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs(changelog): settings groups and service log viewer"
```

---

### Task 7: Verification and installer

- [ ] **Step 1: Full Rust check**

Run: `cargo test --manifest-path src-tauri/Cargo.toml` , `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`, `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
Expected: tests pass (146 plus the new ones), clippy and fmt clean.

- [ ] **Step 2: Frontend check**

Run: `bun run check` and `bun run build`
Expected: both succeed.

- [ ] **Step 3: Live check against the real logs folder**

Run: `\.\scripts\dev\test-home.ps1` first if a test home is needed; for this check use the real install because the existing logs (`postgres.log`, 302 KB `caddy.log`, 1 MB `wordpress.log`) are the point. Close any running DevKu, then `bun tauri dev`. Confirm in Settings > Log that `postgres.log`, `mariadb.log`, `caddy.log`, `apache-error.log`, and the per-site logs are listed with real sizes and times, that selecting `wordpress.log` renders within a second (tail window, not whole file), that `postgres.log` shows the `FATAL` lines in error color, and that "Lihat Log" on the PostgreSQL card lands on `postgres.log`. Collapse the "Data & Server" group while on Database, then jump from a site to Settings > Log and confirm the group reopened.

- [ ] **Step 4: Rebuild the installer**

Run: `bun tauri build`
Expected: `src-tauri/target/release/bundle/nsis/devku_0.1.0_x64-setup.exe` with a fresh timestamp. Report the path and time to the user.
