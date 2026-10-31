# Rencana: satu binary `devku`

Status: rencana, belum dikerjakan. Dibuat 2026-10-31.

## Tujuan

Menggabungkan `devku` dan `devku-cli` menjadi satu berkas `devku.exe`:

- Tanpa argumen: membuka UI seperti sekarang.
- Dengan subperintah: menjalankan mode headless, misalnya `devku status`, `devku up`, `devku install-php`.
- Dengan `--hidden`: membuka aplikasi tanpa jendela untuk autostart login.

Hasil akhir: installer hanya membawa satu binary, dan pengguna hasil instalasi tetap punya akses CLI.

## Keputusan yang sudah ditetapkan

1. **Routing di `main.rs`, sebelum `devku_lib::run()`.** Jalur CLI tidak boleh menginisialisasi WebView atau tray.
2. **Subsistem Windows tetap `windows`** di build release, bukan `console`. Kalau memakai subsistem konsol, jendela hitam berkedip setiap DevKu dibuka dari Explorer.
3. **Konsol dipasang saat mode CLI di build release** lewat `AttachConsole(ATTACH_PARENT_PROCESS)`. Di build debug tidak perlu, karena `windows_subsystem` hanya aktif saat `not(debug_assertions)`.
4. **`--hidden` tetap berarti GUI tanpa jendela.** Argumen ini bukan subperintah CLI.
5. **Semua subperintah lama dipertahankan namanya.** `devku-cli up` menjadi `devku up`, tanpa perubahan opsi.
6. **Exit code**: 0 sukses, 1 galat, 2 perintah tidak dikenal. Sekarang perintah tidak dikenal memakai 1; pemisahan ini supaya skrip bisa membedakan salah ketik dari kegagalan nyata.
7. **Tidak ada alias `devku-cli.exe`.** Binary kedua dihapus, dan perubahan ini dicatat di changelog sebagai breaking change untuk skrip lama.

## Rancangan

### Struktur berkas

```
src-tauri/src/
├── main.rs        routing: argumen -> GUI atau CLI
├── console.rs     (baru) pasang konsol di release Windows
├── cli.rs         (baru) isi devku-cli.rs yang dipindah
├── lib.rs         tambah pub mod cli; pub mod console;
└── bin/
    └── devku-cli.rs  dihapus
```

`tauri.conf.json` tidak berubah. Argumen `--hidden` masih dibaca di `lib.rs` lewat `autostart::HIDDEN_FLAG`.

### Routing argumen

| Argumen pertama | Perilaku |
|---|---|
| tidak ada | `devku_lib::run()`, UI tampil |
| `--hidden` | `devku_lib::run()`, UI disembunyikan, service menyala |
| `-h`, `--help`, `help` | teks bantuan, exit 0 |
| `status`, `sites`, `up`, `install-php`, `install-caddy`, `add-site`, `php-path`, `runtimes`, `install-composer`, `composer-path`, `autostart`, `export-config`, `import-config`, `trust` | mode CLI |
| lainnya | teks bantuan ke stderr, exit 2 |

Kerangka `main.rs`:

```rust
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        None | Some(devku_lib::autostart::HIDDEN_FLAG) => {
            devku_lib::run();
            std::process::ExitCode::SUCCESS
        }
        Some("-h" | "--help" | "help") => {
            devku_lib::cli::print_help();
            std::process::ExitCode::SUCCESS
        }
        Some(_) => {
            devku_lib::console::attach_for_cli();
            devku_lib::cli::run(&args)
        }
    }
}
```

Catatan: `tauri::run()` tidak kembali sampai aplikasi keluar, jadi `ExitCode` hanya relevan untuk jalur CLI dan bantuan.

### Modul CLI

- Pindahkan seluruh isi `src-tauri/src/bin/devku-cli.rs` ke `src-tauri/src/cli.rs`.
- Ganti `#[tokio::main] async fn main()` menjadi `pub fn run(args: &[String]) -> ExitCode` yang membuat runtime sendiri:

```rust
pub fn run(args: &[String]) -> std::process::ExitCode {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build();

    let Ok(runtime) = runtime else {
        eprintln!("DevKu: gagal menyiapkan runtime");
        return std::process::ExitCode::FAILURE;
    };

    match runtime.block_on(dispatch(args)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("DevKu: {err:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
```

- `dispatch` adalah isi `main` yang sekarang, tanpa argumen program.
- `print_help()` memakai nama `devku`, bukan `devku-cli`.
- Fungsi privat lain (`status`, `up`, `export_config`, dan seterusnya) tidak berubah isinya.
- `cli.rs` tetap boleh memakai `devku_lib::` atau `crate::` karena sekarang bagian dari crate yang sama.

### Konsol Windows

`src-tauri/src/console.rs`:

```rust
#[cfg(all(windows, not(debug_assertions)))]
pub fn attach_for_cli() {
    use std::io::IsTerminal;
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};

    // Kalau keluaran sudah dialihkan ke pipe atau berkas, jangan pasang konsol:
    // handle yang ada sudah benar dan tidak boleh diganti.
    if !std::io::stdout().is_terminal() {
        return;
    }

    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(all(windows, not(debug_assertions))))]
pub fn attach_for_cli() {}
```

Tambahan yang perlu diperiksa saat implementasi:

