# DevKu

Development environment PHP untuk Windows: ringan, multi-PHP per domain, terintegrasi dengan runtime sistem.

Ganti XAMPP/Laragon dengan sesuatu yang lebih kecil dan lebih bisa diatur. Rencana lengkap: [`PLAN.md`](PLAN.md). Riwayat perubahan: [`CHANGELOG.md`](CHANGELOG.md).

## Status

| Fase | Isi | Status |
|---|---|---|
| 0 | Spike arsitektur | ✅ [hasil](docs/spike-fase-0.md) |
| 1 | Core services + CLI | ✅ [hasil](docs/fase-1.md) |
| 2 | UI project manager | ✅ wizard, log, hapus situs |
| 3 | Integrasi runtime (Bun/Node/PHP/Composer) | ✅ [hasil](docs/fase-3.md) |
| 4 | Polish: tray, SSL trust, onboarding, autostart | ✅ |
| 5 | Backend Apache | belum |
| 6 | Distribusi: NSIS dan auto-update | NSIS dibuat, auto-update ditunda |

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

Binary `devku` yang sama juga menjadi CLI: tanpa argumen ia membuka aplikasi,
dengan subperintah ia berjalan headless.

```powershell
cargo build --manifest-path src-tauri\Cargo.toml

$devku = "src-tauri\target\debug\devku.exe"
& $devku install-caddy "C:\path\ke\caddy.exe"
& $devku install-php "C:\path\ke\php-8.3-nts.zip" 8.3.36
& $devku add-site app-a "App A" "D:\projects\app-a\public" 8.3.36
& $devku up
```

Perintah lain: `status`, `sites`, `check`, `runtimes`, `php-path`, `install-composer`, `composer-path`, `autostart`, `export-config`, `import-config`, `trust`. `devku --help` menampilkan daftar lengkap. Exit code: 0 sukses, 1 gagal, 2 perintah tidak dikenal. `DEVKU_HOME` menimpa lokasi config/engines (default `%APPDATA%\DevKu`).

PowerShell tidak menunggu proses GUI yang dipanggil dengan `&`, jadi exit code tidak terbaca langsung. Pakai `| Out-String` untuk menangkap keluaran, atau `Start-Process -Wait -PassThru` bila butuh exit code. Di cmd proses ditunggu seperti biasa.

## Development

```powershell
bun install          # dependency frontend
bun run check        # typecheck
bun run tauri dev    # aplikasi desktop

cargo test --manifest-path src-tauri\Cargo.toml --lib
cargo clippy --manifest-path src-tauri\Cargo.toml --all-targets
```

Package manager: **Bun**. Lihat [`AGENTS.md`](AGENTS.md).
