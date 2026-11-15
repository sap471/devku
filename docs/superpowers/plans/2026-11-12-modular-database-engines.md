# Modular Database Engines Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn DevKu's de facto MariaDB-only `DatabaseBackend` into a genuinely engine-neutral trait, add a config-declared list of database engines with an exclusivity rule between the MySQL family and PostgreSQL, and ship PostgreSQL as the first second engine.

**Architecture:** Follow the two patterns DevKu already proves: the `WebServerBackend` trait with two implementations (`CaddyBackend`, `ApacheBackend`) chosen by an enum, and the `PhpManager` catalog with per-version folders. The trait becomes engine-neutral; the five MariaDB-shaped methods move down into `Mariadb` as inherent methods. Engines are declared in `config.json` as an explicit list, with a validation rule that at most one MySQL-family engine may be enabled while PostgreSQL may run alongside.

**Tech Stack:** Rust (Tauri 2), serde, anyhow, tokio, `process-wrap`; Vue 3 frontend; PostgreSQL binaries from EDB (zip archives).

**Spec:** `docs/research/2026-11-12-modular-database-engines.md` (design research) and `docs/research/2026-11-12-database-management.md` (the broader database-management research). The plan argues from those two files; executors should read both.

## Global Constraints

- Design principles from `PLAN.md`, every task implicitly includes them: "Idle processes do not run", "Lightweight is a feature, not a bonus", "Configuration as text", "Loopback only", installer target ≤ 15 MB without PHP.
- Comments explain *why*, never *what*; no decorative comments (AGENTS.md).
- Every behavioral change is recorded in `CHANGELOG.md` under `[Unreleased]` with the commit hash (AGENTS.md).
- Config stays human-readable and diffable; no em dashes in any user-facing text (AGENTS.md).
- Rust: `cargo fmt` and `cargo clippy --all-targets` must be clean before each commit.
- Frontend: `bun run check` (tsc --noEmit) must exit 0.
- Tests must not download engine binaries. Use `.\scripts\dev\test-home.ps1` (AGENTS.md); the download quota rule is binding.
- PostgreSQL source: EDB zip archives. Verified working URL pattern `https://get.enterprisedb.com/postgresql/postgresql-<version>-1-windows-x64-binaries.zip` (HTTP 200 confirmed for 18.6 and 17.11 on 2026-11-12). **EDB publishes no checksum for these archives**, so the existing recorded-digest path in `src-tauri/src/download.rs` (`archive_is_trusted`, sidecar `.sha256`) is used, the same way Bun already is.
- PostgreSQL license: PostgreSQL License (permissive). MySQL server license: GPL-2.0. MariaDB: GPL-2.0. All open source, as the owner required.
- PostgreSQL pin: `18.6` (current stable Windows x64 at plan time). MySQL Community pin is **out of scope for this plan** (see Task 12 note).

## Review Focus

The spec implies these input classes that no single task's tests fully cover; each is pinned to a test in the task that owns the code.

1. **Two MySQL-family engines both enabled** — a reasonable user may check both MariaDB and MySQL in the UI expecting them to coexist; the expected behavior is a clear refusal naming the conflict, never two servers fighting over port 3306. Pinned in Task 3.
2. **PostgreSQL and MariaDB both enabled** — the expected behavior is both start independently on their own ports (3306 / 5432) with no interference; pinned in Task 4 and Task 10.
3. **A configured engine whose binaries are missing** — the engine must report `installed: false` and degrade, never panic or block Caddy/PHP; pinned in Task 8.
4. **An engine that fails to start** — other services keep running and the failure is surfaced per engine, preserving today's `database_error` behavior; pinned in Task 9.
5. **A pre-existing `config.json` with no engine list** — must load as MariaDB/3306 and round-trip without data loss; pinned in Task 2.

---

### Task 1: Make `DatabaseBackend` engine-neutral, move MariaDB-only methods down

**Files:**
- Modify: `src-tauri/src/database.rs` (trait at lines 35-61, `impl` at lines 170-380)
- Test: `src-tauri/src/database.rs` (`#[cfg(test)] mod tests`, existing module)

