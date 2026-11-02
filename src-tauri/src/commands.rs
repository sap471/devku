use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::detect;
use crate::download::{self, PhpRelease};
use crate::paths::Paths;
use crate::php::PhpManager;
use crate::session::{ImportReport, PhpExtensionsView, Session, SiteDraft, Snapshot};

pub struct AppState {
    /// None means the config failed to load. The UI must show an error, not
    /// crash, and must not write anything until the user fixes it.
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

/// anyhow loses its context chain under plain Display; `{:#}` also shows
/// the innermost cause, which is what the user needs.
fn error(err: anyhow::Error) -> String {
    format!("{err:#}")
}

fn unavailable(boot_error: &Option<String>) -> String {
    boot_error
        .clone()
        .unwrap_or_else(|| "konfigurasi belum siap".to_string())
}

async fn session_paths(state: &State<'_, AppState>) -> Result<Paths, String> {
    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    Ok(session.paths().clone())
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

/// `caddy trust` from the app: run the binary in use, record the result in
/// config, and return a fresh snapshot. Requires a live service because
/// `caddy trust` reads CA info from the admin API.
#[tauri::command]
pub async fn trust_caddy(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    if !session.is_running() {
        return Err("Caddy belum berjalan. Start semua dulu, lalu percayai sertifikatnya.".into());
    }

    let backend = crate::caddy::CaddyBackend::new(session.paths().clone());
    backend.trust().await.map_err(error)?;
    session.mark_https_trusted().map_err(error)?;
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
    // Detection spawns external processes; do not block the async runtime.
    tokio::task::spawn_blocking(move || crate::runtime_detect::detect(&paths))
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn open_terminal(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    terminals: State<'_, crate::terminal::Terminals>,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<u32, String> {
    // The shell inherits the app's PATH, read when DevKu starts. Without
    // this injection, a freshly installed `php` is invisible in the terminal.
    let path_dirs = php_path_dirs(&state).await;

    terminals
        .open(app, cwd, cols.unwrap_or(80), rows.unwrap_or(24), path_dirs)
        .map_err(error)
}

async fn php_path_dirs(state: &State<'_, AppState>) -> Vec<PathBuf> {
    let (paths, active) = {
        let guard = state.session.lock().await;
        let Some(session) = guard.as_ref() else {
            return Vec::new();
        };
        (session.paths().clone(), session.config().php.active.clone())
    };

    let installed = PhpManager::new(paths.clone()).installed();
    let chosen = if installed.iter().any(|version| version == &active) {
        active
    } else {
        installed.last().cloned().unwrap_or_default()
    };

    let mut dirs = vec![crate::path_env::bin_dir(&paths)];
    if !chosen.is_empty() {
        dirs.push(paths.php_dir(&chosen));
    }

    // The bundled Bun exists only when this machine has no Bun of its own. Without
    // this folder, the `bun run ...` script button will not find its binary.
    let bun = paths.bundled_binary("bun");
    if bun.exists()
        && let Some(dir) = bun.parent()
    {
        dirs.push(dir.to_path_buf());
    }

    // Composer global tools such as `laravel` can be called right away in
    // the DevKu terminal, without waiting for a new terminal to read the user PATH.
    let composer_global = crate::composer::global_bin_dir();
    if composer_global.is_dir() {
        dirs.push(composer_global);
    }

    // `mariadb` and `mariadb-dump` work directly in the DevKu terminal.
    let mariadb_bin = paths.mariadb_bin();
    if mariadb_bin.is_dir() {
        dirs.push(mariadb_bin);
    }

    dirs
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
pub struct PhpCatalogEntry {
    #[serde(flatten)]
    pub release: PhpRelease,
    pub installed: bool,
    /// Comes from the `engines/php` folder, not from `releases.json`. It has
    /// no download file, so the UI only shows it as installed.
    pub local: bool,
}

#[derive(Clone, Serialize)]
struct DownloadProgress {
    /// "php", "caddy", or "bun". The UI uses this to pick which row
    /// shows progress.
    target: &'static str,
    version: String,
    received: u64,
    total: Option<u64>,
    stage: &'static str,
}

fn report(
    app: &AppHandle,
    target: &'static str,
    version: &str,
    received: u64,
    total: Option<u64>,
    stage: &'static str,
) {
    let _ = app.emit(
        "download://progress",
        DownloadProgress {
            target,
            version: version.to_string(),
            received,
            total,
            stage,
        },
    );
}

#[derive(Serialize)]
pub struct PhpCatalogView {
    pub entries: Vec<PhpCatalogEntry>,
    /// Set when the download list cannot be fetched. Versions already in the
    /// folder still show, so a dead connection is no reason to lose PHP.
    pub warning: Option<String>,
}

#[tauri::command]
pub async fn php_catalog(state: State<'_, AppState>) -> Result<PhpCatalogView, String> {
    let paths = session_paths(&state).await?;
    let installed = PhpManager::new(paths).installed();
    let client = download::client().map_err(error)?;
    let (releases, warning) = match download::php_releases(&client).await {
        Ok(list) => (list, None),
        Err(err) => (
            Vec::new(),
            Some(format!(
                "Daftar versi dari windows.php.net tidak bisa diambil: {err:#}"
            )),
        ),
    };

    let entries = build_catalog(releases, &installed);

    Ok(PhpCatalogView { entries, warning })
}

/// `8.5.12-1` to `8.5`. Only for branch labels, not for any decision.
fn branch_of(version: &str) -> String {
    version.split('.').take(2).collect::<Vec<_>>().join(".")
}

/// Official releases plus existing folders combined. Split from the command so
/// it can be tested without network and without Tauri.
fn build_catalog(releases: Vec<PhpRelease>, installed: &[String]) -> Vec<PhpCatalogEntry> {
    let mut entries: Vec<PhpCatalogEntry> = releases
        .into_iter()
        .map(|release| PhpCatalogEntry {
            installed: installed.iter().any(|version| version == &release.version),
            local: false,
            release,
        })
        .collect();

    // Existing folders stay visible even when their version is gone from
    // releases.json: old builds, custom builds, or manual extractions.
    for version in installed {
        if entries
            .iter()
            .any(|entry| &entry.release.version == version)
        {
            continue;
        }
        entries.push(PhpCatalogEntry {
            installed: true,
            local: true,
            release: PhpRelease {
                branch: branch_of(version),
                version: version.clone(),
                filename: String::new(),
                sha256: String::new(),
                size: String::new(),
            },
        });
    }

    entries
}

/// Downloads, verifies the SHA-256 from releases.json, then installs. If
/// any step fails, no half-finished folder is left behind:
/// extraction starts only after the file is complete and matches.
#[tauri::command]
pub async fn install_php(
    app: AppHandle,
    state: State<'_, AppState>,
    slot: State<'_, download::Slot>,
    version: String,
) -> Result<Snapshot, String> {
    let paths = session_paths(&state).await?;
    let cancel = slot.begin().map_err(error)?;

    let result = install_php_inner(&app, &paths, &version, &cancel).await;
    slot.finish();
    result.map_err(error)?;

    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    Ok(session.snapshot().await)
}

async fn install_php_inner(
    app: &AppHandle,
    paths: &Paths,
    version: &str,
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    let client = download::client()?;
    report(app, "php", version, 0, None, "daftar");

    let releases = download::php_releases(&client).await?;
    let release = releases
        .into_iter()
        .find(|item| item.version == version)
        .ok_or_else(|| anyhow::anyhow!("versi {version} tidak ada di daftar rilis PHP"))?;

    let archive = paths.downloads().join(&release.filename);
    download::download(
        &client,
        &release.url(),
        &archive,
        Some(&release.sha256),
        cancel,
        |received, total| report(app, "php", version, received, total, "unduh"),
    )
    .await?;

    report(app, "php", version, 0, None, "pasang");

    let manager = PhpManager::new(paths.clone());
    let install_version = version.to_string();
    let zip = archive.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        manager.extract(&zip, &install_version)?;
        manager.ensure_ini(&install_version)?;
        Ok(())
    })
    .await
    .map_err(|err| anyhow::anyhow!("pemasangan terhenti: {err}"))??;

    report(app, "php", version, 0, None, "selesai");
    Ok(())
}

#[tauri::command]
pub async fn install_caddy(
    app: AppHandle,
    state: State<'_, AppState>,
    slot: State<'_, download::Slot>,
) -> Result<Snapshot, String> {
    let paths = session_paths(&state).await?;
    let cancel = slot.begin().map_err(error)?;
    let result = install_caddy_inner(&app, &paths, &cancel).await;
    slot.finish();
    result.map_err(error)?;

    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    Ok(session.snapshot().await)
}

async fn install_caddy_inner(
    app: &AppHandle,
    paths: &Paths,
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    let client = download::client()?;
    report(app, "caddy", "", 0, None, "daftar");

    let asset = download::caddy_latest(&client).await?;
    let checksum = download::caddy_checksum(&client, &asset.version, &asset.filename).await?;
    let version = asset.version.clone();

    let archive = paths.downloads().join(&asset.filename);
    download::download(
        &client,
        &asset.url,
        &archive,
        Some(&checksum),
        cancel,
        |received, total| report(app, "caddy", &version, received, total, "unduh"),
    )
    .await?;

    report(app, "caddy", &version, 0, None, "pasang");

    let dir = paths.caddy_dir();
    let zip = archive.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        crate::archive::extract(&zip, &dir)?;
        Ok(())
    })
    .await
    .map_err(|err| anyhow::anyhow!("pemasangan terhenti: {err}"))??;

