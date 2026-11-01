//! Konsol untuk mode CLI di build release.
//!
//! Build release memakai subsistem `windows` supaya membuka DevKu dari
//! Explorer tidak memunculkan jendela hitam. Konsekuensinya proses tidak
//! mewarisi konsol, jadi jalur CLI harus meminjam konsol induk lebih dulu.

#[cfg(all(windows, not(debug_assertions)))]
use windows::Win32::Foundation::HANDLE;
#[cfg(all(windows, not(debug_assertions)))]
use windows::Win32::System::Console::STD_HANDLE;

#[cfg(all(windows, not(debug_assertions)))]
pub fn attach_for_cli() {
    use windows::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };

    // Simpan handle hasil redirect sebelum AttachConsole: pemanggilan itu
    // mengganti std handle proses dengan handle konsol, termasuk yang tadinya
    // menunjuk berkas dari `>` atau pipe dari `|`.
    let saved = [
        (STD_OUTPUT_HANDLE, redirect_handle(STD_OUTPUT_HANDLE)),
        (STD_ERROR_HANDLE, redirect_handle(STD_ERROR_HANDLE)),
    ];

    // Gagal di sini normal kalau proses sudah punya konsol; keluaran tetap
    // dicoba lewat CONOUT$ di bawah.
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }

    for (std_handle, previous) in saved {
        match previous {
            Some(handle) => unsafe {
                let _ = SetStdHandle(std_handle, handle);
            },
            None => bind_console(std_handle),
        }
    }
}

/// Handle yang sudah diarahkan skrip ke pipe atau berkas, kalau ada. Handle
/// konsol atau handle kosong tidak perlu dipertahankan.
#[cfg(all(windows, not(debug_assertions)))]
fn redirect_handle(std_handle: STD_HANDLE) -> Option<HANDLE> {
    use windows::Win32::Storage::FileSystem::{FILE_TYPE_DISK, FILE_TYPE_PIPE, GetFileType};
    use windows::Win32::System::Console::GetStdHandle;

    let handle = unsafe { GetStdHandle(std_handle) }.ok()?;
    let kind = unsafe { GetFileType(handle) };
    (kind == FILE_TYPE_PIPE || kind == FILE_TYPE_DISK).then_some(handle)
}

#[cfg(all(windows, not(debug_assertions)))]
fn bind_console(std_handle: STD_HANDLE) {
    use std::fs::OpenOptions;
    use std::os::windows::io::AsRawHandle;

    use windows::Win32::System::Console::SetStdHandle;

    let Ok(console) = OpenOptions::new().write(true).open("CONOUT$") else {
        return;
    };
    let handle = HANDLE(console.as_raw_handle());
    unsafe {
        let _ = SetStdHandle(std_handle, handle);
    }
    // Handle harus hidup selama proses menulis; std mengambilnya ulang lewat
    // GetStdHandle pada setiap cetakan.
    std::mem::forget(console);
}

#[cfg(not(all(windows, not(debug_assertions))))]
pub fn attach_for_cli() {}
