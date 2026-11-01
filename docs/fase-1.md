# Phase 1: Core Services

Date: 2026-10-30

Core background services engine. Everything can be operated headlessly via `devku`, allowing UI commands in subsequent phases to invoke the same underlying functions.

## Module Map

| File | Responsibility |
|---|---|
| `paths.rs` | Single root for configuration, engines, runtime (overridden via `DEVKU_HOME`) |
| `config.rs` | Schema `config.json`, atomic load/save, `served_sites()`, `php_versions_in_use()` |
| `ports.rs` | Deterministic port allocation per PHP version, port availability check |
| `proc.rs` | Generic supervisor: spawn, readiness check, restart with exponential backoff, crash-loop breaker |
| `php.rs` | PHP zip extraction, `php.ini` bootstrap, `php-cgi -b` command specification |
| `caddy.rs` | `WebServerBackend` trait + `CaddyBackend`: Caddyfile generation, reload, process specification |
| `api.rs` | Minimal HTTP client for Caddy admin API (`POST /load`) |
| `hosts.rs` | Tagged hosts block manager, only used when TLD != `localhost` |
| `runtime.rs` | Orchestration: port allocation -> PHP -> Caddy, clean shutdown |
| `cli.rs` | Headless CLI: `status`, `sites`, `check`, `install-php`, `install-caddy`, `add-site`, `up` |

## Implementation Decisions

1. **`process-wrap` 10 instead of `command-group`:** `process-wrap` is actively maintained and supports Windows Job Objects cleanly.
2. **Zero external HTTP dependencies (no `reqwest`):** Caddy's admin API operates over plain loopback HTTP, so `api.rs` writes a single HTTP request directly to a `TcpStream`. This saves ~1.5 MB in binary size.
3. **No `clap` CLI parser:** The internal CLI requires only 8 commands; a manual 20-line parser avoids adding heavy dependencies.
4. **Deterministic port allocation:** PHP versions are sorted and allocated ports sequentially starting from 9001 (e.g. 7.4.33 on 9001, 8.3.36 on 9002).
5. **Pre-write configuration validation:** Generated configurations are validated via `caddy validate` before writing to disk, ensuring invalid configs are never saved.

## Test Verification

```
test result: ok. 18 passed; 0 failed
```

- Verified Caddyfile generation per site and port isolation.
- Validated zip slip prevention (`../` path traversal protection).
- End-to-end multi-site execution tested with PHP 8.3 and PHP 7.4 running simultaneously on port 80.
- Clean process shutdown on exit with zero orphaned processes left running.
