use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::detect;
use crate::download::{self, PhpRelease};
use crate::paths::Paths;
use crate::php::PhpManager;
use crate::session::{ImportReport, Session, SiteDraft, Snapshot};

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

/// `caddy trust` lewat aplikasi: jalankan binary terpakai, catat hasilnya di
/// config, dan kembalikan snapshot terbaru. Butuh service hidup karena
/// `caddy trust` mengambil info CA dari admin API.
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
    // Deteksi menjalankan proses eksternal; jangan menahan runtime async.
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
    // Shell mewarisi PATH aplikasi, yang dibaca saat DevKu diluncurkan. Tanpa
    // suntikan ini, `php` yang baru dipasang tidak terlihat di terminal.
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

    // Bun bawaan hanya ada kalau mesin ini tidak punya Bun sendiri. Tanpa
    // folder ini, tombol skrip `bun run ...` tidak akan menemukan binary-nya.
    let bun = paths.bundled_binary("bun");
    if bun.exists()
        && let Some(dir) = bun.parent()
    {
        dirs.push(dir.to_path_buf());
    }

    // Tool global Composer seperti `laravel` langsung bisa dipanggil di
    // terminal DevKu, tanpa menunggu terminal baru membaca PATH pengguna.
    let composer_global = crate::composer::global_bin_dir();
    if composer_global.is_dir() {
        dirs.push(composer_global);
    }

    // `mariadb` dan `mariadb-dump` bisa dipakai langsung di terminal DevKu.
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
    /// Berasal dari folder `engines/php`, bukan dari `releases.json`. Tidak
    /// punya berkas unduhan, jadi UI hanya menampilkannya sebagai terpasang.
    pub local: bool,
}

#[derive(Clone, Serialize)]
struct DownloadProgress {
    /// "php", "caddy", atau "bun". UI memakai ini untuk memilih baris mana yang
    /// menampilkan progres.
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
    /// Diisi kalau daftar unduhan tidak bisa diambil. Versi yang sudah ada di
    /// folder tetap ditampilkan, jadi koneksi mati bukan alasan kehilangan PHP.
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

/// `8.5.12-1` → `8.5`. Hanya untuk label cabang, bukan keputusan apapun.
fn branch_of(version: &str) -> String {
    version.split('.').take(2).collect::<Vec<_>>().join(".")
}

/// Gabungan rilis resmi dan folder yang sudah ada. Dipisah dari perintah supaya
/// bisa diuji tanpa jaringan dan tanpa Tauri.
fn build_catalog(releases: Vec<PhpRelease>, installed: &[String]) -> Vec<PhpCatalogEntry> {
    let mut entries: Vec<PhpCatalogEntry> = releases
        .into_iter()
        .map(|release| PhpCatalogEntry {
            installed: installed.iter().any(|version| version == &release.version),
            local: false,
            release,
        })
        .collect();

    // Folder yang sudah ada tetap terlihat walau versinya tidak lagi ada di
    // releases.json: build lama, build custom, atau ekstraksi manual.
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

/// Mengunduh, memverifikasi SHA-256 dari releases.json, lalu memasang. Kalau
/// ada langkah yang gagal, tidak ada folder setengah jadi yang ditinggalkan:
/// ekstraksi baru dimulai setelah berkas utuh dan cocok.
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
        // Zip 35 MB tidak ada gunanya disimpan setelah diekstrak.
        let _ = std::fs::remove_file(&zip);
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
        let _ = std::fs::remove_file(&zip);
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

    // GitHub tidak menerbitkan checksum untuk Bun, jadi integritas berkas
    // bergantung pada HTTPS ke github.com.
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
        let _ = std::fs::remove_file(&zip);
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

/// Composer memakai PHP yang ada di PATH lebih dulu. Kalau tidak ada, PHP
/// bawaan DevKu dipakai dan jalurnya diingat supaya shim tetap hidup di
/// terminal DevKu.
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
    php: String,
) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.set_php(&id, &php).await.map_err(error)?;
    Ok(session.snapshot().await)
}

#[tauri::command]
pub async fn set_default_php(state: State<'_, AppState>, php: String) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.set_default_php(&php).map_err(error)?;
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

/// Membuka php.ini satu versi di editor. Nama editor dikembalikan supaya UI
/// bisa menyebut apa yang barusan dibuka.
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
    /// Path berkas .sql kalau backup diminta.
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

    // Ekstraksi butuh waktu: sekitar 300 MB berkas kecil.
    let dir = paths.mariadb_dir();
    let zip = archive.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        crate::archive::extract(&zip, &dir)?;
        let _ = std::fs::remove_file(&zip);
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

    // Backup hanya masuk akal kalau datanya memang ikut dibuang.
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

#[derive(Serialize)]
pub struct ImportResponse {
    pub report: ImportReport,
    pub snapshot: Snapshot,
}

/// Tulis config yang sedang dipakai ke berkas pilihan user.
#[tauri::command]
pub async fn export_config(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let guard = state.session.lock().await;
    let session = guard
        .as_ref()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    let json = session.export_config().map_err(error)?;
    std::fs::write(&path, json).map_err(|err| format!("gagal menulis {path}: {err}"))
}

/// Ganti config dengan berkas dari mesin lain, lalu selaraskan runtime.
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

/// Nyalakan atau matikan pembukaan DevKu saat user login (HKCU Run).
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

/// Tutup panel first-run, baik setelah selesai maupun dilewati.
#[tauri::command]
pub async fn finish_onboarding(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let mut guard = state.session.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| unavailable(&state.boot_error))?;

    session.mark_onboarded().map_err(error)?;
    Ok(session.snapshot().await)
}

/// Daftarkan folder paket global Composer ke PATH pengguna, supaya `laravel`
/// dan tool global lain bisa dipanggil dari terminal mana pun.
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

    // Deteksi runtime menjalankan proses eksternal; lakukan di luar lock dan
    // di luar worker async.
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

/// Daftar aplikasi untuk membuka folder. Deteksi memanggil `where.exe`, jadi
/// dijalankan di luar worker async.
#[tauri::command]
pub async fn open_targets() -> Result<Vec<crate::launcher::OpenTarget>, String> {
    tokio::task::spawn_blocking(crate::launcher::targets)
        .await
        .map_err(|err| err.to_string())
}

/// `kind` hanya "site" (butuh `id`) atau "data". Path tidak datang dari UI,
/// jadi jendela tidak bisa dipakai membuka folder sembarang.
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

        // Rilis resmi tetap hadir, jadi pemasangan versi lain tidak hilang.
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
