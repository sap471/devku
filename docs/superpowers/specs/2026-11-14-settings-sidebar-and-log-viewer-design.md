# Settings Sidebar and Service Log Viewer

## Intent

Two problems, one page.

The Settings sidebar lists five tabs as equals even though their content is not
equal (Caddy has three rows, PHP has four cards). The user chose a two-level
sidebar because more services are planned (MySQL, Mailpit, Redis), and a flat
list has no room to grow.

Service logs (PostgreSQL, MariaDB, Caddy, Apache) are only reachable as files on
disk. When a service fails, the reason sits in `runtime/logs/<name>.log` and the
UI never shows it. The per-site Log tab already renders logs; service logs have
no entry point at all.

## Goals

- Sidebar with collapsible groups, each holding its tabs.
- A Log tab in Settings that lists every log file DevKu writes, with size and
  modification time, and renders the selected one with a live refresh.
- A jump from a failed database card (and from Caddy and Apache) straight to
  that service's log.
- Reading a log must not load a whole file: the 1 MB `wordpress.log` and the
  growing `caddy.log` are read by tail only.

## Non-goals

- Log rotation, retention, or truncation.
- Writing `error_log` into existing `php.ini` files. Rewriting a user's ini
  behind their back is a behavior change nobody asked for. The empty PHP pool
  log gets an explanatory empty state instead.
- Viewing logs over time (history, charts), or shipping logs anywhere.
- Changing the per-site Log tab, which already works.

## Decisions

- **Sidebar groups** (the option the user picked, "C"):

  | Group | Tabs |
  |---|---|
  | Runtime | PHP & Runtime |
  | Data & Server | Database, Web Server (Caddy), Apache |
  | Aplikasi | Sistem & Integrasi, Log |

  Groups are collapsible and start open. The group holding the active tab is
  forced open whenever the tab is selected from anywhere, so a tab can never be
  hidden by a collapsed parent.

- **`SettingsSubTab`** becomes `"php" | "database" | "caddy" | "apache" |
  "system" | "logs"`. The old `"server"` value is renamed to `"caddy"`; it is
  referenced only by `SettingsView.vue` and the store type, so nothing else
  breaks.

- **Log sources are classified in Rust**, one place, testable. The frontend does
  not parse filenames. Classes: `server`, `database`, `php`, `site`.
  Classification by file name:
  - `caddy.log`, `apache.log`, `apache-error.log` → `server`
  - `postgres.log`, `mariadb.log`, `mysql.log` → `database`
  - `php-<version>.log` → `php`
  - `apache-<site>-access.log`, `apache-<site>-error.log`, `<site>.log` → `site`

- **Tail reads are byte-bounded**: if the file exceeds 256 KB, seek to the last
  256 KB, drop the first partial line, then take the requested number of lines.
  Line count choices: 200, 1000, 5000.

- **The log pane becomes its own component** (`LogsPane.vue`). `SettingsView.vue`
  is already 1674 lines; the new pane is roughly 250 more and has its own state
  (selection, polling, filters). Its siblings stay inline for now; splitting
  them is a separate change.

- **Service log state is separate from the site log state.** The site tab uses
  `logFor` / `logLines`; the new pane uses `serviceLogFor` / `serviceLogLines`,
  so opening a service log cannot clobber an open site log.

- **Jumping to a log** is one store function, `openLog(name)`: it switches the
  main tab to Settings, sets `settingsTab = "logs"`, selects the source, and
  starts loading. Database cards pass `db.engine`, which matches the log file
  name for both `postgres` and `mariadb`.

## Accessibility requirements

- Group headers are `<button>` with `aria-expanded` and `aria-controls`.
- The active tab carries `aria-current="page"`.
- The source list is a real list (`role="list"`), each item a button; the
  selected one carries `aria-current="true"`.
- The log viewport is a `<pre>`-like region with `aria-live="off"` (it changes
  every 2 s; announcing it would be noise) and an accessible label naming the
  file.
- Filter toggle and search input have visible labels or `aria-label`.
- Four explicit states: loading, empty (no sources), empty (file has no lines),
  error, plus ready.

## Where it lives

- `src-tauri/src/logs.rs` (new): classification, source listing, tail reading.
- `src-tauri/src/session.rs`: delegate `tail_log`, add `log_sources`.
- `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`: `log_sources` command,
  `open_in` kind `"log"`.
- `src/components/settings/LogsPane.vue` (new).
- `src/components/settings/SettingsView.vue`: grouped sidebar, `caddy` rename,
  Log tab, "Lihat Log" buttons.
- `src/store/devku.ts`, `src/types.ts`: types, state, `openLog`, load functions.
