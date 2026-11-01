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
| 5 | Apache backend | pending |
| 6 | Distribution: NSIS and signed auto-updates | NSIS created, auto-update deferred |

## How It Works

```
Browser :80
   │
   ▼
Caddy  ──► php_fastcgi ──► php-cgi (one pool per PHP version)
```

- `*.localhost` resolves natively on Windows 11 without editing `hosts` and without administrator privileges.
- Each PHP version runs its own dedicated `php-cgi` pool, allowing App A to run PHP 8.1 while App B runs PHP 8.4.
- Processes are supervised from within the DevKu binary. If a `php-cgi` instance exits unexpectedly, it is restarted with exponential backoff.

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
