# DevKu: Plan

PHP development environment for Windows: lightweight, multi-PHP per domain, integrated with system runtimes.

## Steps

- [x] **Phase 0**: Foundations (architectural decisions, repository scaffold, technical spike) -> [results](docs/spike-fase-0.md)
- [x] **Phase 1**: Core services (Caddy manager, php-cgi pool + supervisor, port allocator) -> [results](docs/fase-1.md)
- [x] **Phase 2**: Project manager (UI site list + wizard + download/install PHP from app, reworked to Vue 3)
- [x] **Phase 3**: Runtime integration (runtime detection, PTY terminal, script runner, Caddy/Bun/Composer downloads) -> [results](docs/fase-3.md)
- [x] **Phase 4**: Polish (tray, SSL trust, onboarding, export/import config, auto-start login)
- [x] **Phase 5 (v2)**: Pluggable backend (Apache as an option via Caddy reverse proxy)
- [ ] **Phase 6 (later)**: Distribution (NSIS installer and signed auto-updates)

---

## 1. Summary

DevKu is a lighter and more modern alternative to XAMPP/Laragon, focusing on three areas where Laragon falls short: **multi-PHP per domain**, **system runtime integration**, and a **swappable backend architecture**.

### Design Principles

1. **Lightweight is a feature, not a bonus.** Idle processes do not run. Apache only runs when a site requires it.
2. **One port for all.** All hostnames run on ports `:80` and `:443`. No `:8080` in URLs.
3. **Do not duplicate binaries.** If the host system already has Bun, Node, or PHP, reuse them.
4. **Configuration as text.** All configurations must be readable, diffable, and commit-ready.
5. **Core is agnostic of backend.** All web server interactions go through a single interface.

### Measurable Targets

| Metric | Target |
|---|---|
| Idle RAM (zero active sites) | <= 25 MB |
| RAM with 2 active sites | <= 70 MB |
| Installer size (without PHP) | <= 15 MB |
| Time to add a new site | <= 10 seconds |

---

## 2. Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        DevKu Core (Rust)                    │
│                                                             │
│  ┌──────────────────┐  ┌────────────────┐  ┌──────────────┐ │
│  │   Site Manager   │  │  Port Manager  │  │  Supervisor  │ │
│  │  sites/ & configs│  │  dynamic 9000+ │  │  php-cgi etc │ │
│  └────────┬─────────┘  └───────┬────────┘  └───────┬──────┘ │
│           │                    │                   │        │
│  ┌────────▼────────────────────▼───────────────────▼──────┐ │
│  │               WebServerBackend Trait                   │ │
│  └────────┬───────────────────────────────────────┬───────┘ │
└───────────┼───────────────────────────────────────┼─────────┘
            │                                       │
            ▼                                       ▼
┌───────────────────────┐               ┌─────────────────────┐
│     CaddyBackend      │               │    ApacheBackend    │
│  (Default, Phase 1)   │               │     (v2, Phase 5)   │
│                       │               │                     │
│  - Caddyfile gen      │               │  - httpd.conf gen   │
│  - php_fastcgi 9000+  │               │  - mod_fcgid 9000+  │
│  - tls internal       │               │  - reverse proxy ke │
│  - Admin API reload   │               │    Caddy :80/:443   │
└───────────────────────┘               └─────────────────────┘
```

---

## 3. Spike Findings (Phase 0)

1. **Caddy natively handles multiple upstream FastCGI pools.** Each `php_fastcgi` block can target a different port on the same server instance.
2. **`*.localhost` resolves automatically without administrator privileges on Windows 11.** No `hosts` file editing is needed for default domains.
3. **`php-cgi` on Windows requires process supervision.** While child processes respawn automatically, if the parent process dies the whole pool goes down. DevKu monitors the parent process PID.
4. **Port 80 does not require admin privileges on Windows.** Windows has no concept of privileged ports below 1024. Caddy serves on `:80` without elevation.

### Port Map

| Port | Service | Bind | Notes |
|---|---|---|---|
| 80 / 443 | Caddy | all interfaces | No admin privileges required on Windows |
| 2019 | Caddy admin API | loopback | Dynamic config reload without full restart |
| 8080 | Apache | loopback | Only when Apache backend is active |
| 9001+ | php-cgi per version | loopback | Dedicated port per PHP version via `PHP_FCGI_CHILDREN` |
| 3306 | MariaDB | loopback | Default database port, configurable |

---

## 4. Technical Decisions

| Area | Decision | Reason |
|---|---|---|
| Default web server | Caddy 2 | Single binary, zero DLLs, native HTTPS, admin API |
| PHP runtime | `php-cgi.exe` FastCGI | Standard FastCGI implementation on Windows |
| GUI | Tauri 2 + Vue 3 | ~5 MB executable, native WebView2, lightweight reactive UI |
| Logic language | Rust | Process supervision, file watchers, Win32 API integration |
| Global config | `%APPDATA%\DevKu\config.json` | Single source of truth, standard JSON |
| Per-site config | Optional `.devku.json` in project root | Project-specific overrides that can be committed |
| Default TLD | `localhost` or `test` | `*.localhost` resolves natively on Windows 11 without UAC |
| Process supervisor | `process-wrap` + tokio | Windows Job Objects ensure zero orphaned processes |
| Package manager | **Bun** (fallback Node/npm) | Fast, modern package management |

---

## 5. Execution Order

```
Phase 0 ──► Phase 1 ──► Phase 2 ──► Phase 3 ──► Phase 4 ──► Phase 5
 spike       core       UI site     runtime     polish     apache
   ▲           ▲
   │           └── v1 dogfoodable
   └── gate: all spikes passed before proceeding
```
