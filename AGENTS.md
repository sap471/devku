# DevKu

PHP development environment for Windows. See [`PLAN.md`](PLAN.md).

## Toolchain

| Area | Tool | Notes |
|---|---|---|
| Package manager (JS/TS) | **Bun** | Always `bun`, never `npm`/`pnpm`/`yarn` |
| Node fallback | Node 24 + npm | Only if Bun fails or a package is incompatible |
| Rust | cargo | Tauri backend |
| Frontend | Tauri 2 + WebView2 | Not Electron |

### Package Manager Rules

- Install: `bun install` (do not run `npm install`).
- Add packages: `bun add <pkg>` / `bun add -d <pkg>`.
- Run scripts: `bun run <script>`.
- Binaries: `bunx <tool>`.
- Lockfile: commit `bun.lock`. Never commit `package-lock.json` or `pnpm-lock.yaml`.
- **Fallback:** if `bun` fails (resolver error, postinstall requiring npm, native package unsupported by Bun), only then use `npm`. State the fallback reason clearly. Never silently switch.
- Never run `npm install` out of habit. Bun first.

## Conventions

- Code comments: only explain *why*, not *what*. No decorative comments.
- User configuration (`config.json`, `.devku.json`) must always be human-readable and diffable.
- Every behavioral change must be documented in `CHANGELOG.md` under `[Unreleased]`, with commit hash.
- Documentation: English is the primary language across all repository documentation and specifications.
- Rust: run `cargo fmt` and `cargo clippy` before committing.
- Frontend: run `bun run check` for typechecking.

## Download Quota

The project owner uses a metered internet plan. Engine binaries (PHP, Caddy, Bun, Composer, MariaDB) range from tens to hundreds of megabytes, so routine tests must not download them.

- Never use an empty `DEVKU_HOME` for testing. Run `.\scripts\dev\test-home.ps1`, which creates a test home environment from local cache using hardlinks without network access in seconds.
- Cache is seeded once using `-Seed`. That command pulls from the real installation at `%APPDATA%\DevKu\engines`, not from the internet.
- Downloads are only permitted during final release tests, and must be announced in advance.
- To verify zero network downloads during tests, run tests with `HTTPS_PROXY=http://127.0.0.1:9`. The PHP catalog is expected to fail.
- Test homes use database port 3307 to avoid conflicting with real instances. For tests that start services, stop any running DevKu instance first because ports 80, 443, and 2019 are shared, then restart it after completion.

## Antislop

UI and text must follow antislop rules. Consult these skills before modifying the interface:

- `antislop` (core, always)
- `antislop-ui`, `antislop-copywriting`, `antislop-human`, `antislop-layoutmobile`
- `antislop-code` when writing or cleaning comments

Visual design direction is documented in [`DESIGN.md`](DESIGN.md). Never design without direction; if direction is absent, mark the result as a draft.

Key rules enforced in this repository:

- No em dashes in text. Use commas, periods, colons, or parentheses.
- No buttons or links without real actions.
- Every data view must implement four explicit states: empty, loading, error, and ready.
- WCAG AA contrast required in both light and dark modes.
- Numbers and metrics must be genuine, never fabricated.