- Setelah `AttachConsole`, handle `stdout`/`stderr` Rust belum tentu menunjuk konsol induk. Kalau keluaran tetap tidak muncul, buka `CONOUT$` dengan `OpenOptions` lalu pasang lewat `SetStdHandle(STD_OUTPUT_HANDLE, handle)`, dan lakukan hal yang sama untuk `CONIN$` dan `STD_INPUT_HANDLE`.
- Tambah feature `Win32_System_Console` pada dependensi `windows` di `Cargo.toml`.
- Jangan panggil `AttachConsole` di jalur GUI. Jendela konsol yang menempel membuat tray dan peluncuran dari Explorer berperilaku aneh.

### `Cargo.toml`

- `default-run = "devku"` boleh dihapus setelah hanya tersisa satu binary.
- Tidak ada `[[bin]]` eksplisit, jadi menghapus `src/bin/devku-cli.rs` otomatis menghilangkan target itu.
- Jangan lupa `cargo clean -p devku` atau hapus `target/debug/devku-cli.exe` lama supaya pengujian tidak memakai binary basi.

## Langkah implementasi

1. Pindahkan `src/bin/devku-cli.rs` ke `src/cli.rs`, ubah titik masuk menjadi `run` dan `dispatch`, ganti nama di teks bantuan.
2. Tambah `pub mod cli;` dan `pub mod console;` di `lib.rs`.
3. Buat `console.rs` dan tambah feature `Win32_System_Console`.
4. Tulis routing di `main.rs` sesuai tabel, termasuk exit code 2 untuk perintah tidak dikenal.
5. Hapus `src/bin/devku-cli.rs` dan `default-run` dari `Cargo.toml`.
6. Perbarui semua rujukan `devku-cli` di dokumentasi: `README.md`, `AGENTS.md`, `PLAN.md`, `CHANGELOG.md`, dan komentar yang menyebut dua binary.
7. Jalankan pengujian di bawah, lalu ubah `CHANGELOG.md` dengan hash commit.

## Pengujian

Build debug:

```powershell
cargo run --manifest-path src-tauri\Cargo.toml -- --help
cargo run --manifest-path src-tauri\Cargo.toml -- status
cargo run --manifest-path src-tauri\Cargo.toml -- up --seconds 2
cargo run --manifest-path src-tauri\Cargo.toml
```

Build release (bagian yang paling penting):

```powershell
cargo build --manifest-path src-tauri\Cargo.toml --release
$exe = "src-tauri\target\release\devku.exe"

& $exe --help
& $exe status
& $exe sites
& $exe up --seconds 2
& $exe perintah-ngawur; $LASTEXITCODE   # harus 2
& $exe status | Out-String               # keluaran harus tertangkap pipe
```

Mode GUI:

- `devku.exe` tanpa argumen menampilkan jendela.
- `devku.exe --hidden` tidak menampilkan jendela, tray aktif, dan service menyala. Verifikasi dengan `EnumWindows` + `IsWindowVisible` seperti pengujian autostart sebelumnya.
- `bun run tauri dev` tetap jalan setelah `default-run` dihapus.

Pemeriksaan lain:

- `cargo test --manifest-path src-tauri\Cargo.toml --lib` tetap 80 test lulus.
- `cargo clippy --all-targets` dan `bun run check` bersih.
- `Get-Item target\release\devku.exe` memastikan hanya satu binary yang diproduksi.

## Risiko dan mitigasi

| Risiko | Dampak | Mitigasi |
|---|---|---|
| Release tidak mencetak apa pun | CLI tidak terpakai | `AttachConsole` + `SetStdHandle`, uji manual di PowerShell dan cmd sebelum merge |
| Output ganda atau tidak terpotong di pipe | Skrip rusak | Jangan attach kalau `stdout.is_terminal()` false; uji `| Out-String` dan `> berkas.txt` |
| Jendela konsol muncul saat GUI dibuka | jelek, terlihat seperti bug | tetap subsistem `windows`; `attach_for_cli` hanya dipanggil di jalur CLI |
| `tauri dev` bingung setelah binary kedua dihapus | alur dev terhenti | hapus `default-run`, jalankan `bun run tauri dev` sebelum commit |
| Exit code hilang | otomasi salah membaca hasil | `main` mengembalikan `ExitCode`, bukan `()` |
| Binary lama tertinggal di `target/` | pengujian memakai versi basi | hapus `devku-cli.exe` sebelum menguji |
| Skrip pengguna memanggil `devku-cli` | breaking | catat di changelog; sediakan satu versi transisi bila diminta |

## Rollback

Tidak ada perubahan config, skema, atau data pengguna. Rollback cukup mengembalikan commit: `git revert <hash>`. Setelah revert, `devku-cli` kembali menjadi target binary terpisah.

## Definisi selesai

- [ ] Satu berkas `devku.exe` di `target/release`, tanpa `devku-cli.exe`.
- [ ] Tanpa argumen membuka UI; `--hidden` membuka tanpa jendela dan menyalakan service.
- [ ] Semua subperintah lama berjalan dengan nama `devku <perintah>`, termasuk `export-config` dan `install-composer`.
- [ ] Keluaran terlihat di PowerShell, cmd, pipe, dan redirect berkas pada build release.
- [ ] Exit code 0, 1, dan 2 sesuai tabel.
- [ ] `cargo test`, clippy, `bun run check`, dan `tauri dev` lulus.
- [ ] Dokumentasi dan `CHANGELOG.md` tidak lagi menyebut dua binary.

## Di luar cakupan

- Lock single-instance supaya `up` menolak jalan saat GUI aktif.
- Auto-reload config di GUI lewat file watcher.
- Sidecar atau alias `devku-cli.exe` untuk kompatibilitas.
- Perubahan nama atau opsi subperintah.