    if !paths.caddy_binary().exists() {
        anyhow::bail!("zip Caddy tidak berisi {}", paths.caddy_binary().display());
    }

    report(app, "caddy", &version, 0, None, "selesai");
    Ok(())
}

/// Installs Apache from Apache Lounge builds. DevKu picks the newest Win64
/// version from the official page, verifies the SHA-256, then stores it in
/// `engines/apache/<version>`.
#[tauri::command]
pub async fn install_apache(
    app: AppHandle,
    state: State<'_, AppState>,
    slot: State<'_, download::Slot>,
) -> Result<Snapshot, String> {
    let paths = session_paths(&state).await?;
    let cancel = slot.begin().map_err(error)?;

    // A running httpd locks httpd.exe; stop it first so
    // reinstalling the same version does not fail mid-extraction.
    {
        let mut guard = state.session.lock().await;
        if let Some(session) = guard.as_mut() {
            session.stop_apache().await;
        }
    }

    let result = install_apache_inner(&app, &paths, &cancel).await;
    slot.finish();

    // Whatever the outcome, restart the httpd stopped earlier so
    // a failed download does not also take down Apache sites.
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    let _ = session.refresh().await;

    result.map_err(error)?;
    Ok(session.snapshot().await)
}

async fn install_apache_inner(
    app: &AppHandle,
    paths: &Paths,
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    let client = download::client()?;
    report(app, "apache", "", 0, None, "daftar");

    let asset = download::apache_latest(&client).await?;
    let checksum = download::apache_checksum(&client, &asset).await?;
    let version = asset.version.clone();

    let archive = paths.downloads().join(&asset.filename);
    download::download(
        &client,
        &asset.url,
        &archive,
        Some(&checksum),
        cancel,
        |received, total| report(app, "apache", &version, received, total, "unduh"),
    )
    .await?;

    report(app, "apache", &version, 0, None, "pasang");

    let dir = paths.apache_version_dir(&version);
    let zip = archive.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        crate::archive::extract(&zip, &dir)?;
        if !crate::apache::has_httpd(&dir) {
            anyhow::bail!("zip Apache tidak berisi bin/httpd.exe");
        }
        Ok(())
    })
    .await
    .map_err(|err| anyhow::anyhow!("pemasangan terhenti: {err}"))??;

    report(app, "apache", &version, 0, None, "selesai");
    Ok(())
}

