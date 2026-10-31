# Changelog

Semua perubahan yang terlihat oleh pengguna dicatat di sini. Format mengikuti
[Keep a Changelog](https://keepachangelog.com/id/1.1.0/), versi mengikuti
[SemVer](https://semver.org/lang/id/).

## [Unreleased]

### Ditambahkan

- Installer Windows NSIS per-machine dengan folder default `C:\devku`. Runtime
  dan config DevKu tetap berada di `%APPDATA%\DevKu`. Installer meminta UAC.
  (`6cc4912`)
- Menu **Buka** di panel situs: Explorer, Terminal, VS Code, Zed, dan salin path.
  Aplikasi yang tidak terpasang tampil nonaktif, bukan hilang tanpa penjelasan.
  (`6cc4912`)
- Tombol **Buka folder** untuk folder data DevKu di bagian Konfigurasi dasbor.
  (`6cc4912`)

- Fondasi proyek: scaffold Tauri 2, spike arsitektur multi-PHP per domain, Caddy
  sebagai edge, dan TLD `localhost` tanpa entri hosts. (`e35dd0a`)
- Core services: trait `WebServerBackend`, backend Caddy, `PhpManager`,
  supervisor proses dengan backoff, PortAllocator, dan CLI `devku-cli`. (`ea8190e`)
- Sesi aplikasi, deteksi framework Laravel, CodeIgniter 3, CodeIgniter 4, dan PHP
  biasa, perintah Tauri, serta arah desain antislop. (`c35746d`)
- Panel kontrol situs, wizard 3 langkah, log per situs, keadaan kosong, memuat,
  dan galat, plus plugin dialog. (`9bd13d1`)
- Dasbor catch-all di port 80 dan status bar service di UI. (`770bb3a`)
- Service menyala saat aplikasi dibuka dan berhenti bersih saat aplikasi ditutup.
  (`be8a06f`)
- Deteksi runtime sistem (bun, node, php, composer) dan panel runtime. (`21896c8`)
- Terminal PTY sungguhan dengan xterm, bukan penjalan perintah sekali jalan.
  (`7f39bb4`)
- Unduh dan pasang PHP dari aplikasi, lengkap dengan verifikasi SHA-256 dan
  progres unduhan. (`28c14e6`)
- Unduh Caddy dan Bun dari aplikasi, runner perintah per situs, dan tombol
  menjalankan berkas TypeScript. (`298902f`)
- Tray icon dengan menu buka, start semua, stop semua, dan keluar. Menutup
  jendela kini hanya menyembunyikannya. (`3fcd348`)
- HTTPS lokal per situs lewat `tls internal` dan tombol Percayai yang menjalankan
  `caddy trust` tanpa dialog saat startup. (`ba46c69`)
- Installer Composer: unduh phar resmi, verifikasi SHA-256, uji `--version`, lalu
  tulis shim `composer.bat` yang memakai PHP di PATH. (`58ee919`)
- Tombol `http` dan `https` di baris domain, aktif hanya saat situs siap. Render
  log akses Caddy diringkas per permintaan dengan warna status, durasi, dan ukuran.
  (`cc4479d`)
- Baris **Composer global** di dasbor: status folder tool global, daftar tool,
  serta tombol Daftarkan dan Lepas PATH. (`f78e3c0`)
- Panduan first-run: status web server, PHP, HTTPS, dan situs pertama dibaca dari
  keadaan nyata, dengan tombol Lewati yang disimpan di config. (`3f14c8e`)
- Ekspor dan impor `config.json` dari dasbor maupun CLI. Impor melewati folder
  yang tidak ada, menonaktifkan situs dengan PHP belum terpasang, dan tidak
  menyalin status trust antar mesin. (`13a487e`)
- Auto-start saat login lewat nilai `HKCU\...\Run`: DevKu dibuka dengan flag
  `--hidden`, tanpa jendela, dan langsung menyalakan service. (`13a487e`)
- Perintah CLI baru untuk jalur headless: `php-path` mengatur PATH PHP (`d83fe3a`),
  `runtimes` mendaftar runtime yang terdeteksi (`6eaa4f9`), `install-composer`
  memasang Composer tanpa UI (`58ee919`), `composer-path` mengatur folder paket
  global (`f78e3c0`), `trust` memasang CA Caddy (`ba46c69`), serta `autostart`,
  `export-config`, dan `import-config` (`13a487e`).

### Diubah

- Config Caddy divalidasi sebelum ditulis, dan reload memakai `POST /load` supaya
  perubahan berlaku atomik tanpa restart. (`76d60bf`)
- Deteksi PHP menjadi folder-first: `PhpManager::installed()` memindai
  `engines/php/*` dan mengurutkan versi secara numerik. Katalog rilis digabung
  dengan versi lokal, dan `config.php.installed` dihapus. (`6eaa4f9`)
- Tata letak shell dibalik: daftar situs berhenti di 400px, panel info mengambil
  seluruh sisa lebar saat jendela diperbesar. (`adc080b`)
- Jendela dibuat dengan `visible: false`, lalu ditampilkan saat peluncuran biasa
  dan tetap tersembunyi saat dibuka oleh Windows. (`13a487e`)
- Dokumentasi: hasil Fase 1 dan Fase 3, laporan spike Fase 0. (`112335f`)
- PLAN disinkronkan dengan keadaan nyata, termasuk commit rujukan per butir.
  (`3f14c8e`)
- Fase 4 ditandai selesai, installer NSIS dan auto-update dipindah ke Fase 6.
  (`38ec738`)
- README memuat status fase terbaru dan tautan ke changelog ini.

### Diperbaiki

- Jendela console tidak lagi berkedip saat DevKu memeriksa versi runtime, Composer,
  atau konfigurasi Caddy. (`6cc4912`)

- `default-run = "devku"` supaya `tauri dev` tidak ambigu antara aplikasi dan CLI.
  (`3246eb8`)
- Folder tanpa penanda PHP tetap bisa dijadikan situs; deteksi framework menjadi
  petunjuk, bukan syarat. (`35aebff`)
- Navigasi kembali ke dasbor, ganti versi PHP per situs, dan pendaftaran `php` ke
  PATH pengguna. (`d83fe3a`)
- Caddy tidak lagi meminta instalasi CA saat startup, sehingga aplikasi tidak
  menggantung menunggu dialog elevasi. (`ba46c69`)
- Panduan first-run tidak muncul lagi setelah dilewati atau setelah situs pertama
  ditambahkan. (`3f14c8e`)

### Dilepas

- Field `php.installed` di config, diganti pemindaian folder `engines/php`.
  (`6eaa4f9`)

## Cara mencatat perubahan

1. Setiap commit yang mengubah perilaku menambah minimal satu baris di
   `[Unreleased]`, pada kelompok **Ditambahkan**, **Diubah**, **Diperbaiki**, atau
   **Dilepas**.
2. Sertakan hash commit pendek dalam tanda kurung, misalnya `(abc1234)`, supaya
   perubahan bisa dilacak ke riwayat git. Baris yang dibuat bersama commit-nya
   sendiri boleh tanpa hash.
   Alurnya: commit perubahan lebih dulu, salin hash pendeknya, lalu tambahkan
   baris changelog di commit terpisah.
3. Tulis dari sudut pandang pengguna: apa yang sekarang bisa dilakukan, bukan
   nama fungsi internal.
4. Bahasa Indonesia, kalimat aktif, tanpa em dash dan tanpa basa-basi.
5. Saat rilis: ganti `[Unreleased]` dengan versi dan tanggal, lalu buka bagian
   `[Unreleased]` baru di atasnya.
