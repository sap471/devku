# Add Database Engine (UI) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user add PostgreSQL to the database engine list from Settings > Database instead of hand-editing `config.json`.

**Architecture:** A `Session` method appends a `DbEngineSettings` entry to `config.database.engines` (validated, saved), a Tauri command exposes it, a store action calls it, and a quiet button appears in the Database pane only while PostgreSQL is not configured yet.

**Tech Stack:** Tauri 2 commands (Rust), Vue 3 store/actions, existing `DatabaseSettings` validation.

**Spec:** `docs/superpowers/specs/2026-11-14-add-database-engine-design.md`

## Global Constraints

- Errors are user-facing Indonesian, consistent with existing commands.
- New entry: `port = engine.default_port()` (5432 for postgres), `autostart = false`.
- No MySQL offer anywhere: its backend is not implemented (documented debt).
- Repo rules: TDD, `cargo fmt` + `clippy` clean, `bun run check` clean, changelog under `[Unreleased]` with commit hash, no em dashes in docs/comments.

## Review Focus

- Double invocation (double click) must not create a duplicate entry: pinned by `add_database_engine_refuses_duplicates_and_mysql`.
- MySQL must stay unreachable through the command: pinned by the same test.
- MariaDB re-add while MySQL is configured must fail on the exclusivity rule: pinned by `add_database_engine_respects_mysql_family_exclusivity`.
- Legacy-shaped `config.json` (`{"port":3306,"autostart":true}`) must still gain a working postgres entry and be rewritten in the new shape: pinned inside `add_database_engine_appends_postgres_with_defaults`.

---

### Task 1: `Session::add_database_engine` with tests

**Files:**
- Modify: `src-tauri/src/session.rs` (near `set_database_autostart`, around line 970; tests at the `mod tests` block, line 1405+)
- Test: same file, `mod tests`

**Interfaces:**
- Consumes: `DatabaseEngine` (`crate::database::DatabaseEngine`, methods `key()`, `default_port()`), `DbEngineSettings { engine, port, autostart }` (`crate::config`), `DatabaseSettings::{entry, validate}`.
- Produces: `pub async fn add_database_engine(&mut self, engine: DatabaseEngine) -> anyhow::Result<()>`. Task 2 relies on this exact name and signature.

- [ ] **Step 1: Write the failing tests** (append to `mod tests` in `session.rs`, following the temp-home pattern of `sync_workspace_registers_detected_folders_once`)

```rust
#[tokio::test]
async fn add_database_engine_appends_postgres_with_defaults() {
    let root = std::env::temp_dir().join(format!("devku-add-db-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let paths = Paths { root: root.clone() };
    paths.ensure().unwrap();

    // A config in the legacy single-engine shape, like the one on a real
    // upgraded machine, must still gain the entry and rewrite cleanly.
    std::fs::write(
        paths.config_path(),
        r#"{"version":1,"backend":"caddy","tld":"localhost","autostart":true,"database":{"port":3306,"autostart":true}}"#,
    )
    .unwrap();

    let mut session = Session::new(paths.clone(), None).unwrap();
    session.add_database_engine(DatabaseEngine::Postgres).await.unwrap();

    let entry = session.config().database.entry(DatabaseEngine::Postgres).unwrap();
    assert_eq!(entry.port, 5432);
    assert!(!entry.autostart);

    let reloaded = Config::load(&paths).unwrap();
    assert!(reloaded.database.entry(DatabaseEngine::Mariadb).is_some());
    assert!(reloaded.database.entry(DatabaseEngine::Postgres).is_some());
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn add_database_engine_refuses_duplicates_and_mysql() {
    let root = std::env::temp_dir().join(format!("devku-add-db2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let paths = Paths { root: root.clone() };
    paths.ensure().unwrap();
    Config::default().save(&paths).unwrap();
    let mut session = Session::new(paths.clone(), None).unwrap();

    assert!(session
        .add_database_engine(DatabaseEngine::Mysql)
        .await
        .unwrap_err()
        .to_string()
        .contains("belum didukung"));

    session.add_database_engine(DatabaseEngine::Postgres).await.unwrap();
    assert!(session.add_database_engine(DatabaseEngine::Postgres).await.is_err());
    assert_eq!(session.config().database.engines.len(), 2);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn add_database_engine_respects_mysql_family_exclusivity() {
    let root = std::env::temp_dir().join(format!("devku-add-db3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let paths = Paths { root: root.clone() };
    paths.ensure().unwrap();
    let mut config = Config::default();
    config.database.engines = vec![crate::config::DbEngineSettings {
        engine: DatabaseEngine::Mysql,
        port: 3306,
        autostart: false,
    }];
    config.save(&paths).unwrap();
    let mut session = Session::new(paths.clone(), None).unwrap();

    assert!(session.add_database_engine(DatabaseEngine::Mariadb).await.is_err());
    assert!(session.config().database.entry(DatabaseEngine::Mariadb).is_none());
    let _ = std::fs::remove_dir_all(&root);
}
```

