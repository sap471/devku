# Phase 0 Spike Results

Date: 2026-10-30
Environment: Windows 11 build 26200, non-elevated user (`DESKTOP-RQLKUBV\aku`)
Binaries: Caddy 2.11.7, PHP 8.3.36 NTS, PHP 7.4.33 NTS

All spikes were executed as a **standard non-administrative user**.

## Summary

| Spike | Question | Result |
|---|---|---|
| A | Can php-cgi + Caddy `php_fastcgi` act as a front controller? | **Passed** |
| B | Can two PHP versions run in parallel across two domains? | **Passed** |
| C | Can php-cgi handle concurrent requests on Windows? | **Passed, with notes** |
| D | Does writing to hosts require elevation? | **Confirmed requires admin: avoidable via `*.localhost`** |

---

## Spike A: Caddy + php-cgi + Front Controller

Configuration: `php-cgi.exe -b 127.0.0.1:9001` + Caddy `php_fastcgi 127.0.0.1:9001`.

```
GET /             -> 200, SCRIPT_NAME=/index.php, REQUEST_URI=/
GET /users/123    -> 200, SCRIPT_NAME=/index.php, REQUEST_URI=/users/123
GET /nope.php     -> 200 (rewritten to index.php, identical to Laravel's .htaccess)
```

`php_fastcgi` provides `try_files` and front-controller routing natively. No manual `rewrite` is necessary.

**Conclusion:** Frameworks like Laravel and CodeIgniter 4 run without `.htaccess` and without custom per-framework web server configurations.

---

## Spike B: Multi-PHP Per Domain

Two `php-cgi` pools (9001 for PHP 8.3.36, 9002 for PHP 7.4.33) and two Caddy blocks:

```
app-a.localhost:8080 -> php_fastcgi 127.0.0.1:9001 -> PHP 8.3.36
app-b.localhost:8081 -> php_fastcgi 127.0.0.1:9002 -> PHP 7.4.33
```

Both served simultaneous requests with isolated versions and distinct loaded extensions (`mbstring`).

**Zero-downtime reloads:** Calling `POST /load` on Caddy's admin API (`:2019`) applies new configuration dynamically without process restarts.

**Conclusion:** Multi-PHP per domain is fully supported on a single Caddy instance.

---

## Spike C: Concurrency on Windows

`php-cgi.exe` on Windows supports worker pools via the environment variable `PHP_FCGI_CHILDREN`:

- When `PHP_FCGI_CHILDREN=4` is set, the parent process spawns 4 worker children.
- Requests execute concurrently across workers.
- If a child worker crashes, the parent spawns a replacement worker.
- If the parent process exits, the entire pool goes offline.

**Conclusion:** DevKu must supervise the parent process PID directly.

---

## Spike D: Hosts & Elevation

- Modifying `C:\Windows\System32\drivers\etc\hosts` as a standard user throws `UnauthorizedAccessException`. **Elevation is required.**
- **Windows 11 natively resolves wildcard `*.localhost` to loopback** without modifying the `hosts` file:
  - `app-a.localhost` -> `127.0.0.1`
  - `my-app.localhost` -> `127.0.0.1`
- Tested end-to-end: `curl http://app-a.localhost:8080` succeeds without `--resolve` and without touching `hosts`.

**Consequences:**
- Default TLD is `localhost` (or `test` with fallback). Zero setup, zero UAC elevation.
- Hosts file modification is kept as an optional fallback for custom TLDs.

---

## Port 80 Without Admin

Binding `127.0.0.1:80` and `0.0.0.0:80` succeeds without administrative elevation on Windows. Caddy serves `http://app.localhost` directly on port 80.
