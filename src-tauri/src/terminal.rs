use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{Context, Result, bail};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tauri::{AppHandle, Emitter};

pub struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Default)]
pub struct Terminals {
    next: AtomicU32,
    sessions: Mutex<HashMap<u32, Session>>,
}

impl Terminals {
    pub fn open(
        &self,
        app: AppHandle,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
        path_dirs: Vec<PathBuf>,
    ) -> Result<u32> {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let pair = native_pty_system()
            .openpty(size(cols, rows))
            .context("gagal membuka PTY")?;

        let mut command = shell_command();
        command.env("TERM", "xterm-256color");
        if let Some(dir) = cwd.as_ref().filter(|dir| !dir.is_empty()) {
            command.cwd(dir);
        }
        if !path_dirs.is_empty() {
            command.env("PATH", joined_path(&path_dirs));
        }

        let child = pair
            .slave
            .spawn_command(command)
            .context("gagal menjalankan shell")?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let channel = format!("pty://{id}");
        let exit_channel = format!("pty://{id}/exit");
        let handle = app.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
                        let _ = handle.emit(&channel, chunk);
                    }
                    Err(_) => break,
                }
            }
            let _ = handle.emit(&exit_channel, ());
        });

        self.sessions.lock().unwrap().insert(
            id,
            Session {
                master: pair.master,
                writer,
                child,
            },
        );

        Ok(id)
    }

    pub fn write(&self, id: u32, data: &str) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions.get_mut(&id).context("terminal tidak ditemukan")?;
        session.writer.write_all(data.as_bytes())?;
        session.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions.get_mut(&id).context("terminal tidak ditemukan")?;
        session.master.resize(size(cols, rows))?;
        Ok(())
    }

    pub fn close(&self, id: u32) -> Result<()> {
        let session = self
            .sessions
            .lock()
            .unwrap()
            .remove(&id)
            .context("terminal tidak ditemukan")?;
        let mut session = session;
        session.child.kill().ok();
        Ok(())
    }
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn joined_path(dirs: &[PathBuf]) -> String {
    let dirs: Vec<String> = dirs
        .iter()
        .map(|dir| dir.to_string_lossy().to_string())
        .collect();
    let existing = std::env::var("PATH").unwrap_or_default();
    join_paths(&dirs, &existing)
}

/// The DevKu folder goes first so a `php` installed through DevKu wins
/// over any other PHP on this machine.
fn join_paths(dirs: &[String], existing: &str) -> String {
    let mut parts: Vec<&str> = dirs.iter().map(String::as_str).collect();
    if !existing.is_empty() {
        parts.push(existing);
    }
    parts.join(if cfg!(windows) { ";" } else { ":" })
}

fn shell_command() -> CommandBuilder {
    #[cfg(windows)]
    {
        // PowerShell handles paths with spaces better than cmd and ships
        // with every Windows 10/11.
        let mut command = CommandBuilder::new("powershell.exe");
        command.args(["-NoLogo", "-NoProfile"]);
        command
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        CommandBuilder::new(shell)
    }
}

/// Used by Tauri commands; keeps the channel name in one place.
pub fn channel(id: u32) -> String {
    format!("pty://{id}")
}

pub fn exit_channel(id: u32) -> String {
    format!("pty://{id}/exit")
}

pub fn ensure_alive(terminals: &Terminals, id: u32) -> Result<()> {
    if !terminals.sessions.lock().unwrap().contains_key(&id) {
        bail!("terminal {id} sudah tertutup");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEP: &str = if cfg!(windows) { ";" } else { ":" };

    #[test]
    fn path_dirs_go_before_inherited_entries() {
        let dirs = vec!["C:\\DevKu\\bin".to_string()];
        assert_eq!(
            join_paths(&dirs, "C:\\Windows"),
            format!("C:\\DevKu\\bin{SEP}C:\\Windows")
        );
    }

    #[test]
    fn empty_inherited_path_leaves_no_separator() {
        let dirs = vec!["C:\\DevKu\\bin".to_string()];
        assert_eq!(join_paths(&dirs, ""), "C:\\DevKu\\bin");
    }
}