**Interfaces:**
- Consumes: existing `Paths`, `ServiceSpec`, `Mariadb`.
- Produces:
  - `pub struct Conn { pub host: String, pub port: u16, pub user: String, pub secret: String }`
  - `pub enum DatabaseEngine { Mariadb, Mysql, Postgres }` with `#[serde(rename_all = "lowercase")]`, `Default` = `Mariadb`
  - trait method set: `engine(&self) -> DatabaseEngine`, `label`, `version`, `bin_dir`, `data_dir`, `is_installed`, `is_initialized`, `initialize(u16)`, `has_server(&self) -> bool`, `spec(&self, u16) -> Result<Option<ServiceSpec>>`, `query(&self, conn: &Conn, sql: &str) -> Result<String>`, `user_databases(&self, conn: &Conn) -> Result<Vec<String>>`, `system_schemas(&self) -> &'static [&'static str]`, `dump(&self, conn: &Conn, dest: &Path) -> Result<()>`, `restore(&self, conn: &Conn, src: &Path) -> Result<()>`, `wipe`
  - `impl Mariadb` inherent: `recovery_spec(&self, u16) -> Result<ServiceSpec>`, `password(&self) -> String`, `store_password(&self, &str) -> Result<()>`, `root_hosts(&self, u16, &str) -> Result<Vec<String>>`, `set_password(&self, u16, &str, &str) -> Result<()>`

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` in `database.rs`:

```rust
#[test]
fn mariadb_reports_engine_and_has_a_server() {
    let paths = Paths { root: std::env::temp_dir().join("devku-plan-t1") };
    let backend = Mariadb::new(paths);
    assert_eq!(backend.engine(), DatabaseEngine::Mariadb);
    assert!(backend.has_server());
    assert!(backend.system_schemas().contains(&"mysql"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml mariadb_reports_engine_and_has_a_server`
Expected: FAIL, `no method named engine` / `has_server not found`.

- [ ] **Step 3: Add `Conn`, `DatabaseEngine`, and reshape the trait**

In `database.rs`, add `DatabaseEngine` (serde derives as above) and `Conn`. Change the trait: remove `id` (replaced by `engine`), change `spec` to return `Result<Option<ServiceSpec>>`, add `has_server`, add `system_schemas`, add `restore`, and re-shape `query`, `user_databases`, `dump` to take `&Conn`. Remove `recovery_spec`, `password`, `store_password`, `root_hosts`, `set_password` from the trait.

- [ ] **Step 4: Move the five methods into `impl Mariadb`**

Change `impl DatabaseBackend for Mariadb` accordingly, and give `Mariadb` inherent versions of the five removed methods (bodies unchanged, just moved out of the trait block). `spec` returns `Ok(Some(self.spec_with(port, true)?))`. `has_server` returns `true`. `system_schemas` returns `SYSTEM_SCHEMAS`. `query` uses `conn.port` / `conn.secret`; `root_hosts`/`set_password` keep their current call shape by constructing a `Conn` internally, or take `&Conn` and adjust callers in Task 8.

- [ ] **Step 5: Update the six call sites in `session.rs` so the crate compiles**

At `session.rs` lines 351, 872, 889, 937, 979, 1013 the code calls trait methods on `Mariadb`. Because the five moved methods are now inherent, those calls keep working; only `query`/`user_databases`/`dump` calls need a `Conn`. Build `Conn { host: "127.0.0.1".into(), port, user: "root".into(), secret: password.into() }` at those sites.

- [ ] **Step 6: Run tests and lints**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all pass, including the new test.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/database.rs src-tauri/src/session.rs
git commit -m "refactor(db): make DatabaseBackend engine-neutral"
```

---

### Task 2: Add the engine list to config with serde defaults

**Files:**
- Modify: `src-tauri/src/config.rs` (`DatabaseSettings` at lines 99-123, `Config` at line 92)
- Test: `src-tauri/src/config.rs` (existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `DatabaseEngine` from Task 1.
- Produces:
  - `pub struct DbEngineSettings { pub engine: DatabaseEngine, pub port: u16, pub autostart: bool }` with `#[serde(default)]` on every field and `Default` returning MariaDB/3306/false.
  - `pub struct DatabaseSettings { #[serde(default)] pub engines: Vec<DbEngineSettings> }` plus `impl DatabaseSettings { pub fn engine(&self, engine: DatabaseEngine) -> Option<&DbEngineSettings>; pub fn validate(&self) -> anyhow::Result<()> }`
  - `validate` refuses when both `Mariadb` and `Mysql` entries are enabled. "Enabled" means present in the list.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn old_config_without_engines_loads_as_mariadb() {
    let raw = r#"{"version":1,"database":{"port":3306,"autostart":true}}"#;
    let config: Config = serde_json::from_str(raw).unwrap();
    assert_eq!(config.database.engines.len(), 1);
    assert_eq!(config.database.engines[0].engine, DatabaseEngine::Mariadb);
    assert_eq!(config.database.engines[0].port, 3306);
    assert!(config.database.engines[0].autostart);
}

#[test]
fn maria_and_mysql_together_is_refused() {
    let settings = DatabaseSettings {
        engines: vec![
            DbEngineSettings { engine: DatabaseEngine::Mariadb, port: 3306, autostart: true },
            DbEngineSettings { engine: DatabaseEngine::Mysql, port: 3307, autostart: true },
        ],
    };
    assert!(settings.validate().is_err());
}

#[test]
fn postgres_alongside_maria_is_accepted() {
    let settings = DatabaseSettings {
        engines: vec![
            DbEngineSettings { engine: DatabaseEngine::Mariadb, port: 3306, autostart: true },
            DbEngineSettings { engine: DatabaseEngine::Postgres, port: 5432, autostart: true },
        ],
    };
    assert!(settings.validate().is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml old_config_without_engines_loads_as_mariadb`
Expected: FAIL, `DatabaseSettings` has no field `engines`.

- [ ] **Step 3: Implement the config types**

Replace `DatabaseSettings` with the list shape. The custom `Deserialize` must treat a legacy `{"port":..,"autostart":..}` object (no `engines`) as a single MariaDB entry: the simplest correct route is `#[serde(default)]` on `engines` plus a custom `Default` for `DatabaseSettings` that yields one MariaDB entry, so a missing field fills from `Default`. Ensure `validate` implements the exclusivity rule and returns an error message naming both engines.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS for the three new tests and no regression in `old_config_without_autostart_defaults_to_on`.

- [ ] **Step 5: Note the config change in the changelog**

Add under `CHANGELOG.md` `[Unreleased]` a line describing the new `database.engines` list and that old configs load as MariaDB. Hash is filled after commit, as existing entries do.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/config.rs CHANGELOG.md
git commit -m "feat(config): database engine list with mysql-family exclusivity"
```

---

### Task 3: Enforce exclusivity at the config boundary

**Files:**
- Modify: `src-tauri/src/config.rs` (`Config::validate` or wherever the current validation lives)
- Test: `src-tauri/src/config.rs`

**Interfaces:**
- Consumes: `DatabaseSettings::validate` from Task 2.
- Produces: `Config::validate` calls `self.database.validate()?` so a bad file is rejected on load, not at start time.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn config_validate_rejects_two_mysql_family_engines() {
    let mut config = Config::default();
    config.database.engines = vec![
        DbEngineSettings { engine: DatabaseEngine::Mariadb, port: 3306, autostart: false },
        DbEngineSettings { engine: DatabaseEngine::Mysql, port: 3307, autostart: false },
    ];
    assert!(config.validate().is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml config_validate_rejects_two_mysql_family_engines`
Expected: FAIL if `Config::validate` does not yet delegate, PASS if it does (then keep the test as a guard).

- [ ] **Step 3: Wire `Config::validate` to delegate**

Add `self.database.validate().context("pengaturan database tidak valid")?;` in the right place.

- [ ] **Step 4: Run tests, fmt, clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`
Expected: tests pass, fmt clean, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs
git commit -m "feat(config): reject two mysql-family engines on load"
```

---

### Task 4: Parameterise `Paths` by engine

**Files:**
- Modify: `src-tauri/src/paths.rs` (lines 106-133, the `mariadb_*` methods)
- Test: `src-tauri/src/paths.rs` (add a `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `DatabaseEngine` from Task 1.
- Produces:
  - `pub fn database_dir(&self, engine: DatabaseEngine) -> PathBuf` → `engines/database/<engine>`
  - `pub fn database_bin(&self, engine: DatabaseEngine) -> PathBuf`
  - `pub fn database_data(&self, engine: DatabaseEngine) -> PathBuf` → `databases/<engine>`
  - `pub fn database_runtime(&self, engine: DatabaseEngine) -> PathBuf` → `runtime/database/<engine>`
  - `pub fn database_config(&self, engine: DatabaseEngine, file: &str) -> PathBuf`
  - `pub fn database_secret(&self, engine: DatabaseEngine) -> PathBuf`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn database_paths_are_engine_scoped() {
    let paths = Paths { root: std::path::PathBuf::from("R") };
    assert!(paths.database_bin(DatabaseEngine::Mariadb).ends_with("engines/database/mariadb/bin") || cfg!(windows));
    assert!(paths.database_data(DatabaseEngine::Postgres).ends_with("databases/postgres"));
    assert_ne!(
        paths.database_data(DatabaseEngine::Mariadb),
        paths.database_data(DatabaseEngine::Postgres)
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml database_paths_are_engine_scoped`
Expected: FAIL, `no method named database_bin`.

- [ ] **Step 3: Implement the engine-scoped path methods**

Add the six methods. Keep the old `mariadb_*` methods only if something outside `database.rs`/`session.rs` still calls them; otherwise remove them and update callers (`database.rs` uses `mariadb_dir`, `mariadb_bin`, `mariadb_data`, `mariadb_config`, `mariadb_password`; `commands.rs` lines 223-225 and 1028, 1037; `download.rs` line 405-409). `database.rs` should call `self.paths.database_bin(DatabaseEngine::Mariadb)` etc.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS, no regression.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/paths.rs src-tauri/src/database.rs src-tauri/src/commands.rs src-tauri/src/download.rs
git commit -m "refactor(paths): engine-scoped database paths"
```

---

### Task 5: Add a `backend_for` factory

**Files:**
- Modify: `src-tauri/src/database.rs` (append)
- Test: `src-tauri/src/database.rs`

**Interfaces:**
- Consumes: `DatabaseEngine`, `Paths`, `Mariadb`.
- Produces: `pub fn backend_for(paths: &Paths, engine: DatabaseEngine) -> Box<dyn DatabaseBackend>`. Returns `Box::new(Mariadb::new(paths.clone()))` for `Mariadb`; for `Mysql` and `Postgres` it returns `Err`-free but not-yet-implemented backends in later tasks (Task 11 replaces the placeholder for `Postgres`; `Mysql` stays unimplemented and is out of scope).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn backend_for_returns_engine_matching_request() {
    let paths = Paths { root: std::env::temp_dir().join("devku-plan-t5") };
    let backend = backend_for(&paths, DatabaseEngine::Mariadb);
    assert_eq!(backend.engine(), DatabaseEngine::Mariadb);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml backend_for_returns_engine_matching_request`
Expected: FAIL, `backend_for not found`.

- [ ] **Step 3: Implement `backend_for`**

Match on `DatabaseEngine`. For the two not-yet-written arms, return a backend type created in the later task; until then this task may only wire `Mariadb` and leave the other arms for the tasks that add them (compile with `unimplemented!()` is **not** acceptable in a committed task: instead add the `match` with only the `Mariadb` arm plus a `todo!()` is disallowed by clippy's `unimplemented` lint only when enabled; if so, return `Box::new(Mariadb::new(...))` for all arms in this task and replace the `Postgres` arm in Task 11).

- [ ] **Step 4: Run tests and lints**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`
Expected: pass and clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/database.rs
git commit -m "feat(db): backend_for factory mirroring caddy/apache selection"
```

---

### Task 6: Config migration round-trip

**Files:**
- Test: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/config.rs` only if the test reveals a gap

**Interfaces:**
- Consumes: Task 2's types.
- Produces: nothing new; this task proves no silent data loss.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn database_engines_round_trip_stably() {
    let config = Config::default();
    let text = serde_json::to_string_pretty(&config).unwrap();
    let again: Config = serde_json::from_str(&text).unwrap();
    assert_eq!(again.database.engines, config.database.engines);
    assert!(text.contains("\"engine\": \"mariadb\""));
}
```

- [ ] **Step 2: Run test to verify it fails or passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml database_engines_round_trip_stably`
Expected: FAIL if `DbEngineSettings` lacks `PartialEq`; add `PartialEq` to the derives as the fix.

- [ ] **Step 3: Fix any gap the test exposes**

Add missing derives; ensure `to_string_pretty` writes one line per field so the file stays diffable.

- [ ] **Step 4: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs
git commit -m "test(config): database engines round-trip stably"
```

---

### Task 7: `Runtime` holds a map of databases

**Files:**
- Modify: `src-tauri/src/runtime.rs` (struct at lines 20-34, start at lines 62-73, `ensure_database`/`stop_database`/`start_database_spec`)
- Test: `src-tauri/src/runtime.rs` if it has tests, otherwise `src-tauri/src/ports.rs`-style unit test where practical

**Interfaces:**
- Consumes: `backend_for` (Task 5), `DatabaseSettings` (Task 2), `Config::validate` (Task 3).
- Produces:
  - `Runtime.databases: BTreeMap<DatabaseEngine, Supervisor>` (was `database: Option<Supervisor>`)
  - `Runtime.database_present: BTreeSet<DatabaseEngine>` (was `bool`)
  - `Runtime.database_errors: BTreeMap<DatabaseEngine, String>` (was `database_error: Option<String>`)
  - `ensure_database(&mut self, engine: DatabaseEngine, port: u16, paths: &Paths, hook: Option<Hook>)`
  - `stop_database(&mut self, engine: DatabaseEngine)`
  - `start_database_spec(&mut self, engine: DatabaseEngine, spec: ServiceSpec, hook: Option<Hook>)`

- [ ] **Step 1: Write the failing test**

This is a structural change with a behavior test at the `Config` level; add to `config.rs` or a runtime test:

```rust
#[test]
fn two_engines_can_be_present_with_distinct_ports() {
    let mut config = Config::default();
    config.database.engines = vec![
        DbEngineSettings { engine: DatabaseEngine::Mariadb, port: 3306, autostart: false },
        DbEngineSettings { engine: DatabaseEngine::Postgres, port: 5432, autostart: false },
    ];
    let ports: std::collections::BTreeSet<u16> = config.database.engines.iter().map(|e| e.port).collect();
    assert_eq!(ports.len(), 2);
    assert!(config.validate().is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails or passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml two_engines_can_be_present_with_distinct_ports`
Expected: PASS once Task 2 types exist; keep as a guard.

- [ ] **Step 3: Convert `Runtime` to the map shape**

Replace the three fields, adjust `Runtime::start` to iterate `config.database.engines`, calling `backend_for` per entry and only creating a `Supervisor` when `backend.is_installed() && backend.is_initialized()` and `entry.autostart` and `backend.has_server()` (idle-processes rule). Record failures into `database_errors` keyed by engine; never let one engine's failure abort the loop (preserve the existing "database never blocks sites" comment and behavior).

- [ ] **Step 4: Update `session.rs` DB methods to pass the engine**

`start_database`, `stop_database`, `ensure_database` calls now name an engine. For now they operate on the single configured MySQL-family engine or a named engine; the UI tasks (Task 10) drive which. Keep `prepare_database` generic over the configured list.

- [ ] **Step 5: Run tests, fmt, clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`
Expected: pass and clean.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/runtime.rs src-tauri/src/session.rs
git commit -m "refactor(runtime): supervise database engines per engine"
```

---

### Task 8: `DatabaseView` becomes a list, degrade per engine

**Files:**
- Modify: `src-tauri/src/session.rs` (`DatabaseView` at lines 91-107, snapshot build at lines 351-372)
- Test: `src-tauri/src/session.rs`

**Interfaces:**
- Consumes: Task 7's runtime map.
- Produces:
  - `pub struct DatabaseView { pub engine: String, pub label: String, pub version: String, pub installed: bool, pub initialized: bool, pub has_server: bool, pub port: u16, pub autostart: bool, pub status: String, pub detail: Option<String>, pub data_path: String, pub bin_path: String, pub has_password: bool }` (one per engine)
  - `Snapshot.database: Vec<DatabaseView>` (was single `DatabaseView`)
  - missing binaries path: `installed: false`, `status: "stopped"`, no panic.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn missing_engine_reports_not_installed_without_panic() {
    let paths = Paths { root: std::env::temp_dir().join("devku-plan-t8") };
    let backend = backend_for(&paths, DatabaseEngine::Mariadb);
    assert!(!backend.is_installed());
    assert!(!backend.is_initialized());
}
```

- [ ] **Step 2: Run test to verify it fails or passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml missing_engine_reports_not_installed_without_panic`
Expected: PASS (guards the degrade behavior); keep it.

- [ ] **Step 3: Build one `DatabaseView` per configured engine**

Loop over `config.database.engines` in `snapshot()`, construct the backend via `backend_for`, and fill the view from it plus the runtime map. Set `has_server` from `backend.has_server()`.

- [ ] **Step 4: Run tests and lints, then `bun run check` will fail until Task 10**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: Rust passes. The frontend type mismatch is expected and fixed in Task 10.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/session.rs
git commit -m "feat(session): one database view per configured engine"
```

---

### Task 9: One engine's failure never blocks sites

**Files:**
- Modify: `src-tauri/src/runtime.rs`
- Test: `src-tauri/src/runtime.rs` or `src-tauri/src/config.rs`

**Interfaces:**
- Consumes: Task 7.
- Produces: `Runtime::database_errors()` accessor used by `session.rs` to fill `detail`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn database_error_is_kept_per_engine() {
    let mut errors = std::collections::BTreeMap::new();
    errors.insert(DatabaseEngine::Postgres, "gagal mulai".to_string());
    assert_eq!(errors.get(&DatabaseEngine::Postgres).map(String::as_str), Some("gagal mulai"));
    assert!(errors.get(&DatabaseEngine::Mariadb).is_none());
}
```

- [ ] **Step 2: Run test to verify it fails or passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml database_error_is_kept_per_engine`
Expected: PASS; keep as a guard for the map-shape decision.

- [ ] **Step 3: Ensure the start loop records and continues**

Verify the loop in `Runtime::start` writes `database_errors.insert(engine, format!("{err:#}"))` and does not `?` out.

- [ ] **Step 4: Run tests, fmt, clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`
Expected: pass and clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/runtime.rs
git commit -m "test(runtime): per-engine failure isolation"
```

---

### Task 10: Frontend, database tab becomes an engine list

**Files:**
- Modify: `src/types.ts` (`DatabaseView` near line 236 and its `database` field)
- Modify: `src/store/devku.ts` (database actions near lines 772-915, `SettingsSubTab` at line 30)
- Modify: `src/components/settings/SettingsView.vue` (Database tab at lines 484+, tab label at line 199)
- Test: `bun run check`

**Interfaces:**
- Consumes: `Snapshot.database` as `DatabaseView[]` from Task 8.
- Produces: UI that lists each configured engine with per-engine install / start / stop / autostart, mirroring the existing PHP versions list; a validation notice when the user tries to enable both MariaDB and MySQL.

- [ ] **Step 1: Update `src/types.ts`**

Change `database: DatabaseView` to `database: DatabaseView[]` and add `has_server: boolean` to `DatabaseView`.

- [ ] **Step 2: Update the store**

Make `installDatabase`, `databaseAction`, `toggleDatabaseAutostart`, `resetDatabasePassword`, `askRemoveDatabase` accept an `engine` key. Add `setDatabaseEngines(engines)` if the UI edits the list. Surface the exclusivity error from the backend as `state.notice`.

- [ ] **Step 3: Update the Settings tab**

Render one row per engine. Hide start/stop/autostart for engines with `has_server === false`. Add the four mandatory states (empty, loading, error, ready) for the engine list as `DESIGN.md` requires.

- [ ] **Step 4: Run the typecheck and build**

Run: `bun run check`
Expected: exit 0.
Run: `bun run build` (or the repo's build script)
Expected: succeeds.

- [ ] **Step 5: Commit**

```bash
git add src/types.ts src/store/devku.ts src/components/settings/SettingsView.vue
git commit -m "feat(ui): database settings lists engines"
```

---

### Task 11: PostgreSQL backend

**Files:**
- Create: `src-tauri/src/postgres.rs`
- Modify: `src-tauri/src/database.rs` (`backend_for` Postgres arm, `VERSION`-style const)
- Modify: `src-tauri/src/download.rs` (add `postgres_archive_url(version)`, reuse the recorded-digest path)
- Modify: `src-tauri/src/commands.rs` (install command dispatches on engine)
- Modify: `src-tauri/src/lib.rs` (module registration)
- Test: `src-tauri/src/postgres.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: everything above.
- Produces:
  - `pub struct Postgres { paths: Paths }` implementing `DatabaseBackend`
  - `pub const POSTGRES_VERSION: &str = "18.6"` in `postgres.rs`
  - `pub fn archive_url(version: &str) -> String` → `https://get.enterprisedb.com/postgresql/postgresql-<version>-1-windows-x64-binaries.zip`
  - `is_installed`: `bin/postgres.exe` and `bin/initdb.exe` exist
  - `is_initialized`: `data/PG_VERSION` exists
  - `initialize(port)`: run `initdb -D <data> -U postgres -A scram-sha-256 -E UTF8`, then write `postgresql.conf` (port, `listen_addresses = '127.0.0.1'`) and `pg_hba.conf` (loopback `scram-sha-256`); DevKu writes both files, as it does `my.ini` for MariaDB
  - `has_server` returns `true`
  - `spec(port)`: `ServiceSpec` running `postgres.exe -D <data> -p <port>` with `cwd = bin_dir`
  - `query(conn, sql)`: `psql -h 127.0.0.1 -p <port> -U <user> -d postgres -t -A -c <sql>` with `PGPASSWORD` set from `conn.secret`
  - `user_databases(conn)`: `SELECT datname FROM pg_database WHERE datistemplate = false AND datname <> 'postgres' ORDER BY datname`
  - `system_schemas()`: `&["template0", "template1", "postgres"]`
  - `dump(conn, dest)`: `pg_dump -h 127.0.0.1 -p <port> -U <user> -Fp -f <dest> postgres` (plain format keeps the cross-engine backup contract identical)
  - `restore(conn, src)`: `psql -h 127.0.0.1 -p <port> -U <user> -d postgres -f <src>`
  - `wipe()`: remove `data_dir()`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn postgres_archive_url_pins_edb_pattern() {
    assert_eq!(
        archive_url("18.6"),
        "https://get.enterprisedb.com/postgresql/postgresql-18.6-1-windows-x64-binaries.zip"
    );
}

#[test]
fn postgres_reports_engine_and_system_schemas() {
    let paths = Paths { root: std::env::temp_dir().join("devku-plan-t11") };
    let backend = Postgres::new(paths);
    assert_eq!(backend.engine(), DatabaseEngine::Postgres);
    assert!(backend.has_server());
    assert!(backend.system_schemas().contains(&"template1"));
}

#[test]
fn postgres_not_installed_on_empty_home() {
    let paths = Paths { root: std::env::temp_dir().join("devku-plan-t11b") };
    let backend = Postgres::new(paths);
    assert!(!backend.is_installed());
    assert!(!backend.is_initialized());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml postgres_`
Expected: FAIL, `Postgres` not found.

- [ ] **Step 3: Implement `Postgres`**

Follow the shape documented in the Interfaces block. Reuse `crate::subprocess::run_hidden`. For `query`, pass `PGPASSWORD` through the process environment; `run_hidden` currently takes no env (see `database.rs` usage), so extend it or use the variant that accepts env, mirroring how `ServiceSpec.env` already carries env for supervised processes.

- [ ] **Step 4: Wire `backend_for` and the install command**

Replace the `Postgres` arm in `backend_for` with `Box::new(Postgres::new(paths.clone()))`. In `commands.rs`, dispatch `install_database` on the engine from the request; for PostgreSQL, download via `download::download` with `expected = None` so the recorded-digest path applies (EDB has no checksum), then extract and verify `bin/initdb.exe`.

- [ ] **Step 5: Run tests, fmt, clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`
Expected: pass and clean. No test downloads binaries.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/postgres.rs src-tauri/src/database.rs src-tauri/src/download.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(db): postgresql engine backend"
```

---

### Task 12: Documentation and changelog

**Files:**
- Modify: `CHANGELOG.md`
- Modify: `README.md` (database section) and `PLAN.md` if the phase list needs a line

**Interfaces:**
- Consumes: all prior tasks.
- Produces: accurate docs. Explicitly states **MySQL Community server is not implemented** in this plan; only MariaDB, MySQL-with-exclusivity, and PostgreSQL. This keeps scope honest.

- [ ] **Step 1: Write the changelog entries**

Under `[Unreleased]`, list: engine-neutral trait, engine list config with MySQL-family exclusivity, PostgreSQL support, engine-scoped paths and runtime. Include commit hashes after committing.

- [ ] **Step 2: Update README**

Add a short "Databases" subsection: MariaDB and MySQL are mutually exclusive; PostgreSQL runs alongside; engines are downloaded on demand and verified (MariaDB via published SHA-256, PostgreSQL via recorded digest because EDB publishes none). Respect the no-em-dash rule.

- [ ] **Step 3: Run the full verification**

Run: `cargo test --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets && bun run check`
Expected: all green.

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG.md README.md PLAN.md
git commit -m "docs: modular database engines"
```

---

## Self-Review

**1. Spec coverage:**
- "Support 3 engines, all open source" → Task 11 (PostgreSQL) plus existing MariaDB; MySQL is deliberately deferred and stated as such in Task 12. **This is a scope reduction the owner must confirm** (see open item below).
- "MySQL/MariaDB exclusive, choose one" → Task 2 + Task 3.
- "PostgreSQL can run alongside" → Task 2 test `postgres_alongside_maria_is_accepted`, Task 7, Task 9.
- "Refactor + PostgreSQL" → Tasks 1-9 (refactor), Task 11 (PostgreSQL).
- Research "do not build" list (no plugin API, no client crate, no per-engine Adminer, no `-Fd`, no port auto-allocation) → respected; nothing in this plan builds them.

**2. Step scan:** each step names one action with a checkable result. Task 5 Step 3 flags a real ambiguity (what the not-yet-implemented arms return) rather than hiding it; Task 11 Step 3 names the `run_hidden` env gap explicitly.

**3. Type consistency:** `DatabaseEngine` variants `Mariadb`/`Mysql`/`Postgres` are used consistently. `DbEngineSettings` (config) vs `DatabaseView` (session) vs `DatabaseEngine` (engine id) are three distinct names used consistently. `Conn` fields are `host/port/user/secret` throughout. `backend_for(paths, engine)` signature stable from Task 5 to Task 11.

**4. Review Focus:** all five items appear with a pinning test: (1) Task 3, (2) Task 2/Task 7/Task 9, (3) Task 8, (4) Task 9, (5) Task 2.

**5. Proportion:** the plan is longer than a summary because it names exact files, signatures, and tests; it does not reproduce function bodies except where the signature leaves a real choice (PostgreSQL `initialize`, `query`).

**Open item for the owner:** the owner asked for three engines including MySQL Community Server. This plan implements the trait, the exclusivity rule, and PostgreSQL, but defers the actual MySQL backend because the research could not verify a first-party, checksummed, machine-readable MySQL Community ZIP endpoint (`docs/research/2026-11-12-modular-database-engines.md`, open question 1). MySQL shares the `Mariadb` driver almost entirely, so it is cheap once that endpoint is confirmed, but shipping an unverified download URL would break DevKu's trust invariant. Task 12 documents the deferral.
