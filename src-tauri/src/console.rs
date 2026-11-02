//! Console support for CLI mode in release builds.
//!
//! Release builds use the `windows` subsystem so opening DevKu from
//! Explorer does not flash a black window. The tradeoff is that the process
//! does not inherit a console, so the CLI path must borrow the parent console first.

#[cfg(all(windows, not(debug_assertions)))]
use windows::Win32::Foundation::HANDLE;
#[cfg(all(windows, not(debug_assertions)))]
use windows::Win32::System::Console::STD_HANDLE;

#[cfg(all(windows, not(debug_assertions)))]
pub fn attach_for_cli() {
    use windows::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };

    // Save redirected handles before AttachConsole: that call replaces the
    // process std handles with console handles, including ones that used to
    // point at a file from `>` or a pipe from `|`.
    let saved = [
        (STD_OUTPUT_HANDLE, redirect_handle(STD_OUTPUT_HANDLE)),
        (STD_ERROR_HANDLE, redirect_handle(STD_ERROR_HANDLE)),
    ];

    // Failure here is normal when the process already owns a console; output is
    // still attempted through CONOUT$ below.
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

/// Handle a script already pointed at a pipe or file, if any. Console
/// handles and empty handles are not worth keeping.
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
    // The handle must stay alive while the process writes; std re-fetches it via
    // GetStdHandle on every print.
    std::mem::forget(console);
}

#[cfg(not(all(windows, not(debug_assertions))))]
pub fn attach_for_cli() {}
