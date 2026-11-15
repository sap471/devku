# Design: add a database engine from the UI

Date: 2026-11-14
Size: small change

## Intent and why

Database engine cards in Settings are rendered per configured engine
(`config.database.engines`). A fresh or legacy config contains only MariaDB,
and nothing in the UI can append another engine, so PostgreSQL can never be
reached: its install command exists, its card never appears. Closing this gap
is what makes the modular-engine work from 2026-11-12 complete for users.

## Goals

- A user can add PostgreSQL to the engine list from Settings > Database.
- Adding respects the existing invariants: no duplicates, MariaDB and MySQL
  mutually exclusive (validated by the same code that validates the file).
- The new entry starts unconfigured-but-visible: its own card with a Pasang
  button, port 5432, autostart off.

## Non-goals

- No MySQL offer in the UI: its backend is not implemented (documented debt).
- No engine removal changes; `remove_database` already exists.
- No hand-editing requirement: editing `config.json` stays possible but is not
  the path.

## Approaches considered

1. **Add-engine action (chosen).** A Tauri command appends an engine entry to
   the config and saves; the UI offers the button only for engines that exist
   in the backend but not in the config (today: PostgreSQL). Keeps ownership
   where it already lives: the engine list is the config's job, the command is
   the door.
2. Always show every engine as a card, install auto-adds to config. Rejected:
   changes what the list means (chosen engines vs available engines), shows a
   MySQL card that cannot work, and quietly mutates config on install.
3. Hand-edit `config.json`. Works today, no code, but leaves the gap open for
   every other user.

## Decided

- `Session::add_database_engine(engine)` (session.rs): refuses MySQL with a
  clear error, refuses duplicates (through `DatabaseSettings::validate`),
  appends `DbEngineSettings { engine, port: engine.default_port(),
  autostart: false }`, saves config, refreshes the snapshot.
- Tauri command `add_database_engine(engine) -> Snapshot`, registered in the
  invoke handler.
- Store action `addDatabaseEngine(engine)` applies the returned snapshot and
  shows an info notice.
- SettingsView: in the Database pane, a quiet "Tambah PostgreSQL" button shown
  only when no postgres card exists yet.

## Left to the builder

- Test placement and setup follow the existing session-test patterns.
- Changelog entry per repo convention, commit hash appended after commit.
