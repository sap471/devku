# Database management for DevKu: comparators, Adminer vs phpMyAdmin, and a minimal scope

Date: 2026-11-12
Scope: How comparable local PHP tools manage databases, the current state of Adminer/phpMyAdmin, single-binary/CLI options (including the DBX client), backup/restore pitfalls on Windows, and the smallest complete direction for DevKu.

Verified against DevKu source (read-only):
- `src-tauri/src/database.rs`: MariaDB `11.4.13` (`VERSION`), pinned from `https://archive.mariadb.org`, SHA-256 checked. `my.ini` rendered by DevKu with `bind-address=127.0.0.1,::1` (or `127.0.0.1` when IPv6 is unusable), `skip-name-resolve`, `port=<port>`. `mariadb-dump` used only at removal, with `--default-character-set=utf8mb4 --single-transaction --routines --events --databases <dbs...> --result-file=<path>`.
- `src-tauri/src/commands.rs`: `install_database`, `start_database`, `stop_database`, `set_database_autostart`, `reset_database_password`, `remove_database`, `open_backups`. `mariadb`/`mariadb-dump` appended to terminal PATH.
- `src/components/settings/SettingsView.vue`: "Database (MariaDB)" tab = install / start / stop / autostart / password reset / remove-with-backup. No listing, no import, no on-demand backup.
- `src-tauri/src/proc.rs`, `ports.rs`: upstreams bind `127.0.0.1` only; Caddy is the sole `:80`/`:443` listener.
- `PLAN.md`: "Idle processes do not run"; "Lightweight is a feature"; "Configuration as text".

---

## Summary

Every comparator that ships a web DB GUI bundles it as a PHP app served by its own always-on web server (XAMPP, Local), or makes it an optional add-on (DDEV, Herd Pro), or ships a native desktop client alongside (Laragon with phpMyAdmin/HeidiSQL, Local with AdminNeo). None runs a database GUI when the user is not looking at it, except the bundled PHP GUI whose web server is already running for the sites. DevKu has a structural advantage: it already owns a Caddy instance and a working `php-cgi` pool, so a single-file PHP client can reuse both with no new runtime.

Recommendation in one line: ship **Adminer as an on-demand, loopback-only vhost behind DevKu's existing Caddy + php-cgi**, add `.sql` import/export via the existing `mariadb`/`mariadb-dump` binaries, add per-database list/create/drop through the existing `DatabaseBackend::query`, and do **not** build a native DB browser, a migration runner, or a schema diff. Nothing new ships as a binary; the only new artifact is one ~580 KB (or ~440 KB MySQL-only) PHP file.

---

## Comparators table

