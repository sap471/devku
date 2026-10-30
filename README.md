# DevKu

Development environment PHP untuk Windows: ringan, multi-PHP per domain, terintegrasi dengan runtime sistem.

Ganti XAMPP/Laragon dengan sesuatu yang lebih kecil dan lebih bisa diatur. Rencana lengkap: [`PLAN.md`](PLAN.md).

## Status

| Fase | Isi | Status |
|---|---|---|
| 0 | Spike arsitektur | ✅ [hasil](docs/spike-fase-0.md) |
| 1 | Core services + CLI | ✅ [hasil](docs/fase-1.md) |
| 2 | UI project manager | belum |
| 3 | Integrasi runtime (Bun/Node) | belum |
| 4 | Distribusi & polish | belum |
| 5 | Backend Apache | belum |

## Cara kerja

```
Browser :80
   │
   ▼
Caddy  ──► php_fastcgi ──► php-cgi (satu pool per versi PHP)
```

- `*.localhost` resolve native di Windows 11 — tidak perlu edit hosts, tidak perlu admin.
- Setiap versi PHP punya satu pool `php-cgi` sendiri, jadi app A bisa PHP 7.4 dan app B PHP 8.3.
- Proses diawasi dari dalam binary DevKu; kalau `php-cgi` mati, dia dihidupkan lagi dengan backoff.

## Menjalankan core (tanpa UI)

```powershell
cargo build --manifest-path src-tauri\Cargo.toml --bin devku-cli

$cli = "src-tauri\target\debug\devku-cli.exe"
& $cli install-caddy "C:\path\ke\caddy.exe"
& $cli install-php "C:\path\ke\php-8.3-nts.zip" 8.3.36
& $cli add-site app-a "App A" "D:\projects\app-a\public" 8.3.36
& $cli up
```

Perintah lain: `status`, `sites`, `check`. `DEVKU_HOME` menimpa lokasi config/engines (default `%APPDATA%\DevKu`).

## Development

```powershell
bun install          # dependency frontend
bun run check        # typecheck
bun run tauri dev    # aplikasi desktop

cargo test --manifest-path src-tauri\Cargo.toml --lib
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets
```

Package manager: **Bun**. Lihat [`AGENTS.md`](AGENTS.md).
