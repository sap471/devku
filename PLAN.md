# DevKu — Plan

Development environment untuk PHP di Windows: ringan, multi-PHP per domain, terintegrasi dengan runtime sistem.

## Steps

- [x] **Fase 0** — Fondasi: keputusan arsitektur, scaffold repo, spike teknis → [hasil](docs/spike-fase-0.md)
- [x] **Fase 1** — Core services: Caddy manager, php-cgi pool + supervisor, port allocator → [hasil](docs/fase-1.md)
- [x] **Fase 2** — Project manager: UI daftar + wizard + unduh/pasang PHP dari aplikasi
- [x] **Fase 3** — Runtime integration: deteksi runtime, terminal PTY, script runner, unduh Caddy/Bun/Composer → [hasil](docs/fase-3.md)
- [x] **Fase 4** — Polish: tray, SSL trust, onboarding, export/import config, auto-start login
- [ ] **Fase 5 (v2)** — Pluggable backend: Apache sebagai opsi via Caddy reverse proxy
- [ ] **Fase 6 (nanti)** — Distribusi: installer NSIS dan auto-update bertanda tangan

---

## 1. Ringkasan

DevKu adalah pengganti XAMPP/Laragon yang lebih ringan dan modern, dengan fokus pada tiga hal yang tidak dimiliki Laragon: **multi-PHP per domain**, **integrasi runtime sistem**, dan **arsitektur backend yang bisa diganti**.

### Prinsip desain

1. **Ringan itu fitur, bukan bonus.** Proses yang tidak dipakai tidak jalan. Apache hanya hidup kalau ada site yang membutuhkannya.
2. **Satu port untuk semua.** Semua hostname di `:80`/`:443`. Tidak ada `:8080` di URL.
3. **Jangan duplikasi binary.** Kalau mesin sudah punya Bun/Node/PHP, pakai itu.
4. **Config sebagai teks.** Semua config bisa dibaca, di-diff, dan di-commit.
5. **Core tidak tahu backend.** Semua akses web server lewat satu interface.

### Target terukur

| Metrik | Target |
|---|---|
| RAM idle (tanpa site aktif) | ≤ 25 MB |
| RAM dengan 2 site aktif | ≤ 70 MB |
| Ukuran installer (tanpa PHP) | ≤ 15 MB |
| Waktu tambah site baru | ≤ 10 detik |
| Waktu ganti PHP version | ≤ 2 detik, tanpa restart site lain |

---

## 2. Pemetaan kriteria

| # | Kriteria | Solusi |
|---|---|---|
| 1 | Mudah membuat environment baru | UI 3 langkah: pilih folder → pilih PHP → selesai. Domain otomatis `nama.localhost` |
| 2 | Hostname untuk Laravel & CodeIgniter | `*.localhost` resolve native di Windows 11 (tanpa hosts) + vhost Caddy. Front controller `php_fastcgi` menangani keduanya |
| 3 | Multi-PHP per path/domain | Satu listener `php-cgi` per versi di port unik; tiap site menunjuk ke listener versinya |
| 4 | Bun sebagai runner TS default | Bun dibundel sebagai fallback; runner TS tanpa config di terminal panel |
| 5 | Interface simpel | Tauri: satu window, daftar site, 1 tombol start/stop, terminal di bawah |
| 6 | Ringan | Caddy + php-cgi lazy. Tanpa Chromium, tanpa Windows service, tanpa daemon permanen |
| 7 | Integrasi Node/Bun sistem | Deteksi `PATH` saat first run; binary sistem dipakai, bundel hanya jika tidak ada |

---

## 3. Arsitektur

```
Browser  :80 / :443
   │
   ▼
Caddy  :80/:443          ← edge: TLS, routing hostname, static files
   │
   ├── site modern ──────► php_fastcgi ──┐
   │                                     │
   └── site .htaccess ──► Apache :8080 ──┤   (v2, lazy — hanya jika dipakai)
                          mod_fcgid      │
                                         ▼
                              php-cgi per versi (loopback)
                              PHP 8.3 :9001  (PHP_FCGI_CHILDREN=4)
                              PHP 7.4 :9002
```

### Hasil validasi Fase 0

