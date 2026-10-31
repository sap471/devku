use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::runtime_detect::find_in_path;
use crate::subprocess::hide_console_std;

/// Satu baris di menu "Buka". Target yang aplikasinya tidak terpasang tetap
/// ditampilkan tapi nonaktif, supaya user tahu kenapa tidak ada di daftar.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct OpenTarget {
    pub id: &'static str,
    pub label: &'static str,
    pub available: bool,
}

pub fn targets() -> Vec<OpenTarget> {
    vec![
        OpenTarget {
            id: "explorer",
            label: "Explorer",
            available: true,
        },
        OpenTarget {
            id: "terminal",
            label: "Terminal",
            available: true,
        },
        OpenTarget {
            id: "vscode",
            label: "VS Code",
            available: find_in_path("code").is_some(),
        },
        OpenTarget {
            id: "zed",
            label: "Zed",
            available: find_in_path("zed").is_some(),
        },
        OpenTarget {
            id: "copy",
            label: "Salin path",
            available: true,
        },
    ]
}

pub fn open(dir: &Path, target: &str) -> Result<()> {
    if !dir.exists() {
        bail!("folder {} tidak ada", dir.display());
    }

    match target {
        "explorer" => {
            let mut command = Command::new("explorer.exe");
            command.arg(dir);
            spawn_detached(&mut command)
        }
        "terminal" => open_terminal(dir),
        "vscode" => run_in_shell("code", dir, "VS Code tidak ditemukan di PATH"),
        "zed" => {
            let path = find_in_path("zed").context("Zed tidak ditemukan di PATH")?;
            let mut command = Command::new(path);
            command.arg(dir);
            hide_console_std(&mut command);
            spawn_detached(&mut command)
        }
        "copy" => copy_path(dir),
        other => bail!("target tidak dikenal: {other}"),
    }
}

/// Membuka satu berkas di editor. Urutannya VS Code, Zed, lalu Notepad, supaya
/// tombolnya tetap bekerja di mesin yang tidak punya editor terpasang.
pub fn open_file(path: &Path) -> Result<&'static str> {
    if !path.exists() {
        bail!("berkas {} tidak ada", path.display());
    }

    let (mut command, label) = if cfg!(windows) {
        if find_in_path("code").is_some() {
            let mut command = Command::new("cmd.exe");
            command.arg("/C").arg("code").arg(path);
            (command, "VS Code")
        } else if let Some(exe) = find_in_path("zed") {
            let mut command = Command::new(exe);
            command.arg(path);
            (command, "Zed")
        } else {
            let mut command = Command::new("notepad.exe");
            command.arg(path);
            (command, "Notepad")
        }
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        (command, "editor")
    };

    hide_console_std(&mut command);
    spawn_detached(&mut command)?;
    Ok(label)
}

/// `code` di Windows berupa `code.cmd`, jadi harus lewat shell.
fn run_in_shell(program: &str, dir: &Path, missing: &str) -> Result<()> {
    if find_in_path(program).is_none() {
        bail!("{missing}");
    }

    let mut command = Command::new("cmd.exe");
    command.arg("/C").arg(program).arg(dir);
    hide_console_std(&mut command);
    spawn_detached(&mut command)
}

fn open_terminal(dir: &Path) -> Result<()> {
    if let Some(terminal) = find_in_path("wt") {
        let mut command = Command::new(terminal);
        command.arg("-d").arg(dir);
        hide_console_std(&mut command);
        return spawn_detached(&mut command);
    }

    // Tanpa Windows Terminal, buka console sendiri: ini satu-satunya target
    // yang memang harus terlihat, jadi tidak memakai CREATE_NO_WINDOW.
    let mut command = Command::new("cmd.exe");
    command.current_dir(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows::Win32::System::Threading::CREATE_NEW_CONSOLE;

        command.creation_flags(CREATE_NEW_CONSOLE.0);
    }
    spawn_detached(&mut command)
}

fn copy_path(dir: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let mut child = Command::new("cmd.exe")
            .args(["/C", "clip"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("gagal menjalankan clip.exe")?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(dir.to_string_lossy().as_bytes())
                .context("gagal menulis ke clipboard")?;
        }
        child.wait().context("clip.exe gagal")?;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = dir;
        bail!("menyalin path hanya didukung di Windows")
    }
}

fn spawn_detached(command: &mut Command) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("gagal membuka {command:?}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_targets_are_always_available() {
        let list = targets();
        for id in ["explorer", "terminal", "copy"] {
            let item = list.iter().find(|item| item.id == id).unwrap();
            assert!(item.available, "{id} harus selalu tersedia");
            assert!(!item.label.is_empty());
        }
    }

    #[test]
    fn unknown_target_is_rejected() {
        let dir = std::env::temp_dir();
        let err = open(&dir, "bukan-aplikasi").unwrap_err();
        assert!(err.to_string().contains("tidak dikenal"));
    }

    #[test]
    fn missing_folder_is_reported_before_launch() {
        let dir = std::env::temp_dir().join("devku-launcher-tidak-ada");
        let err = open(&dir, "explorer").unwrap_err();
        assert!(err.to_string().contains("tidak ada"));
    }

    #[test]
    fn missing_file_is_reported_before_launch() {
        let file = std::env::temp_dir().join("devku-launcher-tidak-ada.php.ini");
        let err = open_file(&file).unwrap_err();
        assert!(err.to_string().contains("tidak ada"));
    }
}
