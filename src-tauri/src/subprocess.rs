#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

pub fn hide_console_std(command: &mut std::process::Command) {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW.0);
}

pub fn hide_console_tokio(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW.0);
}