Empat spike dijalankan tanpa hak admin. Detail di [`docs/spike-fase-0.md`](docs/spike-fase-0.md). Yang mengubah desain:

1. **TLD default `localhost`, bukan `test`.** Windows 11 me-resolve `*.localhost` ke loopback tanpa entri hosts — nol setup, nol UAC. Hosts editor turun jadi fallback untuk TLD custom.
2. **`PHP_FCGI_CHILDREN` bekerja di Windows.** Tidak perlu banyak port + load balancing. Satu induk + N anak per versi, `2091 ms` vs `4193 ms` untuk dua request paralel.
3. **Wajib supervisor.** Anak `php-cgi` yang mati direspawn otomatis, tapi matinya **induk** mematikan seluruh pool (`502`). DevKu harus memantau PID induk.
4. **Port 80 bebas admin.** Windows tidak punya konsep privileged port. Risiko tersisa hanya reservasi `http.sys` (IIS). Terverifikasi: Caddy melayani `:80` tanpa elevasi.

### Peta port

| Port | Layanan | Bind | Catatan |
|---|---|---|---|
| 80 / 443 | Caddy | semua interface | tanpa admin di Windows (terverifikasi) |
| 2019 | Caddy admin API | loopback | untuk reload config tanpa restart |
| 8080 | Apache | loopback | hanya saat backend apache aktif |
| 9001+ | php-cgi per versi | loopback | satu port per versi, pool via `PHP_FCGI_CHILDREN` |
| 3306 | MariaDB | loopback | port bisa diubah di `config.json`, sengaja tidak digeser otomatis |

### Mengapa Caddy, bukan Nginx/Apache sebagai default

- Satu binary, tanpa DLL.
- `php_fastcgi` sudah mengandung `try_files` + front controller — persis kebutuhan Laravel/CI.
- `tls internal` memberi HTTPS lokal tanpa setup manual.
- Admin API memungkinkan reload per-site tanpa restart global.

### Mengapa PHP bukan FPM

PHP-FPM tidak tersedia untuk Windows. Yang dipakai `php-cgi.exe -b 127.0.0.1:PORT` mode FastCGI. Karena Windows tidak mendukung `fork`, process pooling harus disediakan pemanggil:

- **Caddy**: DevKu spawn dan kelola sendiri sejumlah `php-cgi` per versi.
- **Apache** (v2): `mod_fcgid` mengelola pool secara native — ini alasan tambahan Apache tetap relevan sebagai opsi.

---

## 4. Keputusan teknis

| Area | Keputusan | Alasan |
|---|---|---|
| Web server default | Caddy 2 | Lihat §3 |
| PHP runtime | `php-cgi.exe` FastCGI | Satu-satunya opsi nyata di Windows |
| GUI | Tauri 2 (Rust + Web) | ~5 MB exe, pakai WebView2 bawaan. Bukan Electron |
| Bahasa logika | Rust | Proses management, file watcher, Win32 API untuk hosts |
| Config global | `%APPDATA%\DevKu\config.json` | Satu sumber kebenaran, format familiar |
| Config per-site | `.devku.json` opsional di root project | Override yang bisa di-commit ke repo |
| TLD default | `localhost` | Resolve otomatis di Windows 11, tanpa hosts & UAC |
| Hosts editing | Hanya untuk TLD custom | Fallback opsional, bukan jalur utama |
| Runner JS/TS | Bun (sistem → bundel → download) | Lihat §7 |
| Supervisor proses | `process-wrap` 10 + tokio | Job Object di Windows / process group di Unix, tanpa proses tambahan |
| Klien admin API | HTTP manual di `TcpStream` | Loopback saja; hemat ~1.5 MB dibanding `reqwest` |
| Validasi config | `caddy validate` saat start, `/load` saat hidup | Satu pesan galat jelas; file tidak pernah berisi config yang ditolak |
| Auto-update | Tauri updater + signature | Distribusi tanpa installer ulang |
| Package manager | **Bun** (fallback Node/npm) | Lihat §4.1 |

### 4.1 Toolchain development

DevKu sendiri dikembangkan dengan **Bun** sebagai package manager frontend/tooling, dan **cargo** untuk sisi Rust.

