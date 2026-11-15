# Modular database engines for DevKu: is multi-engine worth it, and what is the honest cost?

Date: 2026-11-12
Scope: the **modular / multi-engine** aspect of the database feature only. It refines one question left open in `docs/research/2026-11-12-database-management.md` (which covers comparators, Adminer vs phpMyAdmin, and backup/restore pitfalls). This file does **not** repeat that material; where it references an Adminer or CLI-client decision, it points at that file.

Verified against DevKu source (read-only):
- `src-tauri/src/database.rs`: `DatabaseBackend` trait (line 35) with a single implementation `Mariadb` (line 63). `VERSION = "11.4.13"` pinned from `https://archive.mariadb.org`, SHA-256 verified. `archive_name()` → `mariadb-<ver>-winx64.zip`.
- `src-tauri/src/runtime.rs`: `Runtime` has a single `database: Option<Supervisor>` (line 29); `Mariadb::new(paths.clone())` is constructed at line 62 (not a map, not a trait object).
- `src-tauri/src/session.rs`: `Mariadb::new(self.paths.clone())` at lines 351, 872, 889, 937, 979, 1013 (six literal constructions). `DatabaseView` (line 94) is built from that one backend.
- `src-tauri/src/config.rs`: `DatabaseSettings` (line 101) has only `port` (default 3306) and `autostart` (default false).
- `src-tauri/src/paths.rs`: hardcoded `mariadb_dir()`, `mariadb_bin()`, `mariadb_data()`, `mariadb_runtime()`, `mariadb_config()` (`my.ini`), `mariadb_password()` (`root.pass`).
- `src-tauri/src/php.rs`: `PhpManager` — catalog + folder scan + per-version dirs. THE precedent for multi-version engines.
- `src-tauri/src/commands.rs`: `php_catalog` / `build_catalog` (line 334) — release list merged with folders on disk.
- `src-tauri/src/caddy.rs`: `WebServerBackend` trait (line 15); `src-tauri/src/apache.rs`: `ApacheBackend`. THE precedent for a pluggable backend trait with two implementations.
- `src/components/settings/SettingsView.vue`: tab label literally `Database (MariaDB)` (line 199).
- `PLAN.md`: "Idle processes do not run", "Lightweight is a feature, not a bonus", "Configuration as text", installer target ≤ 15 MB without PHP.

---

## Summary

DevKu's `DatabaseBackend` is **not** engine-neutral. It is a MariaDB driver wearing a trait name. Thirteen of its fourteen methods are either generic lifecycle (`id`, `label`, `version`, `bin_dir`, `data_dir`, `is_installed`, `is_initialized`, `initialize`, `spec`, `wipe`) or MariaDB-specific (`recovery_spec` → `--skip-grant-tables`; `password`/`store_password` → a root account; `root_hosts` → `mysql.user`; `set_password` → `ALTER USER 'root'@host`; `dump` → `mariadb-dump` with `--databases`).

What actually differs between engines is not one or two flags. It is a different **auth model**, a different **first-run step**, a different **dump tool and format**, and a different **config file set**. PostgreSQL has two config files (`postgresql.conf` + `pg_hba.conf`), an `initdb` bootstrap step, role-based auth with no `user@host` rows, and `pg_dump -Fc` custom/binary archives that `mariadb-dump` has no analogue for. SQLite has none of these: no server, no port, no init, no daemon, no dump tool (`.dump` is a shell dot-command). MariaDB and MySQL are close enough to share a driver but differ in install layout, the Windows initializer name, system schema list, and (MySQL 8.0) the auth plugin default.

