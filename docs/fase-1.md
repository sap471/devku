# Fase 1 — Core services

Tanggal: 2026-10-30

Yang dibangun: mesin di belakang UI. Semua bisa dijalankan headless lewat `devku-cli`, jadi Fase 2 tinggal memanggil fungsi yang sama dari Tauri command.

## Peta modul

| File | Tanggung jawab |
|---|---|
| `paths.rs` | Satu root untuk config, engines, runtime (`DEVKU_HOME` bisa menimpa) |
| `config.rs` | Schema `config.json`, load/save atomik, `served_sites()`, `php_versions_in_use()` |
| `ports.rs` | Alokasi port deterministik per versi PHP, cek port benar-benar bebas |
| `proc.rs` | Supervisor generik: spawn, readiness, restart + backoff, crash-loop breaker |
| `php.rs` | Ekstrak zip PHP, bootstrap `php.ini`, spec `php-cgi -b` |
| `caddy.rs` | `WebServerBackend` trait + `CaddyBackend`: render Caddyfile, reload, spec proses |
| `api.rs` | Klien minimal untuk Caddy admin API (`POST /load`) |
| `hosts.rs` | Blok hosts bertanda, hanya dipakai saat TLD ≠ `localhost` |
| `runtime.rs` | Orkestrasi: alokasi port → PHP → Caddy, shutdown bersih |
| `bin/devku-cli.rs` | Kontrol headless: `status`, `sites`, `check`, `install-php`, `install-caddy`, `add-site`, `up` |

## Keputusan yang diambil saat implementasi

1. **`process-wrap` 10, bukan `command-group`.** Rekomendasi awal saya (`command-group` 5.0.1) ternyata rilis terakhirnya November 2023 dan README-nya menyebut `process-wrap` sebagai penerus. `process-wrap` aktif (September 2026) dan MSRV 1.87 masih di bawah rustc kita.
2. **Tanpa `reqwest`.** Caddy admin API hanya HTTP polos di loopback, jadi `api.rs` menulis satu request HTTP langsung ke `TcpStream`. Menghemat ~1.5 MB binary dan menghindari pilihan TLS backend.
3. **Tanpa `clap`.** CLI hanya butuh delapan perintah; parser manual 20 baris lebih kecil daripada dependency-nya.
4. **Port deterministik, bukan disimpan.** Versi PHP diurutkan, lalu dialokasikan mulai 9001. Hasilnya sama setiap start, jadi reload Caddy tidak mengubah upstream.
5. **Readiness = TCP connect.** PID hidup belum berarti siap menerima request; PHP bisa hidup tapi gagal bind.
6. **Shutdown eksplisit.** `KillOnDrop` menjaga anak tidak yatim saat DevKu mati mendadak, tapi jalur normal tetap memanggil `kill()` lalu `wait()` supaya log sempat ditutup.

## Bukti

### Unit test — 18 lulus

```
test result: ok. 18 passed; 0 failed
```

Mencakup: render Caddyfile per site + port terpisah, site nonaktif tidak dirender, versi PHP tanpa port = error, alokasi port deterministik dan unik, deteksi port terpakai, backoff tumbuh lalu mentok 5s, parsing entri zip yang berbahaya (`../`), deteksi zip dengan folder pembungkus, penulisan `php.ini` hanya mengaktifkan ekstensi daftar putih, merge blok hosts tidak menghapus entri asing dan idempoten.

### End-to-end — dua situs, dua versi PHP, port 80

`devku-cli up` dengan Caddy 2.11.7 + PHP 8.3.36 + 7.4.33 yang diekstrak lewat `install-php`:

```
ready  http://app-a.localhost  php=8.3.36  fpm=127.0.0.1:9002
ready  http://app-b.localhost  php=7.4.33  fpm=127.0.0.1:9001
```

```
$ curl http://app-a.localhost/
site=app-a php=8.3.36 uri=/
$ curl http://app-b.localhost/
site=app-b php=7.4.33 uri=/
```

Tanpa entri hosts, tanpa `--resolve`, tanpa admin. Proses saat berjalan: `caddy=1`, `php-cgi=10` (2 versi × 1 induk + 4 anak). Setelah `up` selesai: `0` proses tersisa.

### Supervisor — induk dibunuh

```
[devku] php-7.4.33: ready (pid 25644)
$ Stop-Process -Id 25644
[devku] php-7.4.33: backoff #1 400ms: exit kode -1
[devku] php-7.4.33: starting
[devku] php-7.4.33: ready (pid 23792)
```

`curl` berikutnya `200` untuk kedua domain, pool kembali 10 proses.

### Kegagalan nyata — PHP rusak

PHP 9.9.9 dipasang dengan binary yang langsung keluar:

```
[devku] php-9.9.9: backoff #2 800ms: tidak siap sebelum timeout
[devku] php-7.4.33: stopped
[devku] php-8.3.36: stopped
Error: PHP gagal siap: belum siap setelah 20s (tidak siap sebelum timeout)
```

Runtime membatalkan seluruh startup, mematikan yang sudah jalan, dan tidak meninggalkan proses.

## Validasi config Caddy

`caddy validate --config <file> --adapter caddyfile` memuat dan menyiapkan config tanpa menjalankannya. Dipakai berbeda tergantung keadaan:

| Keadaan | Alat | Alasan |
|---|---|---|
| Belum ada instance hidup | `caddy validate` | Satu pesan galat jelas, tanpa supervisor mencoba 5 kali |
| Instance sudah hidup | `POST /load` | Sudah memvalidasi sendiri dan atomik: ditolak = config lama tetap jalan |

Terverifikasi: `/load` dengan config rusak mengembalikan `400` beserta pesan, dan situs lama tetap `200`.

**Bug yang ditemukan lewat ini:** kode lama menulis Caddyfile ke disk *sebelum* reload. Config yang ditolak Caddy akan tersimpan, sehingga start berikutnya ikut gagal. Sekarang urutannya: render → validasi → baru tulis (`write_config`), dan pada jalur reload, tulis hanya setelah `/load` menerima.

Yang tidak dicakup `validate`: port bentrok, upstream `php-cgi` mati, dan masalah sertifikat. Itu kegagalan runtime, bukan config.

## Yang sengaja ditunda

| Item | Pindah ke | Alasan |
|---|---|---|
| Unduh PHP/Caddy dari internet | Fase 2 | Butuh UI progres unduhan; `install-php <zip>` sudah cukup untuk Fase 1 |
| Tauri command | Fase 2 | UI belum ada; semua fungsi sudah siap dipanggil |
| Backend Apache | Fase 5 | Trait `WebServerBackend` sudah jadi titik masuknya |
| HTTPS `tls internal` | Fase 4 | Fase 1 fokus ke jalur `http://` di port 80 |
| Hosts + elevasi | Saat user pilih TLD custom | Jalur `localhost` tidak membutuhkannya |
| Deteksi framework | Fase 2 | Butuh wizard, bukan mesin |

## Catatan untuk Fase 2

`Runtime::reload()` sudah ada tapi belum dipakai: menambah site saat runtime hidup seharusnya cukup reload Caddy **dan** memastikan versi PHP baru ikut di-spawn. Saat ini `Runtime` membekukan daftar versi di `start()`. Fase 2 perlu `ensure_version(version)` yang men-spawn supervisor baru tanpa mengganggu yang lama.