| Tugas | Perintah |
|---|---|
| Install dependency | `bun install` |
| Tambah paket | `bun add <pkg>` / `bun add -d <pkg>` |
| Jalankan script | `bun run <script>` |
| Jalankan binary | `bunx <tool>` |
| Lockfile | `bun.lock` (commit), bukan `package-lock.json` |

**Aturan fallback:** Bun adalah default, bukan preferensi. `npm` hanya dipakai bila Bun benar-benar gagal — resolver tidak menyelesaikan versi, postinstall butuh npm, atau paket native tidak mendukung Bun. Setiap fallback harus disebutkan alasannya, tidak diam-diam.

Catatan: ini package manager untuk *mengembangkan* DevKu. Bun yang dibundel/dideteksi di §7 adalah fitur produk — dua hal berbeda.

### 4.2 Supervisor proses

Tidak ada supervisor eksternal. `php-cgi` dan `caddy` diawasi oleh loop di dalam binary DevKu sendiri, karena setiap alternatif (pm2, process-compose, supervisord, nssm) menambah proses, binary, dan file config yang membuat installer membengkak serta state service lepas dari UI.

| Kebutuhan | Implementasi |
|---|---|
| Proses group | `process-wrap` — `JobObject` di Windows, `ProcessGroup::leader()` di Unix |
| Anti-yatim | Job Object dengan kill-on-close + `KillOnDrop` |
| Readiness | TCP connect ke port, bukan cek PID |
| Restart | Backoff 200ms → 400 → 800 … mentok 5s |
| Crash-loop | > 5 restart dalam 60 detik → `Failed`, berhenti mencoba |
| State ke UI | Callback `(nama, ServiceState)` — nanti diteruskan sebagai Tauri event |

Batasan yang disadari: backend Apache (Fase 5) **tidak** memakai supervisor ini untuk PHP — `mod_fcgid` sudah melakukannya. Supervisor di sana hanya mengawasi `httpd.exe`.

### Format config global

```json
{
  "version": 1,
  "backend": "caddy",
  "tld": "localhost",
  "runtimes": {
    "bun":  { "source": "system", "path": "C:/Users/x/.bun/bin/bun.exe" },
    "node": { "source": "system", "path": "C:/Program Files/nodejs/node.exe" }
  },
  "php": {
    "active": "8.3.36"
  },
  "sites": [
    {
      "id": "app-a",
      "name": "App A",
      "path": "D:/projects/app-a/public",
      "domain": "app-a.localhost",
      "php": "8.3.14",
      "framework": "laravel",
      "backend": "caddy",
      "enabled": true
    }
  ]
}
```

Alokasi port tidak disimpan permanen — dihitung ulang saat start dari daftar versi terurut. Ini mencegah port bentrok setelah user menghapus site.

### Struktur folder aplikasi

```
DevKu/
├── devku.exe                 # Tauri binary
├── config/                   # generated
│   ├── Caddyfile
│   └── caddy/                # cert, autosave
├── engines/
│   ├── caddy/caddy.exe
│   ├── php/8.3.14/           # php-cgi.exe + ext
│   ├── php/7.4.33/
│   └── bun/bun.exe           # opsional, hanya jika tidak ada di sistem
├── runtime/                  # pid, log
│   ├── pids/
│   └── logs/
└── sites/                    # default root situs baru
```

### Konfigurasi Caddy yang di-generate

```caddyfile
{
    admin localhost:2019
    local_certs
}

(log_common) {
    log {
        output file "{env.DEVKU_LOG_DIR}/{args[0]}.log"
    }
}

app-a.localhost {
    import log_common app-a
    root * D:/projects/app-a/public
    encode gzip
    php_fastcgi 127.0.0.1:9001
    file_server
}
```

Satu blok per site. Reload lewat `POST /load` ke admin API — tidak perlu restart Caddy, tidak mengganggu site lain.

---

## 5. Fase detail

### Fase 0 — Fondasi ✅

**Tujuan:** hilangkan risiko teknis terbesar sebelum menulis UI.

