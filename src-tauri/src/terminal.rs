use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
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
    pub fn open(&self, app: AppHandle, cwd: Option<String>, cols: u16, rows: u16) -> Result<u32> {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let pair = native_pty_system()
            .openpty(size(cols, rows))
            .context("gagal membuka PTY")?;

        let mut command = shell_command();
        command.env("TERM", "xterm-256color");
        if let Some(dir) = cwd.as_ref().filter(|dir| !dir.is_empty()) {
            command.cwd(dir);
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

fn shell_command() -> CommandBuilder {
    #[cfg(windows)]
    {
        // PowerShell menangani jalur berspasi lebih baik daripada cmd dan sudah
        // tersedia di semua Windows 10/11.
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

/// Dipakai perintah Tauri; menjaga nama channel tetap satu sumber.
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
