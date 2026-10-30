# Fase 3 — Runtime integration

Status: selesai. Commit `298902f` dan sebelumnya `d83fe3a`, `7f39bb4`, `21896c8`.

Fase ini membuat DevKu tidak lagi hanya panel situs: runtime sistem dikenali, PHP/Caddy/Bun bisa dipasang dari aplikasi, dan perintah proyek bisa dijalankan tanpa meninggalkan jendela DevKu.

## Yang dibangun

| Bagian | Berkas | Isi |
|---|---|---|
| Deteksi runtime | `runtime_detect.rs` | `where bun/node/composer/php`, versi dinormalisasi, sumber sistem atau bawaan |
| Unduhan | `download.rs` | `releases.json` PHP, rilis GitHub Caddy/Bun, checksum, progres, batal, satu unduhan sekaligus |
| Ekstraksi | `archive.rs` | zip ke folder tujuan, membuang satu folder pembungkus, menolak entri `../` |
| Terminal | `terminal.rs`, `src/terminal.ts` | PTY `portable-pty` + `xterm.js`, PATH diperkaya, cwd akar project |
| Perintah situs | `scripts.rs` | `composer install`, `artisan migrate/serve`, `spark serve`, skrip `package.json` |
| PHP di PATH | `path_env.rs` | shim `bin/php.bat`, PATH pengguna, broadcast `WM_SETTINGCHANGE` |

## Keputusan

**Klien HTTPS: `reqwest` 0.13 dengan `native-tls`.** Default reqwest membawa rustls + aws-lc-rs, yang berarti kriptografi C ikut dikompilasi. Dengan `native-tls`, TLS memakai schannel bawaan Windows. Biaya yang terukur: 643 ke 752 crate dan 34 ke 52 detik build pertama. `ureq` tidak dipakai karena kita juga butuh streaming unduhan.

**Integritas berkas berbeda per sumber, dan itu disengaja.** PHP punya SHA-256 di `releases.json`. Caddy menerbitkan `checksums.txt` berisi SHA-512, jadi algoritmanya dipilih dari panjang digest (64 = SHA-256, 128 = SHA-512). Bun tidak menerbitkan checksum apa pun; integritasnya bergantung pada HTTPS ke github.com. Perbedaan ini dicatat, bukan disembunyikan.

**PHP di PATH memakai shim, bukan folder versi.** PATH pengguna hanya menerima satu entri tetap, `%APPDATA%\DevKu\bin`. Isinya `php.bat` yang meneruskan argumen ke versi default. Ganti versi default berarti menulis ulang `php.bat`, bukan menyunting PATH lagi.

**Script runner menumpang terminal yang sama.** Tombol mengirim `Set-Location` lalu perintah ke PTY yang sudah terbuka. Tidak ada proses atau panel output kedua, jadi `artisan serve` yang berjalan lama tetap terlihat dan bisa dihentikan dengan Ctrl+C.

**Terminal memakai akar project, bukan akar web.** `Site.root` disimpan terpisah dari `Site.path` (document root). Untuk Laravel, terminal mendarat di folder yang punya `artisan`.

## Verifikasi

64 unit test lulus, clippy dan `tsc` bersih. Test berjaringan ditandai `#[ignore]` dan dijalankan manual:

```powershell
cargo test --lib installs_real_php   -- --ignored   # unduh PHP 8.3, SHA-256, ekstrak, php-cgi ada
cargo test --lib installs_real_caddy -- --ignored   # unduh Caddy, SHA-512, caddy.exe dijalankan
cargo test --lib installs_real_bun   -- --ignored   # unduh Bun, bun --version = tag rilis
cargo test --lib resolves_caddy      -- --ignored   # metadata GitHub, checksum 128 karakter
cargo test --lib register_then_unregister -- --ignored  # PATH pengguna kembali identik
```

Hasil nyata di mesin ini: PHP 8.3.36 terpasang dalam 7,9 detik, Caddy 2.11.7 dalam 10 detik, Bun 1.4.4 dalam 10,8 detik. Shim `php.bat` menjalankan `PHP 8.3.36 (cli)`. Aplikasi dibuka tanpa klik: `caddy=1 php-cgi=10`, dasbor tampil; setelah jendela ditutup semuanya nol.

## Batasan yang diketahui

- Antarmuka baru (baris Dasbor, dropdown versi PHP, tombol perintah) belum diuji dengan klik. Yang terbukti baru mesin dan alur datanya.
- Shim `php` berupa `.bat`, jadi aplikasi GUI yang mencari `php.exe` dengan jalur absolut tidak melihatnya.
- Bun tidak punya checksum resmi.
- Script runner mengirim perintah 900 ms setelah PTY baru dibuka. Kalau PowerShell lambat memuat, perintah pertama bisa hilang.
- Belum ada environment per situs (misalnya `NODE_ENV` atau variabel `.env` yang di-inject ke shell).