Deliverable:
- [x] Scaffold `bun create tauri-app` + Rust workspace. Semua dependency JS lewat Bun (`bun install`, `bun add`).
- [x] **Spike A:** `php-cgi -b 127.0.0.1:9001` + Caddy `php_fastcgi` → front controller bekerja, `/users/123` masuk ke `index.php`.
- [x] **Spike B:** PHP 8.3.36 dan 7.4.33 paralel di dua domain, terisolasi, reload Caddy tanpa downtime.
- [x] **Spike C:** `PHP_FCGI_CHILDREN` **bekerja** di Windows (asumsi awal salah). 4 anak → 2 request paralel 2091 ms vs 4193 ms serial. Anak respawn otomatis; induk mati = pool mati.
- [x] **Spike D:** hosts butuh elevasi — tapi `*.localhost` resolve tanpa hosts di Windows 11, jadi jalur utama tidak perlu elevasi sama sekali.

Hasil lengkap: [`docs/spike-fase-0.md`](docs/spike-fase-0.md).

### Fase 1 — Core services ✅

Deliverable:
- [x] `WebServerBackend` trait — `render`, `spec`, `reload`, `healthy`. Core tidak tahu backend mana yang aktif.
- [x] `CaddyBackend`: generator Caddyfile per site + reload lewat admin API (`POST /load`) + manajemen proses.
- [x] `PhpManager`: ekstrak zip PHP, daftar versi, bootstrap `php.ini`, spec `php-cgi -b` dengan `PHP_FCGI_CHILDREN`.
- [x] `PhpSupervisor` (generik di `proc.rs`): pantau proses, restart + backoff, crash-loop breaker, readiness via TCP.
- [x] `HostsManager`: blok bertanda `# devku:begin` / `# devku:end`. **Hanya dipakai bila TLD ≠ `localhost`.**
- [x] `PortAllocator`: port bebas mulai 9001, deterministik per versi.
- [x] CLI internal `devku-cli` — `up`, `status`, `sites`, `check`, `install-php`, `install-caddy`, `add-site`.

Selesai bila: bisa menyalakan Caddy + 2 versi PHP dari CLI internal, dan `curl http://app-a.localhost` mengembalikan response PHP yang benar.

**Terverifikasi:** 18 unit test lulus, dua situs dengan PHP 8.3.36 & 7.4.33 di port 80 tanpa hosts/admin, induk `php-cgi` dibunuh → pulih otomatis, `up` berhenti → 0 proses tersisa. Detail: [`docs/fase-1.md`](docs/fase-1.md).

`Log viewer` digeser ke Fase 2 karena butuh UI; log sudah ditulis per service ke `runtime/logs/`.

### Fase 2 — Project manager ✅

Deliverable:
- [x] UI daftar site: nama, domain, versi PHP, framework, status.
- [x] Wizard tambah site: pilih folder → deteksi framework → pilih versi PHP → assign domain.
- [x] Autodetect framework: cari `artisan` (Laravel), `spark` (CI4), `system/` + `index.php` (CI3), fallback PHP polos.
- [x] Autodetect document root: `public/` untuk Laravel & CI4, root untuk CI3.
- [x] Hapus site: hapus vhost + hosts entry, **jangan** sentuh folder project.
- [x] Log viewer per site, diringkas di frontend (`src/log-format.ts`).
- [ ] Folder watcher: menambah folder di root `sites/` otomatis menawarkan pembuatan site. **Ditunda** ke backlog; belum ada pemantau folder.

Selesai bila: user bisa membuat site Laravel baru dari nol dan membukanya di browser tanpa menyentuh config. Terpenuhi lewat wizard, tanpa watcher.

### Fase 3 — Runtime integration ✅

Deliverable:
- `RuntimeDetector`: `where bun`, `where node`, `where php`; catat sumber (system/bundled) ke config.
- Fallback berlapis Bun: sistem → folder `engines/bun` → unduh dari GitHub lewat aplikasi.
- Terminal panel: shell dengan `PATH` yang sudah diperkaya, `cd` otomatis ke root site aktif.
- Script runner: tombol untuk `artisan migrate`, `artisan serve`, `composer install`, dan skrip `package.json`.
- `bun` sebagai default runner TS: `bun <file>.ts` langsung dari panel situs.
- Indikator runtime di UI: "Bun 1.2.3 (sistem)" vs "(bawaan)".
- [x] Installer Composer: unduh `composer.phar` resmi, verifikasi SHA-256, jalankan `--version`, lalu tulis shim `bin/composer.bat`.
- [x] PATH Composer global: baris status plus tombol daftarkan/lepas folder tool global (`laravel`, `carbon`).