Note: if `Config::load` or the config field name differs (check `config_path` / `Config::load(&paths)` usage at the top of `session.rs`), follow the file's existing helpers; the assertions above are the contract.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml add_database_engine`
Expected: FAIL, `add_database_engine` not found.

- [ ] **Step 3: Implement `add_database_engine`** in `session.rs` next to `set_database_autostart`

```rust
/// Adds an engine to the configured list. The card, install button, and
/// supervision all key off this list, so this is the only door a new engine
/// needs. MySQL stays out until its backend exists.
pub async fn add_database_engine(&mut self, engine: DatabaseEngine) -> Result<()> {
    if engine == DatabaseEngine::Mysql {
        bail!("MySQL belum didukung di versi ini");
    }
    if self.config.database.entry(engine).is_some() {
        bail!("{} sudah ada di daftar engine", engine.key());
    }
    self.config.database.engines.push(DbEngineSettings {
        engine,
        port: engine.default_port(),
        autostart: false,
    });
    self.config.database.validate()?;
    self.config.save(&self.paths)?;
    self.refresh().await
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml add_database_engine`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/session.rs
git commit -m "feat(db): add_database_engine appends a validated engine entry"
```

### Task 2: command, store action, and button

**Files:**
- Modify: `src-tauri/src/commands.rs` (add the command near `set_database_autostart`, line 1139; register in `src-tauri/src/lib.rs` invoke handler, line 116)
- Modify: `src/store/devku.ts` (near `installDatabase`)
- Modify: `src/components/settings/SettingsView.vue` (database pane header around line 487)

**Interfaces:**
- Consumes: `Session::add_database_engine(engine: DatabaseEngine) -> Result<()>` (Task 1).
- Produces: Tauri command `add_database_engine(engine) -> Snapshot`; store action `addDatabaseEngine(engine?: string)`.

- [ ] **Step 1: Add the command** in `commands.rs`, mirroring `set_database_autostart`'s shape (lock session, call method, map error, return `session.snapshot().await`)

```rust
#[tauri::command]
pub async fn add_database_engine(
    state: State<'_, AppState>,
    engine: DatabaseEngine,
) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.add_database_engine(engine).await.map_err(error)?;
    Ok(session.snapshot().await)
}
```

Register `commands::add_database_engine,` in the invoke handler in `lib.rs`.

- [ ] **Step 2: Add the store action** in `src/store/devku.ts` next to `installDatabase`, same busy/notice pattern, `state.busy = "add-db"`, invoke `add_database_engine` with `{ engine }`, `applySnapshot(next)`, info notice `"PostgreSQL ditambahkan. Pasang binernya dari kartu di bawah."`, error notice `Gagal menambah ${engine}: ...`.

- [ ] **Step 3: Add the button** in `SettingsView.vue`: import `addDatabaseEngine`; inside the database pane's `.pane-header` add a button shown only while postgres is absent:

```html
<button
  v-if="!database.some((db) => db.engine === 'postgres')"
  class="btn btn-sm"
  type="button"
  :disabled="state.busy === 'add-db'"
  @click="addDatabaseEngine('postgres')"
>
  Tambah PostgreSQL
</button>
```

- [ ] **Step 4: Verify**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`, `cargo fmt --manifest-path src-tauri/Cargo.toml`, `bun run check`, `bun run build`.
Expected: all exit 0, test count 139 + 3 = 142 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/store/devku.ts src/components/settings/SettingsView.vue
git commit -m "feat(ui): add PostgreSQL to the engine list from Settings"
```

### Task 3: changelog and installer bundle

**Files:**
- Modify: `CHANGELOG.md` (`[Unreleased]`, Added section)
- Build: `src-tauri/target/release/bundle/nsis/devku_0.1.0_x64-setup.exe`

**Interfaces:**
- Consumes: commits from Tasks 1 and 2.

- [ ] **Step 1: Add the changelog entry** under `[Unreleased] > Added`: one entry stating PostgreSQL can now be added from Settings > Database with a button, that the entry starts on port 5432 with autostart off, and that MySQL stays out until its backend exists.

- [ ] **Step 2: Commit docs, then append the hash**

```bash
git add CHANGELOG.md
git commit -m "docs(changelog): add database engine entry"
```
Then append the commit hash to the entry in a follow-up docs commit, per repo convention.

- [ ] **Step 3: Build the NSIS bundle** so the user's installer carries everything from today

Run: `bun tauri build`
Expected: a fresh `devku_0.1.0_x64-setup.exe` in `src-tauri/target/release/bundle/nsis/`, timestamp updated.