| Tool | DB GUI bundled? | Always running or on demand | Ships what | Primary source |
|---|---|---|---|---|
| **Laragon** | Bundled, but not by the app itself: the tray "Database" entry opens a bundled **phpMyAdmin** (older) or **HeidiSQL**; can be changed | GUI runs only when launched; MariaDB is a Laragon-managed service, startable from the tray | phpMyAdmin and/or HeidiSQL as portable apps in `bin`; MySQL/MariaDB service | https://laragon.org/docs/context-menu , https://github.com/leokhoa/laragon/issues/1492 |
| **XAMPP** | Yes, bundled **phpMyAdmin 5.2.1** | phpMyAdmin available whenever Apache runs; Apache/MySQL started from the XAMPP Control Panel (optionally as Windows services) | Apache + MariaDB + PHP + Perl + phpMyAdmin; Windows installer ~191 MB (XAMPP 8.5.12) | https://www.apachefriends.org/download.html , https://www.apachefriends.org/faq_windows.html |
| **Herd (Laravel)** | No GUI bundled. Community uses external tools (e.g. DBngin); **Herd Pro** adds a service manager UI and one-click `.env` wiring | DB service is user-started; Pro manages start/stop; no bundled web GUI | PHP, nginx, dnsmasq, Node; Pro adds MySQL/MariaDB/PostgreSQL/MongoDB/Redis services | https://herd.laravel.com/docs/windows/getting-started/databases.md , https://herd.laravel.com/docs/macos/getting-started/databases.md |
| **Local (WP Engine)** | Yes, **AdminNeo** (an Adminer fork) under the site's Database tab | AdminNeo opens per-site on demand from the app; the site's MySQL is per-site | WordPress-oriented stack; AdminNeo bundled; documented route to external tools | https://localwp.com/help-docs/advanced/connect-a-tool-like-mysql-workbench-to-my-local-sites-database/ , https://www.adminneo.org/ |
| **DDEV** | **Not bundled.** phpMyAdmin and Adminer are add-ons (`ddev add-on get ...`); GUI is otherwise external (TablePlus, SequelAce, DBeaver, PhpStorm, DevDb) | Containers start/stop per project (`ddev start`/`stop`); nothing runs when stopped | CLI clients `mysql`/`mariadb`/`psql` **inside** the `db`/`web` containers; `ddev import-db`/`export-db`; snapshot/restore | https://docs.ddev.com/en/stable/users/usage/database-management/ |
| **TablePlus / HeidiSQL** | They **are** the GUI (standalone clients, not dev-environment bundles) | User-launched desktop app; nothing idles until opened | HeidiSQL: native Delphi app, free/open source, Windows/Linux/macOS | https://www.heidisql.com/ |
| **DBX** (`t8y2/dbx`) | It **is** the GUI (standalone client, not a dev-environment bundle); Rust + Tauri + Vue, same stack as DevKu | User-launched desktop app, or Docker/CLI; nothing idles until opened | 100+ database engines in ~25 MB, Apache-2.0, also ships AI assistant and MCP server | https://github.com/t8y2/dbx |

Notable, verified from the sources above:
- DDEV removed phpMyAdmin from core; it is now an add-on. So the modern trend is "GUI is optional, CLI is primary." (source: DDEV database-management page)
- Herd explicitly tells users to bring an external tool or upgrade to Pro; it does not ship a browser. (source: Herd databases page)
- XAMPP still ships phpMyAdmin in the box, and the whole bundle is ~191 MB. This is the anti-pattern DevKu's PLAN.md pushes against.
- Laragon's own core is "around 6 MB, less than 10 MB RAM" and it separates the GUI (phpMyAdmin/HeidiSQL, opened on demand) from the service. (source: https://laragon.org/docs/lightweight)

