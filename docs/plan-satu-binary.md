# Plan: Single Unified `devku` Binary

Status: completed 2026-10-31.

## Goal

Unify `devku` (GUI desktop app) and `devku-cli` (headless CLI) into a single `devku.exe` binary:

- Without arguments: launches the desktop GUI.
- With subcommands: runs headless CLI operations (e.g. `devku status`, `devku up`, `devku install-php`).
- With `--hidden`: launches the GUI in hidden mode for Windows startup/tray background execution.

End result: the installer ships a single executable while users retain full CLI control.

## Decisions

1. **Routing in `main.rs` before `devku_lib::run()`:** CLI pathways bypass WebView and System Tray initialization.
2. **Windows subsystem remains `windows` in release:** Avoids flashing command prompt windows when launched from Windows Explorer or shortcuts.
3. **Console attachment in release CLI mode:** Attaches to parent terminal via `AttachConsole(ATTACH_PARENT_PROCESS)`.
4. **`--hidden` flags GUI background launch:** Not treated as a CLI subcommand.
5. **Exit codes:** 0 for success, 1 for errors, 2 for unrecognized command syntax.
6. **No separate `devku-cli.exe` alias:** The standalone binary is removed in favor of the unified binary.

## Structure

```
src-tauri/src/
├── main.rs        routing: CLI vs GUI dispatch
├── console.rs     console attachment in release builds
├── cli.rs         command parsing and execution
├── lib.rs         Tauri application logic and backend services
```
