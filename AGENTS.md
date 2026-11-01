# DevKu

Development environment untuk PHP di Windows. Lihat [`PLAN.md`](PLAN.md).

## Toolchain

| Area | Tool | Catatan |
|---|---|---|
| Package manager (JS/TS) | **Bun** | Selalu `bun`, bukan `npm`/`pnpm`/`yarn` |
| Node fallback | Node 24 + npm | Hanya jika Bun error atau paket tidak kompatibel |
| Rust | cargo | Backend Tauri |
| Frontend | Tauri 2 + WebView2 | Bukan Electron |

### Aturan package manager

- Install: `bun install` — jangan `npm install`.
- Tambah paket: `bun add <pkg>` / `bun add -d <pkg>`.
- Jalankan script: `bun run <script>`.
- Binary: `bunx <tool>`.
- Lockfile: commit `bun.lock`. Jangan commit `package-lock.json` atau `pnpm-lock.yaml`.
- **Fallback:** kalau `bun` gagal (error resolver, postinstall butuh npm, paket native tidak support Bun), baru pakai `npm`. Sebutkan alasan fallback-nya. Jangan diam-diam pindah.
- Jangan pernah menjalankan `npm install` hanya karena kebiasaan — Bun dulu.

## Konvensi

- Komentar kode: hanya yang menjelaskan *mengapa*, bukan *apa*. Tanpa komentar dekoratif.
- Config pengguna (`config.json`, `.devku.json`) selalu bisa dibaca dan di-diff.
- Setiap perubahan perilaku dicatat di `CHANGELOG.md` pada bagian `[Unreleased]`, dengan hash commit.
- Rust: `cargo fmt` + `cargo clippy` sebelum commit.
- Frontend: `bun run check` untuk typecheck.

## Kuota unduhan

Pemilik proyek memakai paket internet berkuota. Binary engine (PHP, Caddy, Bun,
Composer, MariaDB) berukuran puluhan sampai ratusan MB, jadi uji rutin tidak
boleh mengunduhnya.

- Jangan memakai `DEVKU_HOME` kosong untuk uji. Jalankan
  `.\scripts\dev\test-home.ps1`, yang membangun home uji dari cache lokal lewat
  hardlink, tanpa jaringan, dalam hitungan detik.
- Cache diisi sekali dengan `-Seed`. Perintah itu mengambil dari instalasi nyata
  di `%APPDATA%\DevKu\engines`, bukan dari internet.
- Unduhan hanya boleh pada uji akhir sebelum naik versi, dan sebutkan lebih dulu
  bahwa uji itu akan mengunduh.
- Untuk membuktikan tidak ada unduhan, jalankan uji dengan
  `HTTPS_PROXY=http://127.0.0.1:9`. Katalog PHP akan gagal, dan itu memang yang
  diharapkan.
- Home uji memakai port database 3307 supaya tidak bentrok dengan instance nyata.
  Untuk uji yang menyalakan service, hentikan dulu DevKu yang sedang berjalan
  karena port 80, 443, dan 2019 dipakai bersama, lalu nyalakan lagi setelah
  selesai.

## Antislop

UI dan teks wajib mengikuti aturan antislop. Baca skill ini sebelum menyentuh antarmuka:

- `antislop` (inti, selalu) di `~/.agents/skills/antislop/SKILL.md`
- `antislop-ui`, `antislop-copywriting`, `antislop-human`, `antislop-layoutmobile` sesuai bidang yang disentuh
- `antislop-code` saat menulis atau merapikan komentar

Arah visual proyek ada di [`DESIGN.md`](DESIGN.md). Jangan mendesain tanpa arah; kalau arah tidak ada, tandai hasilnya sebagai draft.

Aturan yang paling sering dilanggar di proyek ini:

- Teks tanpa em dash. Pakai koma, titik, titik dua, atau kurung.
- Tidak ada tombol atau tautan tanpa perilaku nyata.
- Setiap tampilan data punya keadaan kosong, memuat, galat, dan siap.
- Kontras AA wajib di mode terang dan gelap.
- Angka dan statistik harus nyata, bukan karangan.
