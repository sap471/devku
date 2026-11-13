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

/// Same as `run_hidden`, but feeds a file into the child's stdin. Used to
/// restore a dump: the client reads the SQL stream itself, so no shell
/// redirection is needed and the file never has to be quoted.
pub fn run_hidden_with_stdin(
    program: &std::path::Path,
    args: &[String],
    cwd: Option<&std::path::Path>,
    mut input: std::fs::File,
) -> anyhow::Result<std::process::Output> {
    let mut command = std::process::Command::new(program);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    command.stdin(std::process::Stdio::piped());
    hide_console_std(&mut command);

    let mut child = command
        .spawn()
        .map_err(|err| anyhow::anyhow!("gagal menjalankan {}: {err}", program.display()))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("stdin {} tidak tersedia", program.display()))?;
    let written = std::io::copy(&mut input, &mut stdin)?;
    drop(stdin);

    let output = child
        .wait_with_output()
        .map_err(|err| anyhow::anyhow!("gagal menunggu {}: {err}", program.display()))?;

    // A client that exits before reading everything leaves the pipe broken;
    // that is a real failure and must not be reported as success.
    if written == 0 && input.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
        anyhow::bail!("{} tidak membaca input", program.display());
    }

    Ok(output)
}

/// Same as `run_hidden`, with extra environment variables. PostgreSQL's client
/// tools read the password from `PGPASSWORD`, so they need env the way the
/// MySQL family never does.
pub fn run_hidden_env(
    program: &std::path::Path,
    args: &[String],
    cwd: Option<&std::path::Path>,
    env: &[(String, String)],
) -> anyhow::Result<std::process::Output> {
    let mut command = std::process::Command::new(program);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    for (key, value) in env {
        command.env(key, value);
    }
    command.stdin(std::process::Stdio::null());
    hide_console_std(&mut command);

    command
        .output()
        .map_err(|err| anyhow::anyhow!("gagal menjalankan {}: {err}", program.display()))
}