Selesai bila: tidak ada binary ganda saat Bun/Node sudah terpasang, dan TS bisa dijalankan dari UI tanpa langkah setup.

**Terverifikasi:** 76 unit test lulus; unduhan sungguhan untuk PHP 8.3 (SHA-256), Caddy 2.11.7 (SHA-512 dari `checksums.txt`, lalu `caddy version` dijalankan), Bun 1.4.4 (`bun --version` dibandingkan dengan tag rilis), dan Composer 2.11.1; pendaftaran `php` ke PATH pengguna diuji naik-turun dengan PATH kembali identik. Detail: [`docs/fase-3.md`](docs/fase-3.md).

### Fase 4 — Polish ✅

Deliverable:
- [x] Tray icon: start/stop semua, buka browser, keluar (`3fcd348`).
- [x] Tutup jendela menyembunyikan ke tray; keluar dari tray menghentikan service (`3fcd348`).
- [x] SSL trust: `tls internal` per situs, `caddy trust` sekali, status ditampilkan (`ba46c69`).
- [x] Onboarding first-run: deteksi runtime, tawaran pasang Caddy, pilih PHP, lalu situs pertama. Status langkah dibaca dari keadaan nyata, bukan progres tersimpan.
- [x] Export/import `config.json`. Impor melewati folder yang tidak ada, menonaktifkan situs dengan PHP belum terpasang, dan tidak menyalin status trust antar mesin.
- [x] Auto-start saat login: nilai `HKCU\...\Run` berisi `"devku.exe" --hidden`; dibuka tanpa jendela, service langsung menyala. Bisa dilepas lagi dari UI atau `devku-cli autostart off`.

Selesai bila: DevKu bisa dipakai harian tanpa membuka jendela, dan environment bisa dibagikan lewat berkas config.

**Dipindah ke Fase 6:** installer NSIS dan auto-update. Keduanya butuh keputusan distribusi (kunci tanda tangan, endpoint rilis, host berkas) yang belum ada.

### Fase 5 — Pluggable backend (v2)

Prasyarat: Fase 1–4 stabil dan `WebServerBackend` sudah dipakai di semua jalur.

Deliverable:
- `ApacheBackend`: generator `httpd.conf` + `VirtualHost` per site, `FcgidWrapper` per versi PHP.
- Caddy sebagai edge untuk backend Apache: `reverse_proxy 127.0.0.1:8080` + header `X-Forwarded-Proto` dan `X-Forwarded-For`.
- **Lazy spawn:** Apache hanya start bila ≥1 site ber-`backend: "apache"`. Matikan otomatis saat site terakhir Apache dimatikan.
- Template `TrustProxies` untuk Laravel + panduan setelan CI agar HTTPS terdeteksi.
- Pilihan backend di wizard site (bukan hanya global setting).
- Migrasi site antar backend tanpa mengubah folder.

Selesai bila: satu site `backend: apache` dengan `.htaccess` custom dan satu site `backend: caddy` berjalan bersamaan di port 80, dan Apache mati total saat site Apache dinonaktifkan.

### Fase 6 — Distribusi (sebagian selesai)

Installer NSIS sudah dikonfigurasi. Auto-update menunggu keputusan rilis: kunci tanda tangan, endpoint pembaruan, dan tempat menyimpan berkas.

Deliverable:
- [x] Installer NSIS: `bundle.targets: ["nsis"]`, instalasi per-machine dengan default `C:\devku` dan UAC. Tanpa PHP binary di paket. PHP diunduh saat versi pertama dipilih di UI.
- Auto-update via Tauri updater: `tauri-plugin-updater`, kunci publik di config, endpoint rilis, dan tampilan versi di UI. Ditunda.
- Halaman rilis berisi hash dan catatan perubahan, supaya unduhan bisa diverifikasi.