#[tauri::command]
pub async fn install_bun(
    app: AppHandle,
    state: State<'_, AppState>,
    slot: State<'_, download::Slot>,
) -> Result<Snapshot, String> {
    let paths = session_paths(&state).await?;
    let cancel = slot.begin().map_err(error)?;
    let result = install_bun_inner(&app, &paths, &cancel).await;
    slot.finish();
    result.map_err(error)?;

    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    Ok(session.snapshot().await)
}

async fn install_bun_inner(
    app: &AppHandle,
    paths: &Paths,
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    let client = download::client()?;
    report(app, "bun", "", 0, None, "daftar");

    let asset = download::bun_latest(&client).await?;
    let version = asset.version.clone();
    let archive = paths.downloads().join(&asset.filename);

    // GitHub publishes no checksum for Bun, so file integrity
    // rests on HTTPS to github.com.
    download::download(
        &client,
        &asset.url,
        &archive,
        None,
        cancel,
        |received, total| report(app, "bun", &version, received, total, "unduh"),
    )
    .await?;

    report(app, "bun", &version, 0, None, "pasang");

    let dir = paths.engines().join("bun");
    let zip = archive.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        crate::archive::extract(&zip, &dir)?;
        Ok(())
    })
    .await
    .map_err(|err| anyhow::anyhow!("pemasangan terhenti: {err}"))??;

    let binary = paths.bundled_binary("bun");
    if !binary.exists() {
        anyhow::bail!("zip Bun tidak berisi {}", binary.display());
    }

    report(app, "bun", &version, 0, None, "selesai");
    Ok(())
}