**Could not verify to a primary source:** the exact current Laragon version and whether current Laragon still defaults to a specific phpMyAdmin build vs HeidiSQL. laragon.org docs do not name the bundled GUI in the pages I could reach; the "Database" button behaviour is only evidenced by a user issue (https://github.com/leokhoa/laragon/issues/1492). Treat "Laragon ships phpMyAdmin/HeidiSQL" as corroborated but not from a first-party doc page.

---

## Adminer vs phpMyAdmin

### Adminer

- Latest release: **6.3.0, 2026-11-03** (tag `v6.3.0`, published `2026-11-03T14:41:51Z`).
- Artifact sizes (GitHub release assets):
  - `adminer-6.3.0.php` (all drivers, all languages): **584,036 bytes** (~570 kB).
  - `adminer-6.3.0-mysql.php` (MySQL/MariaDB only): **441,747 bytes** (~431 kB).
  - `adminer-6.3.0-en.php` (English only): **433,004 bytes**.
  - `adminer-6.3.0-mysql-en.php`: **295,404 bytes** (~288 kB). Smallest useful option for DevKu (single DB family, English).
- Requirements: **PHP 5.4+, 7, 8 with sessions enabled.** No extensions are listed as mandatory beyond sessions; MySQL/MariaDB connect via the standard MySQL driver. (source: https://www.adminer.org/en/, "Requirements")
- License: **Apache License 2.0 or GPL 2** (dual). (source: https://www.adminer.org/en/)
- Deploy: one PHP file, drop next to a web root; `adminer.css` / `adminer-dark.css` sit alongside for theming. (source: https://www.adminer.org/en/)
- Maintained and active: repo pushed 2026-11-04. Security is stated as its #1 development priority; it refuses passwordless connections by default and rate-limits login attempts. (source: https://www.adminer.org/en/ , https://github.com/vrana/adminer)
- **Adminer-side "desktop" distribution exists**: `vrana/adminer-desktop` ships `adminer.exe` (~7 MB) for 64-bit Windows that bundles its own PHP (`static-php-cli`), needs no web server, listens only on `127.0.0.1`, and defends against DNS rebinding by answering only `127.0.0.1`/`localhost`. It even accepts an empty password locally via the `login-ip` plugin. License: Apache-2.0 or GPL-2; bundled PHP keeps its own licenses. (source: https://github.com/vrana/adminer-desktop) This is an alternative to running Adminer under DevKu's Caddy, discussed in the recommendation.

Note on the version jump: the last 5.x line seen in the release feed is **5.5.0**; **6.0.0 landed 2026-08-07** with security fixes (two XSS escapes, a CSRF fix in the SQL operator, and a login restriction refusing servers that accept any password). So the 6.x line is current and the one to track. (source: https://github.com/vrana/adminer/releases/tag/v6.0.0)

### phpMyAdmin

- Latest stable: **5.2.3, released 2025-10-08** (bugfix release). (source: https://www.phpmyadmin.net/news/2025/10/8/phpmyadmin-523-is-released/)
- Requirements: **5.2.x is compatible with PHP 7.2 and newer and MySQL/MariaDB 5.5 and newer.** Version in development, **6.0, is expected to require PHP 8.2.0 or newer.** 4.9.11 is "security fixes only." (source: https://www.phpmyadmin.net/downloads/)
- Support timeline: "New releases and support for phpMyAdmin 5 is expected to end in the near future," with 6.0 expected to need PHP 8.2+. (source: https://www.phpmyadmin.net/news/2025/10/8/phpmyadmin-523-is-released/)
- Size: phpMyAdmin is a multi-thousand-file PHP application, not a single file (the project's own frame is "many files"; Adminer's own site contrasts "a single file instead of thousands"). I did **not** find a first-party byte-size figure on the primary pages reached; do not quote a specific MB number as first-party. (claim flagged as unverified to a primary size source)
- License: GPL-2.0-or-later (phpMyAdmin project). **Not confirmed against a first-party license page in this pass**; treat as widely reported, verify before relying on it.

### Can either run on DevKu's Caddy + php-cgi, loopback-only?

Yes for both, in principle, because DevKu already serves PHP through `php-cgi` behind Caddy and all upstreams bind loopback. The practical differences for DevKu:

- Adminer is **one file**: no `config.inc.php`, no session-storage directory tree, no `tmp/` scaffolding. It needs sessions enabled in the PHP build (DevKu controls `php.ini`). Adding a loopback-only vhost that maps a path such as `/adminer` to that one file is the whole integration.
- phpMyAdmin needs a directory of files, a writable `tmp/` for templates and sessions, and its own config; that is more moving parts and more to keep updated. For a tool whose target is ~5 MB executable and near-zero idle footprint, that is disproportionate.
- Security posture for a local GUI: Adminer and adminer-desktop both refuse passwordless logins by default (adminer-desktop relaxes only for `127.0.0.1` via `login-ip`). A GUI reachable on `:80` must be loopback-restricted and ideally bound to a hostname only DevKu's own Caddy serves, never exposed on `0.0.0.0`. DevKu's invariant is already loopback-only, which fits.

---

## Native / CLI options (non-PHP-WebApp)

Existing, zero-new-binary options:

1. **`mariadb` / `mariadb-dump` in the built-in terminal.** Already on PATH (verified in `commands.rs`). Covers power users and scripted import/export today. Zero new dependency. For non-terminal users it is not a "browser", but it is the honest default.
2. **`mariadb` client caveat:** the client defaults to **latin1** unless `--default-character-set` is given, which corrupts non-ASCII output/input. DevKu already passes `--default-character-set=utf8mb4` on dump; any future import must pass it too. (source: https://mariadb.com/docs/server/clients-and-utilities/mariadb-client/mariadb-command-line-client)
3. **`adminer.exe` (adminer-desktop).** ~7 MB, bundles static PHP, listens only on `127.0.0.1`, no web server needed. This is the "launch an external/local client" path if DevKu does not want to serve PHP from its own Caddy. Cost: a new 7 MB artifact and its own update cadence. (source: https://github.com/vrana/adminer-desktop)
4. **Launch a client the user already has** (TablePlus/HeidiSQL/DBeaver). Principle 3 in PLAN.md ("do not duplicate binaries") and DDEV's approach ("use the GUI you already own, here are the connection details"). Zero shipped bytes. Needs only connection-detail handoff.
5. **Embed a Rust MySQL client crate** for a native in-app browser. Credible, maintained, pure-Rust crates:

| Crate | Latest | License | Pure Rust | Activity | Notes |
|---|---|---|---|---|---|
| `mysql_async` | 0.37.1 (2026-09-01) | MIT OR Apache-2.0 | Yes (rustls; no libmysqlclient) | repo pushed 2026-11-08 | async, tokio-based; fits DevKu's tokio stack |
| `mysql` (rust-mysql-simple) | 28.0.4 (2026-10-16) | MIT/Apache-2.0 | Yes | repo pushed 2026-10-16 | sync API; simpler call sites |
| `sqlx` (mysql feature) | 0.9.0 (2026-05-21) | MIT OR Apache-2.0 | Yes | repo pushed 2026-11-10 | full toolkit; heaviest of the three |
| `mysql_common` | 0.38.2 (2026-07-24) | MIT OR Apache-2.0 | Yes | shared protocol/codec layer | dependency, not a standalone client |

License costs are all permissive (MIT/Apache-2.0). The real cost is **build and surface area**: a native browser means writing query execution, result rendering, pagination, editing, import/export UI, plus the SQL-safety work Adminer already solved. That is a large feature that violates "idle processes do not run" only mildly (it is in-app, no new process) but violates "lightweight is a feature" in code volume and maintenance.

---

## DBX (`t8y2/dbx`): a same-stack external client

Requested addition. DBX is worth a dedicated section because it is the closest published analogue to a "native DevKu database browser": same stack as DevKu (Rust + Tauri + Vue), permissive license, and a comparable size claim. It is also the strongest argument for **not** building one into DevKu.

### Verified facts

| Item | Value | Source |
|---|---|---|
| Repository | `https://github.com/t8y2/dbx` | GitHub repo page |
| License | Apache-2.0 | `LICENSE` via repo page |
| Stars / forks | 30.0k stars, 2.7k forks, 97 watching | repo page |
| Stack | Rust + Tauri + Vue (repo topics: `rust`, `tauri`, `vue`) | repo page topics |
| Size claim | "100+ databases in **25 MB**" | README headline |
| Databases claimed | MySQL, PostgreSQL, SQLite, Redis, MongoDB, DuckDB, SQL Server, Dameng; "100+" total | repo description |
| Distribution | Desktop, Docker, CLI, plus a built-in AI assistant and an MCP server | README headline / description |
| Documentation site | `https://docs.dbxio.com/` (referenced by README assets) | README asset host `dl.dbxio.com` |
| License/legal posture | Sponsor page credits **TrustAsia** for cloud code signing of DBX CI/CD builds | repo page sponsor table |

### Points that matter for DevKu

1. **It proves the category is real and the stack works.** A 25 MB Tauri + Rust + Vue database client with 30k stars shows a native DB browser is a legitimate product, not a toy. So the "build a native browser" option in the section above is not technically blocked; it is only a scope question.
2. **It is far past what a dev environment should own.** DBX covers 100+ database engines, an AI assistant, an MCP server, and Docker/CLI distribution. That breadth is exactly what PLAN.md's "lightweight is a feature" pushes against. DevKu serves one engine family (MySQL/MariaDB). Reimplementing even a slice of DBX is a permanent maintenance surface for a feature Adminer already covers in one ~288 KB file.
3. **Same license, same stack, so no integration is blocked by licensing.** Apache-2.0 is compatible with DevKu's MIT project and its vendoring rules. If DevKu ever wanted a native browser, DBX is a reference implementation to study for Tauri-specific pitfalls (window, tray, connection pooling), not code to fork wholesale into a dev-environment shell.
4. **It is an external client, so it fits the existing "bring your own tool" path.** Like TablePlus/HeidiSQL/DBeaver (option 4 above), DBX needs only **connection-detail handoff**: host `127.0.0.1`, DevKu's MariaDB port, user `root`, and the password DevKu already manages. Zero shipped bytes, zero new idle process. This is the same shape as DDEV's documented "use the GUI you already own" approach and Herd's external-tool guidance.
5. **Watch the runtime footprint against DevKu's targets.** DBX's own number is 25 MB for the whole app. DevKu's PLAN.md target is a **<= 15 MB installer without PHP**. Bundling DBX inside DevKu would blow that target on its own; pointing users at DBX as a separate download does not.

### Verification limits (do not overquote)

- The README's middle section (the per-engine feature list and any precise download sizes) was **elided by the fetch** and could not be read in full. The "25 MB" and "100+ databases" figures come from the README headline and repository description only.
- **The latest release was not pinned from the releases page**: the page rendered release notes but the asset list and version tags did not load (`There was an error while loading`). Do not quote a DBX version number without re-checking `https://github.com/t8y2/dbx/releases`.
- The exact Adminer-vs-DBX feature comparison was **not** performed; only the scope and footprint contrast above is grounded.
- `https://docs.dbxio.com/` returned a transport error and could not be read, so no feature claims are sourced from the docs site.

### Effect on the recommendation

Unchanged, and now with a named external alternative. The recommended order stays: **per-DB list/create/drop, then Adminer on-demand behind Caddy, then import/export, then `.env` handoff.** DBX strengthens the "do not build a native Rust browser" call by giving a concrete, same-stack, 25 MB counter-example of how large that feature becomes. If a user wants a full native browser, DevKu should **hand off connection details to DBX or another external client** rather than grow one.


**Weighing against PLAN.md principles:**
- "Idle processes do not run": a PHP GUI served by DevKu's existing Caddy/php-cgi is fine as long as it is pulled on demand; if it must be always-served by Caddy, that is still no *new* idle process because Caddy already runs. `adminer.exe` would be a new process, only while open.
- "Lightweight is a feature": a Rust DB browser is the heaviest option by code volume. Adminer is the lightest GUI. CLI + external client is lightest by shipped bytes.

---

## Backup / restore pitfalls (Windows, MariaDB 11.4)

All flags below are from the MariaDB docs; names and defaults quoted verbatim where it matters.

**Dump correctness (`mariadb-dump`):**
- `--single-transaction`: sends `START TRANSACTION` before dumping; consistent only for **InnoDB** (needs `REPEATABLE-READ` and `START TRANSACTION WITH CONSISTENT SNAPSHOT`). "The dump is not guaranteed to be consistent for other storage engines" (MyISAM, MEMORY). No other connection should run `ALTER TABLE`, `DROP TABLE`, `RENAME TABLE`, `TRUNCATE` during the dump. (source: https://mariadb.com/docs/server/clients-and-utilities/backup-restore-and-import-clients/mariadb-dump)
- `--triggers` is **on by default** (triggers are part of the table definition); disable with `--skip-triggers`. No need to pass it explicitly.
- `--routines` and `--events` are **not** included by default; stored procedures, functions, views and events need to be requested explicitly. DevKu already passes `--routines --events`. `--routines` requires the SELECT privilege. (same source)
- `--databases`: makes **all** name arguments be database names and emits `CREATE DATABASE` + `USE` before each. Without it, the first name is the DB and the rest are tables. DevKu uses `--databases <dbs...>` for a multi-DB dump: correct, and it makes the dump self-contained (creates DBs on restore). (same source)
- `--default-character-set=utf8mb4`: if not specified, `mariadb-dump` **defaults to utf8mb4** anyway. DevKu passing it explicitly is harmless and documents intent. (same source)
- `--opt` (and therefore `--quick`) is **on by default**; extended inserts on by default (`--extended-insert`). `--result-file` writes output without converting `\n` to `\r\n` on Windows and **overwrites the file even on error** — so a partial dump can clobber a good one; write to a temp path and rename. (same source)
- Generated columns: from MariaDB 13.1, dumps no longer write computed generated-column values (they are `DEFAULT`). Not a concern on 11.4 server, but note it if a dump ever moves between versions.
- `INFORMATION_SCHEMA`/`PERFORMANCE_SCHEMA` are **not** dumped by default (good, matches DevKu's `SYSTEM_SCHEMAS` exclusion).

**Restore / import pitfalls:**
- **Sandbox-mode preamble**: `mariadb-dump` writes a sandbox-enabling command at the top of the dump. "This command cannot be interpreted by earlier versions of the `mariadb` command-line client or by the MySQL command-line client `mysql` — an error is thrown if used against client versions that do not support it." Restoring with a *new enough* `mariadb` client is therefore required for dumps from this toolchain; a mismatch errors. (source: same `mariadb-dump` page)
- **Client default charset is latin1**: the `mariadb` client "uses the latin1 character set by default," which mis-formats multibyte output and can corrupt data on import. Always pass `--default-character-set=utf8mb4` on import too. (source: https://mariadb.com/docs/server/clients-and-utilities/mariadb-client/mariadb-command-line-client)
- **Sandbox disables `\.`(source), `\!`(system), `\T`(tee), and pager-with-argument** from MariaDB 11.4.2/11.2.4/11.1.5/11.0.6/10.11.8/10.6.18. On 11.4.13 this applies. Feeding a dump via stdin or `mariadb < dump.sql` is the robust path; relying on the `source` command inside a sandboxed session will error. (same client page)
- **Timezone**: `--tz-utc` is on by default and adds `SET TIME_ZONE='+00:00'` to the dump; `--skip-tz-utc` changes TIMESTAMP handling to the server's local zone. Do not toggle this between dump and restore.
- Restoring a `--databases` dump will `CREATE DATABASE`/`USE`; restoring into an existing non-empty DB can collide. Decide explicitly: fresh import into a new DB, or `DROP`/`CREATE` first.
- Windows specifics already handled by DevKu: write to a file path DevKu controls, use `--result-file`, and account for `remove_dir_all` retries while `mysqld` holds files (`database.rs::wipe` already retries 5×).

---

## Recommendation

**Smallest complete direction:** make Adminer the DevKu database browser, served on demand by DevKu's existing Caddy + `php-cgi`, and make import/export first-class using the binaries DevKu already ships.

### Build first (in order)

1. **Per-database list / create / drop** via the existing `DatabaseBackend::query` and `user_databases`. Pure SQL over the `mariadb` client that is already wired; no new binary. This is the smallest real feature and unblocks everything else.
2. **On-demand "Open database browser" button**: a loopback-only Caddy route (e.g. `db.localhost`) that serves a single Adminer PHP file from DevKu's runtime directory, started only when clicked (or served by the always-running Caddy but the page itself is inert until opened). Prefer the **`adminer-<ver>-mysql-en.php`** asset (~288–440 KB) to minimize size and languages. Pin the file and its SHA-256 exactly as DevKu already pins MariaDB; check the upstream version on demand, never auto-update silently.
3. **Import/export on demand**: `mariadb-dump` export and `mariadb ... < file.sql` import, both with `--default-character-set=utf8mb4`, using `--single-transaction --routines --events --databases` for export. Import must run through a client new enough to parse sandbox mode (the bundled 11.4 client is fine). Validate the file is a readable `.sql` and stream it via stdin, not `source`.
4. **Connection details handoff to a new site**: write the DB host/port/user/password into the site's `.env` when the user opts in, mirroring Herd's one-click `.env` wiring. Config-as-text is preserved.

### Explicitly do NOT build

- **A native Rust DB browser.** `mysql_async`/`mysql`/`sqlx` are all permissive and pure-Rust, but a browser is a large, permanent surface (query UI, pagination, editing, export) that Adminer already provides for ~300 KB and zero concurrency/permission code. It contradicts "lightweight is a feature."
- **A bundled phpMyAdmin.** Multi-file PHP app, needs writable `tmp/`, larger update burden; the project itself is mid-transition to 6.0 (PHP 8.2+). No reason to carry it when one Adminer file does the job.
- **Seeding/fixtures, schema diff, migration runner.** These are framework concerns (Laravel Artisan, framework-specific). DDEV does snapshots because it owns container volumes; DevKu's sites own their migrations. Out of scope for a lightweight host.
- **Always-on GUI web server or a second listener.** Must not add a process that idles.

### Invariants to respect

- **Loopback binding**: the Adminer vhost and MariaDB stay on `127.0.0.1`/`::1` only. Never `0.0.0.0`. Adminer's own guidance is to restrict by IP/password where reachable; DevKu's loopback-only default satisfies this.
- **Config as text**: any Adminer vhost file lives in DevKu's runtime dir as rendered text, like `my.ini`.
- **No idle processes**: the browser page is pulled on demand; Caddy already runs, so serving it adds no new idle process. Do not spawn a persistent PHP server beyond the existing php-cgi pool.
- **Pinned, checksummed artifacts**: Adminer file pinned + SHA-256, same pattern as MariaDB.

### New dependency cost

- Adminer PHP file: ~288–440 KB (MySQL/English build), Apache-2.0 or GPL-2, no new runtime (uses existing PHP). **Preferred.**
- Alternative `adminer.exe` (adminer-desktop): ~7 MB, bundles static PHP, Apache-2.0 or GPL-2 with bundled-PHP licenses; only worth it if DevKu deliberately does not want to serve PHP from Caddy. Adds an artifact + update cadence.
- No Rust crate is needed under this recommendation. If a native browser were ever built, `mysql_async` 0.37.1 (MIT OR Apache-2.0, pure Rust, active) fits DevKu's tokio stack with no C library.

---

## Open questions

1. **Does Caddy serve Adminer always, or only while the user has the browser open?** Caddy is always up (sites depend on it), so "always serve" = no new idle process, but the route is then always reachable on loopback. Confirm this is acceptable, or gate the route on a DevKu toggle.
2. **Single Adminer file vs Adminer Editor.** Adminer (full) vs Adminer Editor (simplified, for end users) is a UX choice; verify which fits DevKu's audience before pinning.
3. **Password handling.** Adminer refuses empty passwords; DevKu's MariaDB may be passwordless. Decide whether to surface the `login-ip` plugin (as adminer-desktop does) or pre-fill credentials and require a password.
4. **Import into existing DB.** Define the contract: new database only, or allow overwrite with confirmation. This affects whether DevKu issues `DROP DATABASE` before import.
5. **Laragon current GUI default** (phpMyAdmin vs HeidiSQL) could not be confirmed from a first-party doc page; only a user issue corroborates it. Re-check laragon.org if a precise citation is needed.
6. **phpMyAdmin license string and exact download size** were not confirmed against a first-party license/size page in this pass; verify at https://www.phpmyadmin.net/ if a precise figure is required.
7. **DBX version and full feature list.** The releases page did not render its asset list and version tags, and `docs.dbxio.com` was unreachable, so no DBX version number or per-engine feature claim is recorded here. Re-check `https://github.com/t8y2/dbx/releases` before quoting either.
8. **Should DevKu document an external-client handoff?** If DBX (or HeidiSQL/TablePlus) is the answer for a full native browser, DevKu needs a small "connection details" surface: host, port, user, password. That is research item 4 in the recommendation, and DBX makes it more valuable, not less.

---

## Sources

- Adminer — official site (version 6.3.0, 2026-11-03; sizes; requirements PHP 5.4+/7/8 with sessions; license Apache-2.0 or GPL-2): https://www.adminer.org/en/
- Adminer — release assets and dates: https://github.com/vrana/adminer/releases
- Adminer — 6.0.0 release notes (security fixes; 2026-08-07): https://github.com/vrana/adminer/releases/tag/v6.0.0
- Adminer — repository: https://github.com/vrana/adminer
- Adminer Desktop (adminer.exe, ~7 MB, bundled static PHP, 127.0.0.1-only, DNS-rebinding defence, `login-ip` plugin, license): https://github.com/vrana/adminer-desktop
- AdminNeo (Adminer fork used by Local; single PHP file; Apache-2.0/GPL-2): https://www.adminneo.org/
- phpMyAdmin — downloads and requirements (5.2.3, PHP 7.2+; 6.0 dev needs PHP 8.2+): https://www.phpmyadmin.net/downloads/
- phpMyAdmin — 5.2.3 release announcement (2025-10-08; support timeline): https://www.phpmyadmin.net/news/2025/10/8/phpmyadmin-523-is-released/
- XAMPP — download page (phpMyAdmin 5.2.1 in "What's Included"; Windows installer ~191 MB): https://www.apachefriends.org/download.html
- XAMPP — Windows FAQ (control panel starts Apache/MySQL; config file locations): https://www.apachefriends.org/faq_windows.html
- Laragon — context menu / system tray (service start-stop, tool access): https://laragon.org/docs/context-menu
- Laragon — lightweight (core ~6 MB, <10 MB RAM): https://laragon.org/docs/lightweight
- Laragon — FAQ: https://laragon.org/docs/faq
- Laragon — issue #1492 (changing the Database button from phpMyAdmin to HeidiSQL): https://github.com/leokhoa/laragon/issues/1492
- Laravel Herd — databases (Windows): https://herd.laravel.com/docs/windows/getting-started/databases.md
- Laravel Herd — databases (macOS): https://herd.laravel.com/docs/macos/getting-started/databases.md
- Local (WP Engine) — connect a database tool (AdminNeo; external tools): https://localwp.com/help-docs/advanced/connect-a-tool-like-mysql-workbench-to-my-local-sites-database/
- DDEV — database management (import-db/export-db; phpMyAdmin/Adminer as add-ons; snapshots; clients in containers): https://docs.ddev.com/en/stable/users/usage/database-management/
- HeidiSQL — official site (native client, features): https://www.heidisql.com/
- DBX (`t8y2/dbx`) — repository, README headline ("100+ databases in 25 MB. Desktop, Docker, CLI, built-in AI assistant, and MCP Server."), LICENSE (Apache-2.0), repo topics (`rust`, `tauri`, `vue`), stars/forks, sponsor table (TrustAsia code signing): https://github.com/t8y2/dbx ; releases (version not pinned; asset list failed to load in this pass): https://github.com/t8y2/dbx/releases
- MariaDB — `mariadb-dump` (`--single-transaction`, `--triggers` default, `--routines`/`--events` opt-in, `--databases`, `--default-character-set` default utf8mb4, `--opt`/`--quick` default, sandbox preamble, `--result-file` overwrite-on-error): https://mariadb.com/docs/server/clients-and-utilities/backup-restore-and-import-clients/mariadb-dump
- MariaDB — `mariadb` command-line client (latin1 default, `--default-character-set`, `--sandbox` disabled commands): https://mariadb.com/docs/server/clients-and-utilities/mariadb-client/mariadb-command-line-client
- Rust crates: crates.io API and GitHub repos — `mysql_async` (0.37.1, MIT OR Apache-2.0) https://github.com/blackbeam/mysql_async ; `mysql` (28.0.4) https://github.com/blackbeam/rust-mysql-simple ; `sqlx` (0.9.0) https://github.com/launchbadge/sqlx ; `mysql_common` (0.38.2) https://github.com/blackbeam/rust_mysql_common
