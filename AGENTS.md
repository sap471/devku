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
- Rust: `cargo fmt` + `cargo clippy` sebelum commit.
- Frontend: `bun run check` untuk typecheck.
