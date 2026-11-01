# Phase 3: Runtime Integration

Status: completed.

This phase integrates system runtimes into DevKu: detecting system PHP/Node/Bun, downloading and installing runtimes directly from the application, and running project-specific CLI commands within the integrated terminal drawer.

## Components Built

| Section | Files | Content |
|---|---|---|
| Runtime Detection | `runtime_detect.rs` | Discovers `bun`, `node`, `composer`, `php` on system PATH, normalizes versions |
| Downloads | `download.rs` | Official PHP `releases.json`, GitHub releases for Caddy/Bun, checksum verification, live progress, single slot |
| Extraction | `archive.rs` | Zip extraction to target directory, stripping root wrapper folder, preventing `../` path traversal |
| Terminal | `terminal.rs`, `src/terminal.ts` | PTY via `portable-pty` + `xterm.js`, enriched PATH, auto `cd` to project root |
| Site Commands | `scripts.rs` | `composer install`, `artisan migrate`, `spark serve`, `package.json` scripts |
| PHP on PATH | `path_env.rs` | Shim `bin/php.bat`, user PATH registration, `WM_SETTINGCHANGE` broadcast |

## Technical Decisions

- **HTTPS Client: `reqwest` with `native-tls`:** Uses Windows native Schannel TLS without compiling additional C crypto libraries.
- **Checksum Verification:** PHP releases verify SHA-256; Caddy verifies SHA-512 via `checksums.txt`.
- **PHP CLI Shim:** User PATH points to a static shim directory (`bin/php.bat`) rather than hardcoding version folders, allowing instant version switching.

## Known Limitations

- The PHP shim is a `.bat` file; GUI applications specifically expecting `php.exe` directly on PATH may not detect the batch wrapper without explicit configuration.
- Bun releases do not provide official release checksums; integrity relies on GitHub release HTTPS transport.
