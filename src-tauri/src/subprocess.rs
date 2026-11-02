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

/// Runs a command once with no console window and returns
/// its output. Used by database tools whose output has to be read.
pub fn run_hidden(
    program: &std::path::Path,
    args: &[String],
    cwd: Option<&std::path::Path>,
) -> anyhow::Result<std::process::Output> {
    let mut command = std::process::Command::new(program);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    command.stdin(std::process::Stdio::null());
    hide_console_std(&mut command);

    command
        .output()
        .map_err(|err| anyhow::anyhow!("gagal menjalankan {}: {err}", program.display()))
}