**Recommendation in one line:** adopt **Shape A (curated multi-engine catalog)** — make the trait engine-neutral, drop the MariaDB-shaped methods down into the implementation, add an `engine` discriminant to config, and ship exactly the engines DevKu writes, tests, and SHA-256-verifies. Do **not** build a public plugin API (Shape B). The honest cost ranking is **SQLite (trivial, no server) < MySQL (cheap, shares MariaDB's driver) < PostgreSQL (expensive, new auth + init + dump model)**. Build SQLite first only if "no server" is the goal; otherwise build **PostgreSQL second** because it is the engine whose PHP story (`pdo_pgsql`, already in `DEFAULT_EXTENSIONS`) actually diverges from MariaDB, and treat MySQL as a low-cost third.

---

## Engine differences

Everything below is from first-party docs. Where a figure could not be reached, it says so.

### Install layout and archive format on Windows

| Engine | Windows distribution DevKu would download | Format | Layout after extract |
|---|---|---|---|
| MariaDB 11.4 | `mariadb-<ver>-winx64.zip` from `archive.mariadb.org` (already done) | zip | `bin/` must stay next to `server.dll` (verified in `paths.rs`) |
| MySQL 8.x | MySQL Community Server "ZIP Archive" (no installer) | zip | `bin/`, `lib/`, `share/`; no `server.dll` stub split |
| PostgreSQL 18 | **EDB "binaries" zip** (`postgresql-<ver>-windows-x64-binaries.zip`) or the EDB interactive installer | zip | `bin/` (`initdb.exe`, `postgres.exe`, `pg_ctl.exe`, `psql.exe`, `pg_dump.exe`), `lib/`, `share/` |
| SQLite 3.54 | `sqlite-tools-win-x64-*.zip` (CLI) or `sqlite-dll-win-x64-*.zip` (DLL) | zip | flat; `sqlite3.exe` (CLI) and/or `sqlite3.dll` |

Primary sources:
- EDB binaries page (zip archives of the files installed by the EDB installer, "intended for users who wish to include Postgres as part of another application installer"): https://www.enterprisedb.com/download-postgresql-binaries
- PostgreSQL Windows download page (points at EDB installer; names the zip alternative): https://www.postgresql.org/download/windows/
- SQLite download page (exact Windows artifacts and sizes: `sqlite-tools-win-x64-3540100.zip` 6.66 MiB; `sqlite-dll-win-x64-3540100.zip` 1.38 MiB; SQLite 3.54.1): https://www.sqlite.org/download.html

**Could not verify from a primary source:** an official, stable, checksummed URL list for the MySQL Community "ZIP Archive" as a machine-fetchable endpoint. MySQL's download page is a JS-driven portal; no first-party JSON index was reached in this pass. Treat the MySQL archive URL pattern as unverified until a first-party mirror with checksums is confirmed.

### Init / first-run step

| Engine | First-run command | Notes |
|---|---|---|
| MariaDB (Windows) | `mariadb-install-db.exe` | "initializes the MariaDB data directory and creates the system tables in the `mysql` database"; creates `my.ini` in the data directory and "appends the server settings it generates (such as `datadir` and `port`)". Can also register a Windows service and set the root password. |
| MySQL (Windows) | `mysqld --initialize` / `--initialize-insecure` (MySQL 8) | mysqli layout is MySQL's own; **not verified against a first-party MySQL page in this pass**. |
| PostgreSQL | `initdb -D <dir>` (also `pg_ctl initdb`) | "Creating a database cluster consists of creating the directories ... generating the shared catalog tables ... and creating the `postgres`, `template1`, and `template0` databases." `initdb` "must be run as the user that will own the server process" and "will in fact refuse to [run as root]". Uses `-A`/`--auth` to set the default `pg_hba.conf` method. |
| SQLite | **none** | "If the named file does not exist, a new database file with the given name will be created automatically." |

Primary sources:
- `mariadb-install-db.exe` on Windows (parameters `-d/--datadir`, `-p/--password`, `-P/--port`, `-S/--service`, `-c/--config` template): https://mariadb.com/docs/server/server-management/install-and-upgrade-mariadb/installing-mariadb/installing-system-tables-mariadb-install-db/mariadb-install-db-exe
- `mariadb-install-db` (Unix) — "The Windows version `mariadb-install-db.exe` ... has a lot of functionality specific to Windows ... does *not* share command line parameters with the Unix shell script": https://mariadb.com/docs/server/clients-and-utilities/deployment-tools/mariadb-install-db
- `initdb` (PostgreSQL 18): https://www.postgresql.org/docs/current/app-initdb.html
- `pg_ctl` (`init`/`initdb` mode, Windows `register`/`unregister`): https://www.postgresql.org/docs/current/app-pg-ctl.html
- SQLite CLI ("Starting the CLI"): https://www.sqlite.org/cli.html

### Server binary and control

| Engine | Server binary | Control | Daemon? |
|---|---|---|---|
| MariaDB | `mysqld.exe` | `--defaults-file=my.ini --console`, optional `--skip-grant-tables` | yes |
| PostgreSQL | `postgres.exe` (invoked directly or via `pg_ctl`/`postgres -D`) | `pg_ctl start -D <datadir> -l <logfile> -o "-p <port>"`; readiness via `postmaster.pid`; `pg_ctl stop -m fast` | yes |
| SQLite | **none** | n/a — the process is the caller | no |

`pg_ctl` is documented as encapsulating "redirecting log output and properly detaching from the terminal and process group"; `start` waits by polling `postmaster.pid` until "the PID file indicates that the server is ready to accept connections". Source: https://www.postgresql.org/docs/current/app-pg-ctl.html

**Consequence for DevKu:** the existing `ServiceSpec { program, args, port, log_dir, .. }` + `Supervisor::start` model works for MariaDB and PostgreSQL. SQLite has **no `ServiceSpec` at all** — it is a file plus a client process. That is a structural mismatch, not a config flag.

### Config file format and name

| Engine | Config file(s) |
|---|---|
| MariaDB | `my.ini` (DevKu renders it; `[mysqld]`, `basedir`, `datadir`, `port`, `bind-address`, `skip-name-resolve`, `plugin-dir`, `character-sets-dir`) |
| PostgreSQL | `postgresql.conf` (server settings incl. `port`, `listen_addresses`) **plus** `pg_hba.conf` (client authentication) — separate files, separate reload semantics |
| SQLite | none (values via `PRAGMA` or URI query parameters like `?mode=ro`) |

PostgreSQL: `pg_hba.conf` "is read on start-up and when the main server process receives a SIGHUP signal", reload via `pg_ctl reload` / `pg_reload_conf()`. Windows note from the same page: "any changes in the `pg_hba.conf` file are immediately applied by subsequent new connections." Source: https://www.postgresql.org/docs/current/auth-pg-hba-conf.html

SQLite URI parameters (`mode=ro|rw|rwc|memory`, `cache=shared|private`, `nolock`, `immutable`) are documented per-connection, not file-level. Source: https://www.sqlite.org/c3ref/open.html

### Default port

| Engine | Default port |
|---|---|
| MariaDB / MySQL | **3306** (DevKu's `default_db_port()`) |
| PostgreSQL | **5432** ("the port specified at compile time, usually 5432") — https://www.postgresql.org/docs/current/app-psql.html |
| SQLite | none |

DEVKU docs are Indonesian, but the repo comment for 3306 already notes Laravel `.env` files expect it. Note the conflict risk: PostgreSQL on 5432 while another engine also runs is fine, but two MySQL-family engines cannot both claim 3306.

### Authentication model

| Engine | Model |
|---|---|
| MariaDB / MySQL | Accounts are `user@host` rows in `mysql.user`. DevKu already handles the "usable root host" problem (`skip-name-resolve`, `ALTER USER 'root'@host`, `localhost`/`%`/IP rows) — see `Mariadb::usable_root_host`. |
| PostgreSQL | Server-side `pg_hba.conf` records (`host all all 127.0.0.1/32 <method>`), methods `trust`/`scram-sha-256`/`md5`/`password`/`reject`/... plus **roles** (no `user@host`; a role has no host component). Cluster superuser is set by `initdb -U` and defaults to the OS user running `initdb`. |
| SQLite | none (file permissions only) |

Primary source: https://www.postgresql.org/docs/current/auth-pg-hba-conf.html

**This is the single biggest architectural divergence.** DevKu's `password()`, `store_password()`, `root_hosts()`, `set_password()`, `recovery_spec()` all exist because of the MySQL `user@host` model. PostgreSQL has no equivalent: privilege setup happens at `initdb -A trust` / `-A scram-sha-256` time and in `pg_hba.conf`, and password changes are `ALTER ROLE ... PASSWORD` against an already-running server. These five methods do not map onto PostgreSQL at all; they must move *down* into `Mariadb`, not stay on the trait.

### Running arbitrary SQL from the CLI

| Engine | Command shape |
|---|---|
| MariaDB | `mariadb.exe --host=127.0.0.1 --port=<p> --user=root --password=<pw> --connect-timeout=5 --batch --skip-column-names --execute=<sql>` (DevKu's exact shape) |
| PostgreSQL | `psql.exe -h 127.0.0.1 -p <p> -U <role> -d <db> -t -A -c "<sql>"`; `-t/--tuples-only` and `-A/--no-align` give the bare-values form DevKu relies on |
| SQLite | `sqlite3.exe <file.db> "<sql>"` with `.mode list`/`.mode csv`/`-json` etc.; `sqlite_schema` lists tables |

Sources: https://www.postgresql.org/docs/current/app-psql.html ; https://www.sqlite.org/cli.html

### Dump and restore

| Engine | Dump tool | Formats | Restore |
|---|---|---|---|
| MariaDB | `mariadb-dump` | plain SQL only (`--databases`, `--routines`, `--events`, `--single-transaction`) | `mariadb < file.sql` |
| PostgreSQL | `pg_dump` | **plain** (`-Fp`), **custom** (`-Fc`), **directory** (`-Fd`, only format supporting parallel `-j`), tar (`-Ft`) | `psql -f` for plain; `pg_restore -d <db>` for archive formats |
| SQLite | shell `.dump` (a dot-command, not a separate binary) | plain SQL | pipe back into `sqlite3` |

PostgreSQL primary source (format list, `-Fc`/`-Fd` "most flexible ... compressed by default", `pg_dump` dumps *one* database only, `pg_dumpall` for the whole cluster): https://www.postgresql.org/docs/current/app-pgdump.html
SQLite `.dump` / `.recover`: https://www.sqlite.org/cli.html

**Consequence:** DevKu's `dump()` signature `(port, password, dest: &Path)` assumes a single plain-text file. `pg_dump -Fc` produces a binary archive and `-Fd` produces a **directory**, and PostgreSQL has no "dump all databases into one file" (that is `pg_dumpall`, which dumps a cluster, not a chosen DB list). The trait must grow a format/artifact concept, or `dump` must return what it actually produced.

### Single-file web admin tool

- **Adminer** is database-agnostic and **supports PostgreSQL and SQLite**, not only MySQL. The multi-driver `adminer-<ver>.php` (~584 KB) drives MySQL/MariaDB, PostgreSQL, SQLite, Oracle, MSSQL, Elasticsearch, etc.; the MySQL-only build (~441 KB) does not. Source: https://www.adminer.org/en/ and the earlier file `docs/research/2026-11-12-database-management.md`.
- **phpMyAdmin** is MySQL/MariaDB-only and not single-file. Source: https://www.phpmyadmin.net/downloads/

So the "one browser for all engines" answer already exists in Adminer, **provided DevKu ships the all-driver build**, which is a small (~140 KB) increase over the MySQL-only build. This removes one whole class of per-engine work.

---

## How other tools make DB pluggable

| Tool | Abstraction shape | Real plugin API? | What the user picks |
|---|---|---|---|
| **DDEV** | config enum → **Docker image tag** | No | `database: {type, version}` in `.ddev/config.yaml`; `type` ∈ `mariadb`/`mysql`/`postgres` (plus add-on types `mongo`, `sqlsrv`). The "plugin" is a container image (`ddev/ddev-dbserver`) selected by type+version. |
| **Laragon** | curated folder list | No | Docs claim multi-version support for "Apache, Nginx, PHP, MySQL, PostgreSQL, Node.js, Python, Ruby, Golang" with a `Menu > <Tool> > Version` switcher; you drop a version into `bin\<tool>`. |
| **Herd (Laravel)** | curated service list, Pro-only UI | No | Ships PHP extensions for all major DBs; SQLite needs no service. MySQL/MariaDB/PostgreSQL/MongoDB/Redis are Herd **Pro** services, or you bring your own (e.g. DBngin). |
| **Laravel Sail** | `compose.yaml` service entries | No | Each engine is its own container service; `sail:install` / `sail:add` writes the service into `compose.yaml`. |

Primary sources:
- DDEV config `database` + `dbimage`: https://docs.ddev.com/en/stable/users/configuration/config/ ; database types page: https://docs.ddev.com/en/stable/users/extend/database-types/
- Laragon multi-version: https://laragon.org/docs/multi-version ; lightweight claims: https://laragon.org/docs
- Herd databases (Windows): https://herd.laravel.com/docs/windows/getting-started/databases
- Laravel Sail: https://laravel.com/docs/12.x/sail

**Direct answer to the question asked:** **no lightweight Windows dev environment ships a code-level plugin API for database engines.** Every one of them curates a fixed list: DDEV as image tags, Laragon as a `bin\` folder convention, Herd as Pro services, Sail as container services. DDEV's "add-ons" add *types* by shipping another image, not by extending an in-process interface. The nearest thing to a real extension model is DDEV's add-on mechanism (`ddev add-on get ...`), and that is Docker Compose composition, not an API contract. So Shape B (open plugin API) would make DevKu an outlier with no precedent among its comparators, for the least-used feature.

**Contrast — how a mature project does expose a real plugin API:** Caddy's module system is the reference design. A plugin is "any named type that registers itself as a Caddy module when its package is imported", implements `caddy.Module` with an ID in a namespace (`http.handlers.visitor_ip`), and optional `Provisioner`/`Validator`/`CleanerUpper` interfaces. Crucially the plugin is **compiled in** (`xcaddy build` re-links Caddy with your module), and registration happens via `caddy.RegisterModule` + a blank import. Sources: https://caddyserver.com/docs/extending-caddy . That model is not transferable to a Tauri desktop app as-is: Caddy's plugins are Go source compiled into the binary, and the design explicitly leans on `init()` + `xcaddy` to do so. A Rust equivalent would be a `dylib` load boundary with a stable C ABI — a large, permanent, security-sensitive surface (see Shape B below).

---

## Shape A vs Shape B mapped onto DevKu's real code

### What breaks in `DatabaseBackend` as currently shaped

Against `src-tauri/src/database.rs` line 35:

| Method | Engine-neutral? | Action |
|---|---|---|
| `id`, `label`, `version` | yes | keep |
| `bin_dir`, `data_dir` | yes | keep |
| `is_installed`, `is_initialized` | yes | keep |
| `initialize(port)` | yes (SQLite: no-op) | keep; SQLite returns `Ok(())` |
| `spec(port) -> ServiceSpec` | **no for SQLite** | must become `Option<ServiceSpec>` or a separate "has a server" capability |
| `recovery_spec(port)` | **MariaDB/MySQL only** (`--skip-grant-tables`) | move into `Mariadb` |
| `password`, `store_password` | **MariaDB/MySQL only** (root account) | move into `Mariadb` |
| `root_hosts` | **MariaDB/MySQL only** (`mysql.user`) | move into `Mariadb` |
| `set_password` | **MariaDB/MySQL only** (`ALTER USER 'root'@host`) | move into `Mariadb` |
| `query(port, password, sql)` | partly — the `password` argument is MySQL-shaped | re-shape to a `Conn` (host/port/user/secret/db) or `query(&self, conn: &Conn, sql)` |
| `user_databases(port, password)` | partly — `information_schema.schemata` exists in PostgreSQL too but system-schema names differ | keep as a semantic method but let each engine own its SQL and its system-schema list |
| `dump(port, password, dest: &Path)` | **no** — assumes one plain-text file | re-shape to return a produced artifact (file or directory) and take a format |
| `wipe` | yes | keep |

### Minimum set a multi-engine trait must expose

Install / presence:
- `is_installed(&self) -> bool`
- `is_initialized(&self) -> bool`
- `initialize(&self, port: u16) -> Result<()>` (SQLite: no-op)
- `wipe(&self) -> Result<()>`

Spec / control:
- `spec(&self, port: u16) -> Result<Option<ServiceSpec>>` and/or a `fn has_server(&self) -> bool` so `Runtime` can skip supervision entirely for SQLite.

Query:
- `query(&self, conn: &Conn, sql: &str) -> Result<String>` where `Conn` carries `{port, user, secret, database}` — the shape that both `mariadb --execute` and `psql -c` and `sqlite3 <file> "<sql>"` can be built from.

Catalog / management:
- `user_databases(&self, conn: &Conn) -> Result<Vec<String>>`
- `system_schemas(&self) -> &'static [&'static str]`

Dump / restore:
- `dump(&self, conn: &Conn, dest: &Path, format) -> Result<DumpArtifact>` (SQLite: generate `.dump` output via the shell, or copy the file)
- `restore(&self, conn: &Conn, src: &Path) -> Result<()>`

The MariaDB-only methods (`recovery_spec`, `password`, `store_password`, `root_hosts`, `set_password`) **leave the trait** and become inherent methods on `Mariadb`, called by the MariaDB-specific commands only. That is the single most important refactor: it stops forcing PostgreSQL to fake a `user@host` model it does not have.

### How `DatabaseSettings`, `Paths`, `Runtime` change

`config.rs` `DatabaseSettings` — add an engine discriminant (see the migration section):

```rust
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseEngine {
    #[default]
    Mariadb,
    Postgres,
    Sqlite,
}
```

`paths.rs` — replace the six hardcoded `mariadb_*` methods with parameterised ones mirroring the PHP pattern (`php_dir(version)`):

```
fn database_dir(&self, engine) -> PathBuf          // engines/database/<engine>
fn database_bin(&self, engine) -> PathBuf
fn database_data(&self, engine) -> PathBuf         // databases/<engine>
fn database_runtime(&self, engine) -> PathBuf      // runtime/database/<engine>
fn database_config(&self, engine, file) -> PathBuf // my.ini | postgresql.conf | pg_hba.conf
fn database_secret(&self, engine) -> PathBuf
```

This mirrors `php_dir(version)` exactly, and it is what makes the "engines/database/<engine>/<version>" layout possible later.

`runtime.rs` — `database: Option<Supervisor>` becomes `databases: BTreeMap<DatabaseEngine, Supervisor>` (the same shape as `php: Vec<(String, Supervisor)>`). `Runtime::start` currently constructs `Mariadb::new(paths.clone())` at line 62; it becomes a loop over the configured engines calling a single factory:

```rust
fn backend_for(paths: &Paths, engine: DatabaseEngine) -> Box<dyn DatabaseBackend>
```

which is the exact analogue of how `CaddyBackend` / `ApacheBackend` are chosen. `DatabaseView` (session.rs line 94) gains an `engine` field already present; the panel becomes a list keyed by engine, like the PHP version list in `SettingsView.vue`.

### Shape A — curated multi-engine catalog

**What it is:** DevKu writes, tests, and SHA-256-verifies a fixed list of engine implementations, exactly as it already does for PHP versions (`PhpManager` + `build_catalog`). The trait stays internal (`pub(crate)` or a private module), no manifest format, no versioning contract, no discovery.

**Fit with DevKu:** high. It reuses three existing patterns verbatim:
1. `WebServerBackend` with `CaddyBackend` + `ApacheBackend` — a trait with two implementations, chosen by an enum (`Backend` in `config.rs`).
2. `PhpManager` catalog + folder scan — a fixed list, merged with what is on disk.
3. `DatabaseBackend` + `Mariadb` — already the right skeleton, just over-fitted.

**Cost:** one trait refactor plus N implementations. No new trust surface. The installer stays small because engines are downloaded on demand (MariaDB already is).

### Shape B — open plugin API

**What it is:** users or third parties add engines without changing DevKu. Requires: a stable public ABI (Rust needs either a `dylib` + stable C ABI, or a WASM boundary, or out-of-process plugins), a manifest/descriptor format, API versioning, discovery, and trust/sandboxing.

**What breaks in DevKu:** everything above, plus:
- **Tauri is not a plugin host for core logic.** Tauri plugins extend the *webview/IPC* surface; they do not give you a stable in-process ABI for arbitrary Rust types. The DevKu core is a single statically linked binary (`crate-type = ["rlib"]`, `panic = "abort"`, `lto = true`, `strip = true` in `Cargo.toml`). `panic = "abort"` and full LTO are actively hostile to `dylib` plugin loading.
- **A Rust `dyn` trait cannot cross a `dylib` boundary safely** across compiler versions; you would need a `#[repr(C)]` vtable + an ABI version check, which is a genuine engineering project. The Caddy model (compile the plugin in) does not apply because DevKu users do not compile DevKu.
- **Trust:** an engine plugin ships a server binary and a download URL. DevKu's whole safety story today is "pinned version + SHA-256 from archive.mariadb.org" (`database.rs` lines 8–31). A plugin API hands that trust decision to a manifest DevKu cannot verify.
- **Cost:** months of design, an ABI, a manifest spec, and a sandbox story, for a feature that **no comparator** ships.

**Verdict:** Shape B is the wrong trade for this stage. Shape A is the correct one and it leaves Shape B reachable later: an internal trait whose methods are already engine-neutral is precisely the prerequisite for a future plugin API, if one is ever justified.

---

## Config migration

Today `config.json` holds `database: {port, autostart}` (`config.rs` line 101). The repo requires config stays human-readable and diffable (`PLAN.md` principle 4; `AGENTS.md`).

### The recommended shape: internally tagged enum, with a default

Use an **internally tagged** enum with `#[serde(default)]` at the container level and on each field, so old configs and new configs both round-trip:

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DatabaseSettings {
    #[serde(default)]
    pub engine: DatabaseEngine,          // "mariadb" | "postgres" | "sqlite"
    #[serde(default = "default_db_port")]
    pub port: u16,
    #[serde(default)]
    pub autostart: bool,
}
```

Why this and not an untagged enum:

- Serde's **internally tagged** representation puts the tag *inside* the content: `{"type": "Request", ...}`. It "works for struct variants, newtype variants containing structs or maps, and unit variants but does not work for enums containing tuple variants." Source: https://serde.rs/enum-representations.html
- Serde's **untagged** representation "will try to match the data against each variant in order and the first one that deserializes successfully is the one returned", and "when no variant matches, `untagged` does not produce an informative error". Source: https://serde.rs/enum-representations.html — that is a bad fit for a config file users edit by hand and expect clear errors from.
- `#[serde(default)]` on a container means "any missing fields should be filled in from the struct's implementation of `Default`". Source: https://serde.rs/container-attrs.html — this is exactly what keeps `{"port":3306,"autostart":true}` (no `engine`) loading as MariaDB, preserving existing installs.

### Known pitfalls, cited

1. **`deny_unknown_fields` conflicts with `flatten`** and is "not supported in combination with `flatten`, neither on the outer struct nor on the flattened field". Source: https://serde.rs/container-attrs.html . DevKu does not use `flatten` today; do not introduce `deny_unknown_fields` on `DatabaseSettings` if a future per-engine settings block is flattened in.
2. **Untagged enums give no error diagnostics** and the implementation "can be costly" — prefer a tagged enum or the `serde-untagged` crate if diagnostics matter. Source: https://serde.rs/container-attrs.html
3. **Round-trip stability / diffability:** `serde_json::to_string_pretty` (`config.rs` `save()`) will now always write `"engine": "mariadb"` plus `port` and `autostart`. That is *good* for diffability (the file states its engine explicitly) but it does change every existing user's file on first save. Note this in `CHANGELOG.md` per `AGENTS.md`.
4. **Enum case:** keep `#[serde(rename_all = "lowercase")]` on `DatabaseEngine`, matching the existing `Backend` enum (`config.rs` line 10), so `"mariadb"` / `"postgres"` / `"sqlite"` are the on-disk spellings.
5. **DevKu already does forward-compatible defaults well.** `config.rs` tests `old_config_without_autostart_defaults_to_on` and `old_config_without_onboarded_starts_unonboarded` prove the existing pattern. Add the equivalent test: `old_config_without_engine_is_mariadb`.

### Why not a version bump

`Config.version` exists (currently 1) but is not used to branch. Since the new field is purely additive and defaults to the current behaviour (`mariadb`, port 3306), **no migration code is needed** — only the default. Adding a migration step would be speculative machinery for a change that serde already handles. If a *destructive* change were ever needed (e.g. renaming a key), that is when `version` earns its keep.

---

## Runtime and process model

### Port allocation

`ports::allocate` (`ports.rs`) assigns one port per PHP version starting at `PHP_PORT_START = 9001` (`runtime.rs` line 16), deterministically and uniquely, using `first_free` (loopback bind test). It is **not** the right mechanism for database engines, and here is why:

- PHP ports are *internal* (Caddy forwards to them; the user never sees them). Database ports are *user-visible contract*: Laravel `.env` says `DB_PORT=3306` or `5432`. The doc comment on `default_db_port()` says exactly this ("3306 ... is what Laravel `.env` files use").
- Therefore database ports should be **fixed per-engine defaults** (`3306` MariaDB/MySQL, `5432` PostgreSQL), stored in config, and only overridden by the user. `ports::allocate` should stay PHP-only.

The interaction to get right: if two engines are installed and both want a MySQL-family default (MariaDB and MySQL), they cannot share 3306. Two options:
1. Only one MySQL-family engine enabled at a time (a constraint, not a rule in code) — simplest.
2. Give MySQL a non-conflicting default (e.g. 3307) — but that breaks `.env` expectations.

Recommendation: **one port per engine, fixed default, user-overridable, and detect a collision at start** using the existing `ports::is_free` for a clear error instead of letting `mysqld` fail opaquely. Reuse `ports::is_free`; do not reuse `ports::allocate`.

### Supervision of N optional services

The principle "idle processes do not run" (PLAN.md) plus the existing model (`Runtime` already keeps `caddy: Option<Supervisor>` and `apache: Option<Supervisor>` and only starts Apache when a site needs it) gives a clean rule:

- **A database engine's `Supervisor` exists only when that engine is installed, initialized, and `autostart` is true.** `Runtime::start` currently gates on `database.is_installed() && database.is_initialized()` (line 63) and then `config.database.autostart` (line 65). Do the same, per engine, in a loop.
- **SQLite has no supervisor.** It must be a first-class "engineless" case: no process, no port, no `ServiceSpec`. The `Runtime` change is `has_server()` (or `Option<ServiceSpec>`) gating whether a `Supervisor` is created at all.
- **Engine failure never blocks sites.** This invariant already exists: `runtime.database_error = Some(...)` and other services keep running (lines 70–73). Preserve it per engine — one failed engine must not stop Caddy or PHP.
- **`ServiceSpec` needs no change** for MariaDB/PostgreSQL. `process-wrap` + Job Objects (`proc.rs`) already guarantee no orphaned processes, which matters more with N servers.

Reference for the supervision model: the existing `Supervisor`/`ServiceSpec`/`ServiceState` machinery in `proc.rs` (restart window `MAX_RESTARTS = 5` in 60 s, `READY_TIMEOUT`, `wait_ready` port probe). No new process-management dependency is needed.

---

## Recommendation

### Direction: Shape A, curated catalog, internal trait

Build the engine-neutral trait, ship SQLite and PostgreSQL (or PostgreSQL alone) as curated implementations, and keep the trait private. Do **not** build a plugin API.

### What to build first

1. **Refactor the trait** (`database.rs`): remove `recovery_spec`, `password`, `store_password`, `root_hosts`, `set_password` from `DatabaseBackend` into inherent methods on `Mariadb`. Change `spec` to `Option<ServiceSpec>` (or add `has_server`). Introduce a `Conn { port, user, secret, database }` and re-shape `query`/`user_databases`/`dump` around it. Add `system_schemas()` and `restore()`.
2. **Add `DatabaseEngine` to config** with `#[serde(default)]`, plus the round-trip test. No migration code.
3. **Parameterise `Paths`** with `database_*` methods keyed by engine (mirror `php_dir`).
4. **Change `Runtime.database` to a map** and add a `backend_for(paths, engine)` factory (mirror the Caddy/Apache selection).
5. **Ship SQLite first if the goal is "no server"**: it needs no `init`, no port, no `ServiceSpec`, no auth, no `mariadb-dump` analogue. It is the cheapest possible proof that the refactor is real. It is also the engine Herd now defaults new Laravel apps to (https://herd.laravel.com/docs/windows/getting-started/databases).
6. **Ship PostgreSQL next**, because it is the engine with the genuinely different auth/init/dump model and therefore the real test of the trait.

### Exact trait shape to adopt

```rust
pub trait DatabaseBackend: Send + Sync {
    fn engine(&self) -> DatabaseEngine;
    fn label(&self) -> &'static str;
    fn version(&self) -> &'static str;

    fn bin_dir(&self) -> PathBuf;
    fn data_dir(&self) -> PathBuf;
    fn is_installed(&self) -> bool;
    fn is_initialized(&self) -> bool;
    fn initialize(&self, port: u16) -> Result<()>;   // SQLite: Ok(())

    fn has_server(&self) -> bool;                     // false for SQLite
    fn spec(&self, port: u16) -> Result<Option<ServiceSpec>>;

    fn query(&self, conn: &Conn, sql: &str) -> Result<String>;
    fn user_databases(&self, conn: &Conn) -> Result<Vec<String>>;
    fn system_schemas(&self) -> &'static [&'static str];

    fn dump(&self, conn: &Conn, dest: &Path) -> Result<DumpArtifact>;
    fn restore(&self, conn: &Conn, src: &Path) -> Result<()>;
    fn wipe(&self) -> Result<()>;
}
```

MariaDB-only recovery/password/root-host methods live on `Mariadb` and are called from the MariaDB-specific commands, exactly as `CaddyBackend::validate` and `ApacheBackend`-specific helpers live off the shared trait today.

### What to explicitly NOT build

- **No public plugin API, no manifest format, no ABI versioning.** No comparator ships one; it needs a C-ABI boundary that `panic = "abort"` + full LTO make painful; and it hands a URL+hash trust decision to unverified manifests, breaking DevKu's pinned-artifact invariant.
- **No `sqlx`/`mysql_async`/native client crate.** The existing approach (shell out to the engine's own CLI) already works, adds zero dependencies, and matches the earlier file's conclusion in `docs/research/2026-11-12-database-management.md`.
- **No Adminer-per-engine.** Adminer's multi-driver build already covers PostgreSQL and SQLite; shipping one all-driver file is cheaper than special-casing.
- **No `pg_dump -Fd` / parallel dump / snapshot machinery.** Plain/one-file artifacts keep the backup contract identical across engines.
- **No port auto-allocation for databases.** Ports are user-visible contract; use fixed defaults + collision detection.
- **No install-wizard complexity for MySQL** until MariaDB + PostgreSQL are done; MySQL adds a third near-duplicate driver for little user benefit.

### Invariants it must respect

- **Idle processes do not run** — a supervisor exists only for an installed + initialized + opted-in engine; SQLite never spawns one.
- **One failed engine never blocks sites** — preserve the `database_error` pattern per engine.
- **Pinned, checksummed artifacts** — every engine download pinned to a version with SHA-256, exactly as MariaDB is today.
- **Config as text** — the engine is an explicit `"engine": "..."` string in `config.json`, diffable, with serde defaults so old files keep working.
- **Loopback only** — every engine binds `127.0.0.1` (and `::1` when usable), matching `Mariadb::bind_addresses`.
- **Installer ≤ 15 MB without PHP** — engines stay download-on-demand; nothing new is bundled.

### New dependencies per engine

DevKu's current crate set (`Cargo.toml`) is serde/serde_json/anyhow/async-trait/tokio/process-wrap/portable-pty/zip/reqwest/sha2/winreg/windows, all MIT/Apache-2.0 (or MIT OR Apache-2.0). None of the engine implementations below need a new crate, because the CLI-shelling pattern (`.rs`) covers all three:

| Engine | New crate | External binary downloaded | License of the binary | Approx. size on Windows |
|---|---|---|---|---|
| SQLite | none | `sqlite3.exe` (`sqlite-tools-win-x64-*.zip`) or `sqlite3.dll` | Public domain (SQLite) | tools zip 6.66 MiB; DLL zip 1.38 MiB (https://www.sqlite.org/download.html) |
| PostgreSQL | none | EDB windows-x64 binaries zip | PostgreSQL License (permissive, BSD-like) | **size not verified from a primary source in this pass** |
| MySQL | none | MySQL Community ZIP | GPL-2.0 (server) | **size and URL not verified from a primary source in this pass** |

If a native SQLite binding were ever wanted, `rusqlite` (MIT) exists, but it is not needed: the CLI-shelling pattern is already established and keeps the dependency graph flat.

### Honest cost ranking

1. **SQLite — trivial.** No server, no port, no init, no auth, no `ServiceSpec`, no separate dump binary (`.dump` is a shell built-in). The only real work is making the trait tolerate "no server". Cheapest by a wide margin.
2. **MySQL — cheap.** Shares the `Mariadb` driver almost entirely: same `mysql.user` model, same `--defaults-file`, same client flags. Real differences: install layout (no `server.dll` split), the initializer name (`mysqld --initialize`), the system-schema list, and MySQL 8.0's auth-plugin default. Mostly a second struct next to `Mariadb`.
3. **PostgreSQL — expensive.** Own auth model (`pg_hba.conf` + roles, no `user@host`), own init (`initdb`, must not run as root), own control (`pg_ctl`/`postmaster.pid`), own config pair (`postgresql.conf` + `pg_hba.conf`), own client flags (`psql -t -A -c`), own dump formats (`-Fc` custom / `-Fd` directory) and its own restore tool (`pg_restore`). This is the implementation that proves whether the refactor was correct. Do not start here, but do not skip it.

---

## Open questions

1. **MySQL archive endpoint.** No first-party, checksummed, machine-readable URL for the MySQL Community ZIP was reached. Verify before shipping a MySQL implementation; MariaDB's `archive.mariadb.org` + `sha256sums.txt` pattern is the model to require.
2. **PostgreSQL binary size.** The EDB binaries page does not state the zip size. Measure it before claiming anything about installer impact (though engines are downloaded, not bundled).
3. **PostgreSQL `initdb -A` choice.** `trust` is "the default for ease of installation" but "Do not use `trust` unless you trust all local users". For a loopback-only dev environment `trust` is defensible; for parity with MariaDB's password model, `scram-sha-256` with a generated secret is closer. Decide explicitly, and note that `password()`/`set_password()` semantics do not transfer.
4. **SQLite's place in the UI.** SQLite has no service, so the "Database" tab's install/start/stop/autostart controls do not apply. Decide whether SQLite is a *backend for sites* (per-site `.sqlite` file, picked in the site wizard) rather than a global service — this is likely the correct shape and differs from every other engine.
5. **Do sites declare an engine, or is there one global engine?** `Site` has `php` and `backend` fields today. Whether it should also gain a `database` field (per-site engine) or stay global is a product decision, not a technical one. Per-site is closer to DDEV's model; global is closer to DevKu's current one-engine reality.
6. **Adminer build.** Confirm the all-driver `adminer-<ver>.php` covers the exact PostgreSQL and SQLite versions DevKu ships, and pin it with SHA-256 as the earlier file recommends.
7. **MySQL 8.0 migration caveat.** DDEV documents that MySQL 8.0 dumps "often have keywords or other features not supported elsewhere" (https://docs.ddev.com/en/stable/users/extend/database-types/), so a MariaDB↔MySQL dump interchange should not be assumed. Any future "switch engine" flow needs an explicit dump/restore contract.

---

## Sources

Engine facts:
- MariaDB `mariadb-install-db` (Windows `.exe`, parameters, `my.ini` generation): https://mariadb.com/docs/server/server-management/install-and-upgrade-mariadb/installing-mariadb/installing-system-tables-mariadb-install-db/mariadb-install-db-exe
- MariaDB `mariadb-install-db` (Unix; states the Windows tool does not share parameters): https://mariadb.com/docs/server/clients-and-utilities/deployment-tools/mariadb-install-db
- PostgreSQL `initdb`: https://www.postgresql.org/docs/current/app-initdb.html
- PostgreSQL `pg_ctl`: https://www.postgresql.org/docs/current/app-pg-ctl.html
- PostgreSQL `pg_dump` (formats `-Fp`/`-Fc`/`-Fd`/`-Ft`, one database per dump, `pg_dumpall` for a cluster): https://www.postgresql.org/docs/current/app-pgdump.html
- PostgreSQL `pg_hba.conf` (auth records, reload semantics, Windows note): https://www.postgresql.org/docs/current/auth-pg-hba-conf.html
- PostgreSQL `psql` (`-t`, `-A`, `-c`, `-f`, default port 5432): https://www.postgresql.org/docs/current/app-psql.html
- PostgreSQL Windows downloads (EDB installer; zip alternative): https://www.postgresql.org/download/windows/
- EDB PostgreSQL binaries (zip archives of installed files): https://www.enterprisedb.com/download-postgresql-binaries
- SQLite CLI (`.dump`, `.recover`, `.open`, `.mode`, dot-command list): https://www.sqlite.org/cli.html
- SQLite `sqlite3_open_v2` (creation on open, URI params `mode`/`cache`/`nolock`/`immutable`): https://www.sqlite.org/c3ref/open.html
- SQLite download page (Windows artifacts and sizes; 3.54.1): https://www.sqlite.org/download.html

Comparators:
- DDEV config options (`database`, `dbimage`): https://docs.ddev.com/en/stable/users/configuration/config/
- DDEV database types (`mariadb`/`mysql`/`postgres` values, add-ons, MySQL 8.0 divergence): https://docs.ddev.com/en/stable/users/extend/database-types/
- Laragon multi-version services: https://laragon.org/docs/multi-version
- Laragon getting started / lightweight claims: https://laragon.org/docs
- Herd databases (Windows): https://herd.laravel.com/docs/windows/getting-started/databases
- Laravel Sail: https://laravel.com/docs/12.x/sail

Plugin-API design reference:
- Caddy "Extending Caddy" (modules, namespaces, `caddy.RegisterModule`, `Provisioner`/`Validator`/`CleanerUpper`, `xcaddy`, compile-in model): https://caddyserver.com/docs/extending-caddy

Serialization:
- Serde enum representations (externally/internally/adjacently/untagged; untagged has no informative error and can be costly): https://serde.rs/enum-representations.html
- Serde container attributes (`default`, `deny_unknown_fields` and its `flatten` conflict, `tag`, `untagged`): https://serde.rs/container-attrs.html

DevKu repository (read-only):
- `PLAN.md`, `src-tauri/src/database.rs`, `src-tauri/src/runtime.rs`, `src-tauri/src/session.rs`, `src-tauri/src/config.rs`, `src-tauri/src/paths.rs`, `src-tauri/src/php.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/caddy.rs`, `src-tauri/src/apache.rs`, `src-tauri/src/ports.rs`, `src-tauri/src/proc.rs`, `src-tauri/Cargo.toml`, `src/components/settings/SettingsView.vue`
- `docs/research/2026-11-12-database-management.md` (earlier database-management research; referenced, not duplicated)

Claims that could not be verified against a primary source in this pass:
- MySQL Community Server Windows ZIP archive URL, size, and checksum endpoint.
- MySQL 8.x first-run command (`mysqld --initialize` / `--initialize-insecure`) and layout specifics.
- PostgreSQL Windows binaries zip size.
- The MySQL Community Server license string as stated on a first-party page (widely reported as GPL-2.0, not confirmed here).
