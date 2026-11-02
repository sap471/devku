# DevKu

PHP development environment for Windows: lightweight, multi-PHP per domain, integrated with system runtimes.

Replace XAMPP/Laragon with something smaller, faster, and more manageable. Full plan: [`PLAN.md`](PLAN.md). Changelog: [`CHANGELOG.md`](CHANGELOG.md). Domain model: [`GLOSSARY.md`](GLOSSARY.md).

## Status

| Phase | Content | Status |
|---|---|---|
| 0 | Architectural spike | ✅ [results](docs/spike-fase-0.md) |
| 1 | Core services + CLI | ✅ [results](docs/fase-1.md) |
| 2 | UI project manager (Vue 3 rework) | ✅ sites list, custom titlebar, settings page, wizard, logs |
| 3 | Runtime integration (Bun/Node/PHP/Composer) | ✅ [results](docs/fase-3.md) |
| 4 | Polish: tray, SSL trust, onboarding, autostart | ✅ |
| 5 | Apache backend | ✅ internal `httpd` behind Caddy, auto-installed |
| 6 | Distribution: NSIS and signed auto-updates | NSIS created, auto-update deferred |

## How It Works

```
Browser :80/:443
   │
   ▼
Caddy ──┬── php_fastcgi ──► php-cgi (one pool per PHP version)
        └── reverse_proxy ──► httpd :8080 ──► php-cgi (sites that choose Apache)
```

- `*.localhost` resolves natively on Windows 11 without editing `hosts` and without administrator privileges.
- Each PHP version runs its own dedicated `php-cgi` pool, allowing App A to run PHP 8.1 while App B runs PHP 8.4.
- Caddy is the only entry point: it owns 80/443, TLS, and the domain, then forwards sites whose backend is Apache to a loopback `httpd` on port 8080. Apache sites therefore keep the same URL and HTTPS as everything else; port 8080 is internal and never exposed.
- Apache has its own generated `httpd.conf` and its own Settings tab, separate from the Caddyfile. DevKu installs it itself from Apache Lounge with SHA-256 verification, and `httpd` starts only while at least one site uses it.
- Processes are supervised from within the DevKu binary. If a `php-cgi` instance exits unexpectedly, it is restarted with exponential backoff.

## Where Data Lives

Three locations with separate jobs:

```
install dir   C:\devku             devku.exe and the uninstaller (read-only)
data root     %APPDATA%\DevKu      engines, runtime, config.json, logs, databases
workspace     %USERPROFILE%\DevKu\sites   project folders (content, not state)
```

- The data root can be moved from Settings > System. DevKu copies the tree to an empty folder, leaves a small `location.json` pointer in `%APPDATA%\DevKu`, and restarts. `DEVKU_HOME` overrides everything and locks the move from the UI.
- The workspace is the starting folder for the Add Site wizard and the Open Workspace button. It is not a site registry: projects inside it still have to be registered, and they may live anywhere.
- Engines stay out of the install directory on purpose, so downloading an engine never needs elevation and an update never touches your data.
- `bun tauri dev` uses the same data root as the installed app, so a dev build reuses your engines, config, and MariaDB data. Set `DEVKU_HOME` to a separate folder when you want the two isolated.
- Uninstalling removes only the install directory. The data root stays, because re-downloading engines costs time and bandwidth; tick the uninstaller's data checkbox to remove it. The uninstaller detaches the `bin` shim from the user PATH before deleting the binary, so no dead PATH entry is left behind.

## Running Core (Headless / CLI)

The same `devku` binary also serves as a CLI: without arguments it launches the desktop UI, while subcommands run headlessly.

```powershell
cargo build --manifest-path src-tauri\Cargo.toml

$devku = "src-tauri\target\debug\devku.exe"
& $devku install-caddy "C:\path\to\caddy.exe"
& $devku install-php "C:\path\to\php-8.4-nts.zip" 8.4.1
& $devku add-site app-a "App A" "D:\projects\app-a\public" 8.4.1
& $devku up
```

Other commands: `status`, `sites`, `check`, `runtimes`, `php-path`, `install-composer`, `composer-path`, `autostart`, `export-config`, `import-config`, `trust`. Run `devku --help` for the complete list. Exit codes: 0 for success, 1 for error, 2 for unrecognized command. `DEVKU_HOME` overrides configuration and engine locations (defaults to `%APPDATA%\DevKu`).

PowerShell does not automatically await GUI binaries invoked with `&`, meaning exit codes are not captured directly. Pipe to `| Out-String` or use `Start-Process -Wait -PassThru` if exit codes are required.

## Development

```powershell
bun install          # install frontend dependencies
bun run check        # typecheck TypeScript & Vue
bun run tauri dev    # launch desktop application in dev mode

cargo test --manifest-path src-tauri\Cargo.toml --lib
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets
```

Package manager: **Bun**. See [`AGENTS.md`](AGENTS.md).