/// Composer uses the PHP on PATH first. When there is none, the PHP bundled with
/// DevKu is used and its path is remembered so the shim keeps working in
/// the DevKu terminal.
#[tauri::command]
pub async fn install_composer(
    app: AppHandle,
    state: State<'_, AppState>,
    slot: State<'_, download::Slot>,
) -> Result<Snapshot, String> {
    let paths = session_paths(&state).await?;
    let cancel = slot.begin().map_err(error)?;
    let result = crate::composer::install(&paths, &cancel, |stage, received, total| {
        report(&app, "composer", "", received, total, stage);
    })
    .await;
    slot.finish();
    result.map_err(error)?;

    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub fn cancel_download(slot: State<'_, download::Slot>) -> bool {
    slot.cancel()
}

#[tauri::command]
pub async fn set_site_php(
    state: State<'_, AppState>,
    id: String,
    version: String,
) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.set_php(&id, &version).await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn set_site_backend(
    state: State<'_, AppState>,
    id: String,
    backend: crate::config::Backend,
) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session
        .set_site_backend(&id, backend)
        .await
        .map_err(error)?;
    Ok(session.snapshot().await)
}

/// The Apache port is separate from Caddy's 80/443, so the value can change
/// without touching Caddy.
#[tauri::command]
pub async fn set_apache_port(state: State<'_, AppState>, port: u16) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.set_apache_port(port).await.map_err(error)?;
    Ok(session.snapshot().await)
}

