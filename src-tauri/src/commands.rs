use std::path::PathBuf;

use serde::Serialize;
use tauri::State;
use tokio::sync::Mutex;

use crate::detect;
use crate::session::{Session, SiteDraft, Snapshot};

pub struct AppState {
    /// None berarti config gagal dibaca. UI harus menampilkan galat, bukan
    /// crash, dan tidak boleh menulis apa pun sampai user memperbaikinya.
    pub session: Mutex<Option<Session>>,
    pub boot_error: Option<String>,
}

impl AppState {
    pub fn new(session: Result<Session, anyhow::Error>) -> Self {
        match session {
            Ok(session) => Self {
                session: Mutex::new(Some(session)),
                boot_error: None,
            },
            Err(err) => Self {
                session: Mutex::new(None),
                boot_error: Some(format!("{err:#}")),
            },
        }
    }
}

/// anyhow kehilangan rantai konteks saat di-Display biasa; `{:#}` menampilkan
/// penyebab terdalam juga, yang justru dibutuhkan user.
fn error(err: anyhow::Error) -> String {
    format!("{err:#}")
}

fn unavailable(boot_error: &Option<String>) -> String {
    boot_error
        .clone()
        .unwrap_or_else(|| "konfigurasi belum siap".to_string())
}

#[tauri::command]
pub async fn snapshot(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn start(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    session.start().await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn stop(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    session.stop().await;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn add_site(state: State<'_, AppState>, draft: SiteDraft) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    session.add_site(draft).await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn remove_site(state: State<'_, AppState>, id: String) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    session.remove_site(&id).await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn set_site_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    session.set_enabled(&id, enabled).await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn runtimes(
    state: State<'_, AppState>,
) -> Result<Vec<crate::runtime_detect::RuntimeInfo>, String> {
    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    let paths = session.paths().clone();
    // Deteksi menjalankan proses eksternal; jangan menahan runtime async.
    tokio::task::spawn_blocking(move || crate::runtime_detect::detect(&paths))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn open_terminal(
    app: tauri::AppHandle,
    terminals: State<'_, crate::terminal::Terminals>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<u32, String> {
    terminals
        .open(app, cwd, cols.unwrap_or(80), rows.unwrap_or(24))
        .map_err(error)
}

#[tauri::command]
pub fn write_terminal(
    terminals: State<'_, crate::terminal::Terminals>,
    id: u32,
    data: String,
) -> Result<(), String> {
    terminals.write(id, &data).map_err(error)
}

#[tauri::command]
pub fn resize_terminal(
    terminals: State<'_, crate::terminal::Terminals>,
    id: u32,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    terminals.resize(id, cols, rows).map_err(error)
}

#[tauri::command]
pub fn close_terminal(
    terminals: State<'_, crate::terminal::Terminals>,
    id: u32,
) -> Result<(), String> {
    terminals.close(id).map_err(error)
}

#[derive(Serialize)]
pub struct DetectionView {
    pub framework: String,
    pub label: String,
    pub document_root: String,
    pub project_root: String,
}

#[tauri::command]
pub fn detect_project(path: String) -> Option<DetectionView> {
    detect::detect(&PathBuf::from(path)).map(|found| DetectionView {
        framework: found.framework.to_string(),
        label: detect::label(found.framework).to_string(),
        document_root: found.document_root.to_string_lossy().to_string(),
        project_root: found.project_root.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn tail_log(
    state: State<'_, AppState>,
    name: String,
    lines: Option<usize>,
) -> Result<Vec<String>, String> {
    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    session.tail_log(&name, lines.unwrap_or(200)).map_err(error)
}
