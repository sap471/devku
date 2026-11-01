# Changelog

All user-facing changes are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning follows [SemVer](https://semver.org/).

## [Unreleased]

### Added

- Data root and workspace are now settings. The data root can be moved to another drive from Pengaturan > Sistem: DevKu stops services, copies the whole tree with a progress bar, writes a small `location.json` pointer beside the default home, then restarts, so engines are copied instead of downloaded again. `DEVKU_HOME` still wins and locks the move. The workspace folder is the starting point for the Add Site wizard and the Open Workspace button, while engines, runtime, and config stay in their own root. `devku status` also prints the workspace. New commands: `set_workspace`, `move_data_home`, `restart_app`. (`<hash>`)
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