/// Project parent folder, not the data folder. `None` returns it to the default.
#[tauri::command]
pub async fn set_workspace(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.set_workspace(path).await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[derive(Clone, Serialize)]
struct MoveProgress {
    done: u64,
    total: u64,
}

#[derive(Serialize)]
pub struct MoveReport {
    pub home: String,
    pub files: u64,
    pub bytes: u64,
}

/// Moves the data folder to another location. Services are stopped first because
/// running processes hold the engine and database files, then
/// the pointer is written after the copy is complete. The app must restart
/// afterwards, and the UI triggers it.
#[tauri::command]
pub async fn move_data_home(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<MoveReport, String> {
    if crate::location::env_home().is_some() {
        return Err("DEVKU_HOME sedang diset, lokasi data dikunci environment".to_string());
    }

    let current = {
        let guard = state.session.lock().await;
        let session = guard
            .as_ref()
            .ok_or_else(|| unavailable(&state.boot_error))?;
        session.paths().root.clone()
    };

    let target = PathBuf::from(path.trim());
    crate::location::validate_target(&current, &target).map_err(error)?;

    // The shim in `bin/` writes an absolute path to PHP, so after a move it must
    // be rewritten and the old PATH entry replaced. Captured before
    // services stop because that state is read from the user PATH.
    let old_paths = Paths {
        root: current.clone(),
    };
    let registered = crate::path_env::is_registered(&crate::path_env::bin_dir(&old_paths));
    let shim_version = crate::path_env::status(&old_paths).shim_version;

    {
        let mut guard = state.session.lock().await;
        if let Some(session) = guard.as_mut() {
            session.stop().await;
        }
    }

    let from = current.clone();
    let to = target.clone();
    let copy_target = to.clone();
    let emitter = app.clone();
    let totals = tokio::task::spawn_blocking(move || {
        let mut last = std::time::Instant::now();
        crate::location::copy_tree(&from, &copy_target, &mut |done, total| {
            // Hundreds of MB: per-file reports would flood the channel.
            if last.elapsed() >= std::time::Duration::from_millis(120) {
                last = std::time::Instant::now();
                let _ = emitter.emit("location://progress", MoveProgress { done, total });
            }
        })
    })
    .await
    .map_err(|err| format!("pemindahan gagal dijalankan: {err}"))?
    .map_err(error)?;

    let default_home = crate::paths::default_home();
    crate::location::write_pointer(&default_home, &to).map_err(error)?;

    // The pointer is correct. What remains is cleanup: a failure
    // here must not mark the move as failed.
    let new_paths = Paths { root: to.clone() };
    if registered {
        let _ = crate::path_env::remove_dir(&crate::path_env::bin_dir(&old_paths));
        let _ = crate::path_env::add_dir(&crate::path_env::bin_dir(&new_paths));
    }
    if let Some(version) = shim_version {
        let _ = crate::path_env::sync_shim(&new_paths, &version);
    }

    // The copy is complete and the pointer already targets it, so the rest
    // is cleaning the old folder without cancelling the move.
    let _ = crate::location::prune_source(&current);

    let _ = app.emit(
        "location://progress",
        MoveProgress {
            done: totals.bytes,
            total: totals.bytes,
        },
    );

    Ok(MoveReport {
        home: to.to_string_lossy().to_string(),
        files: totals.files,
        bytes: totals.bytes,
    })
}

/// Called after the data folder moves. Services are not up in the old process
/// because they were stopped before the copy.
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

#[tauri::command]
pub async fn set_default_php(
    state: State<'_, AppState>,
    version: String,
) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.set_default_php(&version).map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn enable_php_path(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    let version = session.snapshot().await.default_php;
    if version.is_empty() {
        return Err("belum ada PHP terpasang".to_string());
    }

    crate::path_env::register(session.paths(), &version).map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn disable_php_path(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    crate::path_env::unregister(session.paths()).map_err(error)?;
    Ok(session.snapshot().await)
}

#[derive(Serialize)]
pub struct PhpRemovalResponse {
    pub removal: crate::session::PhpRemoval,
    pub snapshot: Snapshot,
}

/// Opens one version php.ini in an editor. The editor name is returned so the UI
/// can say what was just opened.
#[tauri::command]
pub async fn open_php_ini(state: State<'_, AppState>, version: String) -> Result<String, String> {
    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    let ini = session.php_ini_path(&version).map_err(error)?;
    crate::launcher::open_file(&ini)
        .map(str::to_string)
        .map_err(error)
}

#[tauri::command]
pub async fn restart_php(state: State<'_, AppState>, version: String) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.restart_php(&version).await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn php_extensions(
    state: State<'_, AppState>,
    version: String,
) -> Result<PhpExtensionsView, String> {
    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.php_extensions(&version).await.map_err(error)
}

#[tauri::command]
pub async fn set_php_extension(
    state: State<'_, AppState>,
    version: String,
    name: String,
    enable: bool,
) -> Result<PhpExtensionsView, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session
        .set_php_extension(&version, &name, enable)
        .await
        .map_err(error)
}

#[tauri::command]
pub async fn remove_php(
    state: State<'_, AppState>,
    version: String,
) -> Result<PhpRemovalResponse, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    let removal = session.remove_php(&version).await.map_err(error)?;
    Ok(PhpRemovalResponse {
        removal,
        snapshot: session.snapshot().await,
    })
}

#[tauri::command]
pub async fn remove_all_php(state: State<'_, AppState>) -> Result<PhpRemovalResponse, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    let removal = session.remove_all_php().await.map_err(error)?;
    Ok(PhpRemovalResponse {
        removal,
        snapshot: session.snapshot().await,
    })
}

#[derive(Serialize)]
pub struct DatabaseRemovalResponse {
    pub removal: crate::session::DatabaseRemoval,
    /// Path to the .sql file when a backup was requested.
    pub backup: Option<String>,
    pub snapshot: Snapshot,
}

#[tauri::command]
pub async fn install_database(
    app: AppHandle,
    state: State<'_, AppState>,
    slot: State<'_, download::Slot>,
) -> Result<Snapshot, String> {
    let paths = session_paths(&state).await?;
    let cancel = slot.begin().map_err(error)?;
    let result = install_database_inner(&app, &paths, &cancel).await;
    slot.finish();
    result.map_err(error)?;

    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    session.prepare_database().await.map_err(error)?;
    Ok(session.snapshot().await)
}

async fn install_database_inner(
    app: &AppHandle,
    paths: &Paths,
    cancel: &AtomicBool,
) -> anyhow::Result<()> {
    let client = download::client()?;
    let version = crate::database::VERSION;
    let filename = crate::database::archive_name(version);
    report(app, "mariadb", version, 0, None, "daftar");

    let checksum = download::mariadb_checksum(&client).await?;
    let archive = paths.downloads().join(&filename);
    download::download(
        &client,
        &crate::database::archive_url(version),
        &archive,
        Some(&checksum),
        cancel,
        |received, total| report(app, "mariadb", version, received, total, "unduh"),
    )
    .await?;

    report(app, "mariadb", version, 0, None, "pasang");

    // Extraction takes a while: around 300 MB of small files.
    let dir = paths.mariadb_dir();
    let zip = archive.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        crate::archive::extract(&zip, &dir)?;
        Ok(())
    })
    .await
    .map_err(|err| anyhow::anyhow!("pemasangan terhenti: {err}"))??;

    if !paths.mariadb_bin().join("mysqld.exe").exists() {
        anyhow::bail!("zip MariaDB tidak berisi bin/mysqld.exe");
    }

    report(app, "mariadb", version, 0, None, "selesai");
    Ok(())
}

