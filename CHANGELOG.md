# Changelog

All user-facing changes are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning follows [SemVer](https://semver.org/).

## [Unreleased]

### Fixed

- Opening Pengaturan threw "reactive is not defined" and left the page blank: the grouped sidebar uses `reactive` for the open/closed group state, but the import was never added. The pane renders again. ($h)

- The install progress banner for a database never went away: the store kept the last `download://progress` event ("selesai", no total) after the command returned, so the bar stayed filled as if the install were still running. `installDatabase` now seeds and clears that state around the call, so the banner disappears when the install ends, whether it succeeded or failed. (`17d758b`)

- "Buka di Explorer" on a database card failed with "invalid args `kind` for command `open_in`". The store called the command without its required `kind` argument. The button now opens that engine's data folder through a new `db` kind, and the site "Buka di" menu, which had the same missing argument, passes `site`. (`4d58fc8`)

- PostgreSQL wrote `postgresql.conf` and `pg_hba.conf` under `runtime/postgres/`, where the server never reads them, so it ignored DevKu's port and auth rules and ran on its own defaults. Both files now live in the datadir. Backups and restores also logged in as `root`, a role PostgreSQL does not have; the superuser is now a backend property, `postgres` for PostgreSQL and `root` for MariaDB. Removing an engine's data now happens before the binaries are deleted, so a failed wipe no longer leaves a half-removed install. (`4d58fc8`)

- PostgreSQL failing to start with "database gagal siap: belum siap setelah 30s". The server spec passed `-D <datadir>` and `-p <port>` as single argv strings, so `postgres.exe` looked for a directory literally named `" C:/Users/.../databases/postgres"` inside its `bin` folder and exited on every restart. The data directory is now passed as `-D DATADIR`, two separate arguments, which is the only form `postgres.exe` accepts. (`fbe114e`)

- PostgreSQL `initdb` failing with "invalid authentication method" on the first click of Pasang. The backend passed `-A scram-sha-256` and similar flags as a single argv string with the flag and value glued together by a space; `Command::args` forwarded each element verbatim, so `initdb` received `" scram-sha-256"` (leading space) and rejected it. All four argument lists in the postgres backend now use the `--long=value` form, which carries no ambiguity. Additionally, the password file written for `initdb` was always empty; `initdb` refuses an empty pwfile, so a random password is now generated and stored before `initdb` runs. (`db7c75b`)

- Touchpad scrolling no longer rubber-bands the whole window. A two-finger gesture on a page the root cannot scroll made WebView2 play an elastic overscroll bounce, visibly pulling the titlebar and content down. The shell now sets `overscroll-behavior: none` on `html` and `body`. (`3429866`)

### Changed

- `bun run check` and `bun run build` now run `vue-tsc` instead of plain `tsc`, so mistakes inside Vue components are caught before a build. It immediately found the missing `reactive` import that broke Pengaturan, plus five unused imports and a click handler bound to the wrong signature. Those are fixed. (`4ccd2e9`, `d98e068`)

- The Caddy settings tab moved to the sidebar's Data & Server group and its internal key changed from `server` to `caddy`. Nothing about the pane itself changed. (`2675d89`)

- The application identifier changed from `com.devku.app` to `id.devku.desktop`, matching the domain the project actually lives under instead of claiming `devku.com`. The identifier only names the WebView2 profile folder, so this renames `%LOCALAPPDATA%\id.devku.desktop` (a cache that regenerates itself); app state in `%APPDATA%\DevKu` is untouched. Installer, uninstaller, autostart, and shortcuts do not use the identifier and keep working. (`c950858`)

- Database services are supervised per engine instead of as one slot. Every engine listed in `config.json` starts on its own port, and a failure in one engine is recorded for that engine alone while the others and the sites keep running. Settings now shows one card per engine with its own status, port, autostart, password, data folder, and remove action. Start, stop, autostart, backup, and remove commands take the engine key. (`e051862`)
- On the first start after this update, an existing MariaDB install moves from `engines/mariadb` to `engines/database/mariadb` automatically, so the binaries are kept and no re-download happens. (`82e512d`)
- MySQL Community Server is deliberately not implemented in this batch. The download source has since been verified (`cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.13-winx64.zip`, with the published MD5 sidecar on the same CDN), and the pin is decided: 8.4.13 LTS. A follow-up task implements it on the shared MariaDB driver. Until then, selecting MySQL in the configuration is refused with a clear error instead of silently running another engine.

- The Log tab now opens a log that does not exist yet, or is still empty, as an empty state instead of a red error, and a single line longer than the read window (a long stack trace) is shown whole instead of disappearing. The log console follows the app's light and dark themes, and Live stops polling when you leave Pengaturan. (`76f6da8`)

### Added

- A Log tab in Pengaturan shows every log DevKu writes: Caddy, Apache, PostgreSQL, MariaDB, PHP pools, and per-site logs, each with its file size and last write time. Lines containing `FATAL`, `PANIC`, `ERROR`, Apache `AH00xxx`, Caddy `"level":"error"`, or PHP fatal errors are marked red and can be filtered; warnings get their own filter. A Live toggle reloads the file every 2 seconds, and opening a log reads only the last 256 KB, so a megabyte log renders immediately instead of loading whole. Every database card now has a "Lihat Log" button, a failed card offers "Buka Log" next to its error, and the Caddy and Apache panes have their own log shortcut. New command: `log_sources`. (`d75aabe`, `0e91ce2`)
- The Settings sidebar is now grouped into three collapsible sections, Runtime, Data & Server, and Aplikasi, so more services can be added without turning the list into a wall of equal tabs. Selecting a tab from anywhere opens its group, so a collapsed section can never hide the active tab. Group headers report their state with `aria-expanded`, and the active tab carries `aria-current`. (`f37c2e3`, `46454e3`, `2675d89`)

- PostgreSQL can now be added from Pengaturan > Database with a "Tambah PostgreSQL" button instead of hand-editing `config.json`. The button shows only while no PostgreSQL entry exists; the new entry starts on port 5432 with autostart off. MySQL stays out of the offer until its backend is implemented. New command: `add_database_engine`. (`07bc3bb`, `c1f66e2`)
- PostgreSQL is now supported as a second database engine. It installs from the EDB Windows binaries zip (pinned to 18.6), initializes with `initdb -A scram-sha-256` so the superuser authenticates with a password just like MariaDB, binds only loopback, and can run alongside MariaDB or MySQL on its own port 5432. Backups use `pg_dump -Fp` (plain SQL), keeping the same `.sql` backup contract as MariaDB. EDB publishes no checksum for its zip, so the recorded-digest verification path is used, the same way Bun's archive is handled. (`57e199a`)

- Database engines are now an explicit list in `config.json` (`database.engines`) instead of a single implied MariaDB block. Each entry carries its own engine, port, and autostart. MariaDB and MySQL cannot be enabled together, because both are the MySQL family and would fight over the same port; PostgreSQL may run alongside either. Configs written before this change still load as MariaDB with their original port and autostart value, and a hand-edited file that lists MariaDB and MySQL together is rejected on load with a message naming the conflict. (`ab6c983`)
- Database backend is engine-neutral: the server contract no longer assumes MySQL's `user@host` accounts, and MariaDB-only recovery and password handling moved into the MariaDB implementation. Added a `restore` operation that streams a `.sql` backup into the client with the charset pinned to utf8mb4, so multibyte data survives the round trip. (`48d929c`)
- Database storage is now scoped per engine: binaries live under `engines/database/<engine>`, data under `databases/<engine>`, and generated config plus the stored password under `runtime/<engine>`. A second engine therefore cannot overwrite MariaDB's files. `backend_for` picks the implementation by engine, mirroring how Caddy and Apache are chosen. (`6012836`)

- Workspace sync button in the Situs panel. One click scans the workspace folder (default `~/DevKu/sites`) one level deep and registers every subfolder whose framework marker `detect` recognises, so projects dropped there no longer have to be added one by one through the wizard. Folders without a marker are reported, never registered, and a failure on one folder does not stop the rest. The scan saves and reloads once, not per folder. New command: `sync_workspace`. (`5b7352b`)
- Framework detection expanded from Laravel, CodeIgniter 3, CodeIgniter 4, and vanilla PHP to also cover WordPress, Symfony, Slim, and Drupal, plus a `composer.json` fallback that labels a Composer project as plain PHP. WordPress is checked before the generic `index.php` rule so a WordPress root is not mislabeled as `php`. (`5b7352b`)
- Repository licensed under MIT: `LICENSE` in the repository root, `license` metadata in `package.json` and `Cargo.toml`, a copyright string plus the license text in the installer bundle, and a License section in the README. (`f15d03d`)
- Titlebar shortcut to open the workspace folder, with the configured path in its tooltip. The folder is created on first use, so the button works before a project is ever placed there, and the label collapses to an icon-only button below 900px so the titlebar still fits the 780px minimum window width. (`16c3b5f`)
- Data root and workspace are now settings. The data root can be moved to another drive from Pengaturan > Sistem: DevKu stops services, copies the whole tree with a progress bar, writes a small `location.json` pointer beside the default home, then restarts, so engines are copied instead of downloaded again. `DEVKU_HOME` still wins and locks the move. The workspace folder is the starting point for the Add Site wizard and the Open Workspace button, while engines, runtime, and config stay in their own root. `devku status` also prints the workspace. New commands: `set_workspace`, `move_data_home`, `restart_app`. (`ad4ed5f`)
- Optional Apache backend per site, still fronted by Caddy: Caddy keeps 80/443 and reverse-proxies Apache sites to a loopback `httpd` (default `127.0.0.1:8080`) with the Host header preserved, so every site keeps its domain and HTTPS and never exposes a port. Apache has its own generated `httpd.conf` and its own Settings tab, separate from the Caddyfile. It runs PHP through the same php-cgi pool via `mod_proxy_fcgi`, starts only while at least one enabled site uses it, and restarts only when its generated config changes. DevKu installs Apache itself: it picks the latest Win64 build from Apache Lounge, verifies the published SHA-256, and extracts it to `engines/apache/<version>`. The internal port rejects 80, 443, and 2019. New commands: `set_site_backend`, `set_apache_port`, `install_apache`. (`026fc15`, `46e46f9`)
- Live log: toolbar toggle in the site Log tab that repolls `tail_log` every 2 seconds while the tab is open, removing the need to press Refresh to follow incoming entries. Polling pauses when leaving the tab and resumes with the toggle state remembered. (`46e46f9`)
- PHP extension manager in the PHP settings tab: per-version list combining `ext/php_*.dll` dlls and `extension=`/`zend_extension=` directives in php.ini, shown as rows with sliders. Toggling rewrites php.ini atomically and rejects enabling extensions whose dll is missing; when the php-cgi pool of that version is live the card offers a one-click pool restart since php-cgi only reads php.ini at startup. New commands: `php_extensions`, `set_php_extension`. (`026fc15`, `46e46f9`)
- Established English as the primary language across all repository documentation, specifications, and domain models (`GLOSSARY.md`). (`3f0507a`)
- Reworked UI frontend architecture to Vue 3 with Composition API and `<script setup>` for high modularity and low memory footprint. (`3f0507a`)
- Tauri v2 window customization with borderless window (`decorations: false`), native drag region (`data-tauri-drag-region`), and custom window controls (minimize, maximize, hide to tray). (`3f0507a`)
- Unified `AppTitlebar.vue` integrating brand identity, top-level tab navigation (Sites vs Settings), live server status badge, quick server start/stop, and collapsible terminal drawer toggle. (`3f0507a`)
- Dedicated multi-tab Settings view separating PHP runtime management, MariaDB database controls, Caddy web server routing, and Windows PATH/system toolchain configurations. (`3f0507a`)
- Sub-tabbed Site Detail panel (General info, formatted HTTP/PHP Access Logs, and quick development CLI commands). (`3f0507a`)
- Add Site Wizard modal with persistent backdrop (prevents accidental dismissal on backdrop clicks), native directory picker, and automatic framework detection. (`3f0507a`)
- Downloaded engine archives are cached; reinstalling the same version avoids re-downloading as long as checksums match. For packages without official checksums, digests are recorded after first download. Configuration section displays archive count and byte size, with a Clean button. (`0e3e090`)
- Local **MariaDB 11.4.13** server via Database panel in dashboard: install, start, stop, and independent autostart toggle. Version is pinned and verified with `sha256sums.txt`. (`80a23e1`)
- Database credentials use `root` with empty password, bound exclusively to `127.0.0.1`. Passwords are saved in `runtime/mariadb/root.pass`, outside `config.json`, to prevent accidental export. (`80a23e1`)
- MariaDB removal offers options to delete data and auto-export a `.sql` backup to the `backups` directory, which opens upon completion. (`80a23e1`)
- `mariadb` and `mariadb-dump` commands available in DevKu terminal. (`80a23e1`)
- Installed PHP versions list with Edit php.ini, Restart Pool, and Remove actions. Installation catalog only lists versions not currently installed. (`e945210`)
- **Edit php.ini** opens the file in VS Code, Zed, or Notepad as detected. Changes apply after restarting the pool. (`e945210`)
- Removing PHP disables associated sites, shifts default PHP, and redirects Composer. Removing all PHP unregisters shims from PATH and returns to 0 installed versions. (`e945210`)
- Shared dialog for destructive actions: title, data rows, input fields, nested checkboxes, and choice buttons. Config import and site removal use this. (`20dab20`)
- Windows NSIS per-machine installer with default directory `C:\devku`. DevKu runtimes and configs remain in `%APPDATA%\DevKu`. Installer requests UAC. (`6cc4912`)
- **Open** menu in site panel: Explorer, Terminal, VS Code, Zed, and copy path. Unavailable applications appear disabled rather than disappearing silently. (`6cc4912`)
- **Open data folder** button for DevKu data folder in dashboard Configuration section. (`6cc4912`)
- Project foundation: Tauri 2 scaffold, multi-PHP per domain spike, Caddy as edge server, and `localhost` TLD without hosts entries. (`e35dd0a`)
- Core services: `WebServerBackend` trait, Caddy backend, `PhpManager`, process supervisor with backoff, PortAllocator, and `devku` CLI. (`ea8190e`)
- App session, framework detection for Laravel, CodeIgniter 3, CodeIgniter 4, and vanilla PHP, Tauri commands, and antislop design guidelines. (`c35746d`)

### Changed

- Development servers moved from Vite 1420 / HMR 1421 to 1441 / 1442, in both `vite.config.ts` and `src-tauri/tauri.conf.json`, so `tauri dev` no longer collides with other local tools holding the old ports. (`e0ac6db`)
- Unification of GUI and CLI into a single `devku.exe` binary. Without arguments it launches the GUI; with subcommands it runs headlessly. Unrecognized subcommands exit with code 2. (`6a64b8b`)
- Caddy config is validated before writing, and reload uses `POST /load` so changes apply atomically without restarts. (`76d60bf`)
- PHP detection changed to folder-first: `PhpManager::installed()` scans `engines/php/*` and sorts versions numerically. Release catalog is merged with local versions, and `config.php.installed` is removed. (`6eaa4f9`)
- Shell layout inverted: site list is bounded, while the detail pane claims all remaining width when resizing. (`adc080b`)
- Windows created with `visible: false`, shown upon standard launch and kept hidden when launched at Windows login. (`13a487e`)
- Documentation: Phase 1 and Phase 3 results, Phase 0 spike reports. (`112335f`)
- PLAN synchronized with current implementation, including commit references. (`3f14c8e`)
- Phase 4 marked complete; signed auto-update and a clean-machine installer test moved to Phase 6. NSIS packaging itself was already in place from `6cc4912`. (`38ec738`)

### Fixed

- Uninstalling no longer leaves the PHP shim in the user PATH: the uninstaller runs `devku php-path off` before deleting the binary, so removing the data root does not leave a dead PATH entry. The confirm page also explains that `%APPDATA%\DevKu` (config, engines, databases) is kept unless the data checkbox is ticked. `devku php-path off` now works without a readable `config.json`, which is the state left behind by a partial uninstall. (`e52143e`)
- Local apps that set `DB_HOST=localhost` no longer stall about 2 seconds per request: MariaDB now also listens on the IPv6 loopback (`bind-address=127.0.0.1,::1`) when IPv6 is available, because Windows answers `localhost` with `::1` first and the unusable address cost ~2 s before the client fell back to IPv4. Measured on a WordPress site: 2.4 s down to 0.3 s. IPv4-only config is kept on machines without IPv6 so the server still starts. (`74386fd`)
- Window controls (minimize, maximize, close) in custom titlebar granted explicit Tauri capability permissions (`core:window:allow-minimize`, `allow-maximize`, `allow-unmaximize`, `allow-toggle-maximize`, `allow-close`, `allow-hide`, `allow-show`, `allow-start-dragging`) and decoupled from header drag region.
- Checksum lists with `./` prefixes (such as MariaDB) now parse and verify properly. (`80a23e1`)
- Nested dialog checkboxes restore their initial state when their parent is re-checked. (`80a23e1`)
- Machine-named root accounts skipped when altering passwords, since server runs with `skip-name-resolve`. (`80a23e1`)
- `devku --help` and `devku -h` help text prints to terminal in release builds when invoked from PowerShell or cmd. (`fa5035f`)
- Console window no longer flashes when DevKu queries runtime versions, Composer, or Caddy configuration. (`6cc4912`)
- `default-run = "devku"` configured so `tauri dev` is unambiguous. (`3246eb8`)
- Folders without PHP markers can still be registered as sites; framework detection is advisory rather than mandatory. (`35aebff`)
- Caddy no longer requests CA installation on startup, preventing app hangs waiting for elevation dialogs. (`ba46c69`)
- First-run wizard no longer reappears after being dismissed or after adding the first site. (`3f14c8e`)
- Changing a site's PHP version works again: `set_site_php` and `set_default_php` argument renamed from `php` to `version` to match what the frontend sends, and the Windows "launch at login" toggle now sends `enabled` as the command expects.

### Removed

- Standalone `devku-cli.exe` binary. Legacy scripts should invoke `devku` with the same subcommands. (`6a64b8b`)
- `php.installed` field in config, replaced by `engines/php` directory scanning. (`6eaa4f9`)

## How to Record Changes

1. Every commit that modifies behavior must add at least one line under `[Unreleased]` in the **Added**, **Changed**, **Fixed**, or **Removed** section.
2. Include the short commit hash in parentheses, e.g. `(abc1234)`, to trace changes back to git history.
3. Write from the user's perspective: describe what can now be done, not internal function names.
4. Active voice, clear English, no em dashes.
5. On release: replace `[Unreleased]` with version and date, then open a fresh `[Unreleased]` section above it.
