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