#[tauri::command]
pub async fn start_database(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.start_database().await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn stop_database(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.stop_database().await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn set_database_autostart(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session
        .set_database_autostart(enabled)
        .await
        .map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn reset_database_password(
    state: State<'_, AppState>,
    password: String,
) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session
        .reset_database_password(&password)
        .await
        .map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn remove_database(
    state: State<'_, AppState>,
    remove_data: bool,
    backup: bool,
) -> Result<DatabaseRemovalResponse, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    // A backup only makes sense when the data is actually discarded.
    let backup_path = if backup && remove_data {
        Some(session.backup_database().await.map_err(error)?)
    } else {
        None
    };

    let removal = session.remove_database(remove_data).await.map_err(error)?;

    Ok(DatabaseRemovalResponse {
        removal,
        backup: backup_path.map(|path| path.to_string_lossy().to_string()),
        snapshot: session.snapshot().await,
    })
}

#[tauri::command]
pub async fn open_backups(state: State<'_, AppState>) -> Result<String, String> {
    let paths = session_paths(&state).await?;
    std::fs::create_dir_all(paths.backups()).map_err(|err| error(anyhow::Error::from(err)))?;
    crate::launcher::open(&paths.backups(), "explorer").map_err(error)?;
    Ok(paths.backups().to_string_lossy().to_string())
}

#[tauri::command]
pub async fn clear_downloads(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let paths = session_paths(&state).await?;
    crate::download::clear_archives(&paths).map_err(error)?;

    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;
    Ok(session.snapshot().await)
}

#[derive(Serialize)]
pub struct ImportResponse {
    pub report: ImportReport,
    pub snapshot: Snapshot,
}

/// Write the config in use to a file the user chooses.
#[tauri::command]
pub async fn export_config(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    let json = session.export_config().map_err(error)?;
    std::fs::write(&path, json).map_err(|err| format!("gagal menulis {path}: {err}"))
}

/// Replace the config with a file from another machine, then align the runtime.
#[tauri::command]
pub async fn import_config(
    state: State<'_, AppState>,
    path: String,
) -> Result<ImportResponse, String> {
    let text =
        std::fs::read_to_string(&path).map_err(|err| format!("gagal membaca {path}: {err}"))?;

    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    let report = session.import_config(&text).await.map_err(error)?;
    Ok(ImportResponse {
        report,
        snapshot: session.snapshot().await,
    })
}

/// Turn DevKu launch at user login on or off (HKCU Run).
#[tauri::command]
pub async fn set_start_at_login(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<Snapshot, String> {
    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    let exe = std::env::current_exe().map_err(|err| format!("tidak tahu lokasi DevKu: {err}"))?;
    crate::autostart::set_enabled(&exe, enabled).map_err(error)?;
    Ok(session.snapshot().await)
}

/// Close the first-run panel, whether finished or skipped.
#[tauri::command]
pub async fn finish_onboarding(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.mark_onboarded().map_err(error)?;
    Ok(session.snapshot().await)
}

/// Register the Composer global package folder on the user PATH, so `laravel`
/// and other global tools can be called from any terminal.
#[tauri::command]
pub async fn enable_composer_global_path(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    let dir = crate::composer::global_bin_dir();
    if !dir.is_dir() {
        return Err(format!(
            "folder Composer global belum ada di {}. Jalankan `composer global require laravel/installer` dulu.",
            dir.display()
        ));
    }

    crate::path_env::add_dir(&dir).map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn disable_composer_global_path(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    crate::path_env::remove_dir(&crate::composer::global_bin_dir()).map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn site_commands(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<crate::scripts::CommandView>, String> {
    let paths = session_paths(&state).await?;

    // Runtime detection spawns external processes; do it outside the lock and
    // outside the async worker.
    let bun = tokio::task::spawn_blocking(move || {
        crate::runtime_detect::detect(&paths)
            .iter()
            .any(|runtime| runtime.name == "bun" && runtime.version.is_some())
    })
    .await
    .map_err(|err| err.to_string())?;

    let root = {
        let guard = state.session.lock().await;
        let session = guard
            .as_ref()
            .ok_or_else(|| unavailable(&state.boot_error))?;
        let site = session
            .config()
            .sites
            .iter()
            .find(|site| site.id == id)
            .ok_or_else(|| format!("site {id} tidak ditemukan"))?;
        site.root.clone().unwrap_or_else(|| site.path.clone())
    };

    tokio::task::spawn_blocking(move || crate::scripts::for_site(&root, bun))
        .await
        .map_err(|err| err.to_string())
}

/// List of apps for opening folders. Detection calls `where.exe`, so
/// it runs outside the async worker.
#[tauri::command]
pub async fn open_targets() -> Result<Vec<crate::launcher::OpenTarget>, String> {
    tokio::task::spawn_blocking(crate::launcher::targets)
        .await
        .map_err(|err| err.to_string())
}

/// `kind` is only "site" (needs `id`) or "data". The path does not come from the UI,
/// so the window cannot be used to open arbitrary folders.
#[tauri::command]
pub async fn open_in(
    state: State<'_, AppState>,
    kind: String,
    id: Option<String>,
    target: String,
) -> Result<(), String> {
    let dir = {
        let guard = state.session.lock().await;
        let session = guard
            .as_ref()
            .ok_or_else(|| unavailable(&state.boot_error))?;

        match kind.as_str() {
            "data" => session.paths().root.clone(),
            "workspace" => {
                // Titlebar and Settings buttons must work before
                // the workspace folder has ever been created.
                let dir = session.config().workspace_dir();
                std::fs::create_dir_all(&dir)
                    .map_err(|err| format!("gagal membuat {}: {err}", dir.display()))?;
                dir
            }
            "site" => {
                let id = id.ok_or_else(|| "situs tidak dipilih".to_string())?;
                let site = session
                    .config()
                    .sites
                    .iter()
                    .find(|site| site.id == id)
                    .ok_or_else(|| format!("site {id} tidak ditemukan"))?;
                site.root.clone().unwrap_or_else(|| site.path.clone())
            }
            other => return Err(format!("jenis folder tidak dikenal: {other}")),
        }
    };

    tokio::task::spawn_blocking(move || crate::launcher::open(&dir, &target))
        .await
        .map_err(|err| err.to_string())?
        .map_err(error)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str) -> PhpRelease {
        PhpRelease {
            branch: branch_of(version),
            version: version.to_string(),
            filename: format!("php-{version}-nts.zip"),
            sha256: "a".repeat(64),
            size: "35 MB".to_string(),
        }
    }

    #[test]
    fn catalog_marks_release_installed_from_folder() {
        let entries = build_catalog(vec![release("8.3.36")], &["8.3.36".to_string()]);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].installed);
        assert!(!entries[0].local);
    }

    #[test]
    fn catalog_adds_folder_versions_absent_from_releases() {
        let entries = build_catalog(
            vec![release("8.3.36")],
            &["8.3.36".to_string(), "8.5.12-1".to_string()],
        );

        let local = entries
            .iter()
            .find(|entry| entry.local)
            .expect("versi lokal");
        assert_eq!(local.release.version, "8.5.12-1");
        assert_eq!(local.release.branch, "8.5");
        assert!(local.installed);
        assert!(local.release.filename.is_empty());

        // Official releases are still present, so installing another version stays possible.
        assert!(
            entries
                .iter()
                .any(|entry| entry.release.version == "8.3.36")
        );
    }

    #[test]
    fn catalog_stays_usable_without_release_list() {
        let entries = build_catalog(Vec::new(), &["7.4.33".to_string()]);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].installed && entries[0].local);
    }
}