Selesai bila: installer jalan di mesin Windows bersih tanpa langkah manual, dan versi lama bisa memperbarui diri lewat tanda tangan yang valid.

---

### Fase 7 — Database lokal (selesai, MariaDB)

MariaDB 11.4.13 LTS dipatok di kode, diunduh dari `archive.mariadb.org`, dan diverifikasi lewat `sha256sums.txt`. Binari di `engines/mariadb`, data di `databases/mariadb`, sehingga memasang ulang binari tidak menyentuh database.

Deliverable:
- [x] Panel Database di dasbor: pasang, mulai, stop, autostart, atur password root, hapus.
- [x] Password root disimpan di `runtime/mariadb/root.pass`, terpisah dari `config.json` supaya tidak ikut ter-ekspor.
- [x] Server berjalan lewat supervisor yang sama dengan Caddy dan php-cgi, jadi ikut mati saat DevKu keluar.
- [x] Hapus dengan ceklis data dan backup `.sql` otomatis ke `backups/`.
- [x] `mariadb` dan `mariadb-dump` tersedia di terminal DevKu.
- PostgreSQL: implementasi lain dari trait `DatabaseBackend`, belum dikerjakan.

Selesai bila: situs bisa memakai `DB_HOST=127.0.0.1` dengan port dari config tanpa langkah manual.

---

## 6. Risiko & mitigasi

| Risiko | Dampak | Mitigasi |
|---|---|---|
| Port 80 dipegang IIS/`http.sys` | Caddy gagal bind | Cek saat start, tampilkan proses penahan, tawarkan port alternatif |
| Induk `php-cgi` mati | Seluruh pool mati, `502` | `PhpSupervisor` pantau PID induk, restart + backoff, tampilkan status di UI |
| Konkurensi `php-cgi` | Request antre di luar ukuran pool | `PHP_FCGI_CHILDREN` per versi (default 4), bisa diatur per site |
| TLD custom butuh hosts | Elevasi/UAC | Default `localhost` (tanpa hosts). Elevasi hanya saat user memilih TLD lain |
| Windows 10 tidak resolve `*.localhost` | Site tidak terbuka | Deteksi saat first-run; tawarkan hosts otomatis atau TLD `test` |
| Windows Defender flag binary | User ragu | Code signing cert; dokumentasi hash rilis |
| Caddy belum populer di komunitas PHP Windows | Kurang tutorial | Sediakan "lihat Caddyfile" di UI + preset framework |
| `.htaccess` custom tidak jalan di Caddy | Migrasi gagal | Fase 5 Apache backend; deteksi `.htaccess` non-standar dan sarankan Apache |
| PHP extension butuh DLL tambahan | Error runtime | Sertakan `ext/` lengkap; preset `php.ini` per framework |

---

## 7. Non-goals (v1)

- Bukan pengelola dependency PHP: DevKu bisa memasang Composer, tetapi `composer.json`, lockfile, dan paket tetap urusan Composer.
- Tidak mengelola database server (MySQL/Postgres). Skema integrasi bisa masuk v2 lewat `docker compose` opsional.
- Tanpa dukungan macOS/Linux di v1. Arsitektur tidak menutup kemungkinan, tapi backend dan hosts handling berbeda.
- Tidak mengemulasi `.htaccess` di Caddy. Itu jalan pintas yang rapuh — Apache backend adalah jawaban yang benar.
- Tidak membundel PHP di installer. Unduh on-demand agar installer tetap kecil.

---

## 8. Urutan eksekusi

```
Fase 0 ──► Fase 1 ──► Fase 2 ──► Fase 3 ──► Fase 4 ──► Fase 5
 spike      core       UI site     runtime     polish     apache
   ▲           ▲
   │           └── v1 bisa dipakai (dogfood)
   └── gate: semua spike lulus sebelum lanjut
```

Fase 2 selesai = sudah bisa dipakai sendiri untuk kerja harian. Mulai dari situ, feedback nyata mengoreksi prioritas Fase 3–4.

Fase 6 (distribusi) menyusul kapan saja setelah Fase 4, tergantung kesiapan kunci tanda tangan dan host rilis. Fase 5 tidak menunggunya.
