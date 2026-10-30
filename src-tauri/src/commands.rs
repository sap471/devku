use std::path::PathBuf;

use serde::Serialize;
use tauri::State;
use tokio::sync::Mutex;

use crate::detect;
use crate::session::{Session, SiteDraft, Snapshot};

pub struct AppState {
    pub session: Mutex<Session>,
}

/// anyhow kehilangan rantai konteks saat di-Display biasa; `{:#}` menampilkan
/// penyebab terdalam juga, yang justru dibutuhkan user.
fn error(err: anyhow::Error) -> String {
    format!("{err:#}")
}

#[tauri::command]
pub async fn snapshot(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let session = state.session.lock().await;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn start(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut session = state.session.lock().await;
    session.start().await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn stop(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut session = state.session.lock().await;
    session.stop().await;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn add_site(state: State<'_, AppState>, draft: SiteDraft) -> Result<Snapshot, String> {
    let mut session = state.session.lock().await;
    session.add_site(draft).await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn remove_site(state: State<'_, AppState>, id: String) -> Result<Snapshot, String> {
    let mut session = state.session.lock().await;
    session.remove_site(&id).await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn set_site_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<Snapshot, String> {
    let mut session = state.session.lock().await;
    session.set_enabled(&id, enabled).await.map_err(error)?;
    Ok(session.snapshot().await)
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
    let session = state.session.lock().await;
    session.tail_log(&name, lines.unwrap_or(200)).map_err(error)
}
