use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{Config, Site};
use crate::database::{DatabaseBackend, Mariadb};
use crate::detect;
use crate::paths::Paths;
use crate::proc::ServiceState;
use crate::runtime::{Hook, Runtime};

/// Permintaan tambah site dari UI. Semua opsional kecuali folder, karena
/// pemindaian folder sudah bisa mengisi sisanya.
#[derive(Clone, Debug, Deserialize)]
pub struct SiteDraft {
    pub path: PathBuf,
    #[serde(default)]
    pub php: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub framework: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SiteView {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub php: String,
    pub framework: String,
    pub enabled: bool,
    pub path: String,
    /// Akar project, dipakai terminal. Untuk Laravel beda dengan `path`.
    pub root: String,
    pub port: Option<u16>,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServiceView {
    pub name: String,
    pub status: String,
    pub detail: Option<String>,
}

/// Ringkasan satu penghapusan PHP, supaya UI bisa menyebut apa yang ikut
/// berubah alih-alih hanya "berhasil".
#[derive(Clone, Debug, Serialize)]
pub struct PhpRemoval {
    /// Versi yang dihapus. Kosong berarti semua versi sekaligus.
    pub version: Option<String>,
    pub disabled_sites: Vec<String>,
    pub default_php: String,
    pub composer_moved: bool,
    pub removed_all: bool,
}

/// Keadaan server database lokal. Cukup untuk merender panel tanpa memanggil
/// perintah lain.
#[derive(Clone, Debug, Serialize)]
pub struct DatabaseView {
    pub engine: String,
    pub label: String,
    pub version: String,
    pub installed: bool,
    pub initialized: bool,
    pub port: u16,
    pub autostart: bool,
    pub status: String,
    pub detail: Option<String>,
    pub data_path: String,
    pub bin_path: String,
    pub has_password: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct DatabaseRemoval {
    pub removed_data: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub tld: String,
    pub running: bool,
    pub caddy_installed: bool,
    /// Catatan dari config: CA lokal Caddy pernah dipercaya lewat DevKu.
    pub https_trusted: bool,
    /// First-run sudah selesai atau dilewati.
    pub onboarded: bool,
    pub installed_php: Vec<String>,
    /// Versi yang dipakai untuk situs baru dan untuk shim `php` di PATH.
    pub default_php: String,
    pub php_path: crate::path_env::PathStatus,
    pub composer_installed: bool,
    pub composer_version: Option<String>,
    /// PHP yang dipakai shim Composer saat ini.
    pub composer_php: Option<String>,
    pub composer_php_missing: bool,
    /// Folder paket global Composer, tempat `laravel` dan tool lain berada.
    pub composer_global: crate::composer::GlobalStatus,
    /// Lokasi config yang sedang dipakai, supaya UI bisa menyebutnya.
    pub config_path: String,
    /// Folder data DevKu: config, engines, runtime, dan log.
    pub home_path: String,
    /// Nilai Run di registry untuk binary yang sedang berjalan, bukan sekadar
    /// catatan config: user bisa mencabutnya lewat Task Manager.
    pub start_at_login: bool,
    pub sites: Vec<SiteView>,
    pub services: Vec<ServiceView>,
    /// Server database lokal, termasuk saat belum terpasang.
    pub database: DatabaseView,
}

#[derive(Clone, Debug, Serialize)]
pub struct ImportReport {
    pub imported: usize,
    /// Site yang foldernya tidak ada di mesin ini.
    pub skipped: Vec<String>,
    /// Site yang PHP-nya belum terpasang, jadi dinonaktifkan.
    pub disabled: Vec<String>,
    pub active_php_cleared: Option<String>,
}

/// Pemilik state aplikasi: config di disk + runtime yang mungkin hidup.
/// Semua perintah UI lewat sini, jadi tidak ada jalur yang menyentuh
/// supervisor secara langsung.
pub struct Session {
    paths: Paths,
    config: Config,
    runtime: Option<Runtime>,
    hook: Option<Hook>,
}

impl Session {
    pub fn new(paths: Paths, hook: Option<Hook>) -> Result<Self> {
        paths.ensure()?;
        let config = Config::load(&paths)?;
        Ok(Self {
            paths,
            config,
            runtime: None,
            hook,
        })
    }

    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn is_running(&self) -> bool {
        self.runtime.is_some()
    }

    pub async fn snapshot(&self) -> Snapshot {
        let mut services = Vec::new();
        let mut ports = std::collections::BTreeMap::new();
        let mut php_status: Vec<(String, ServiceState)> = Vec::new();

        if let Some(runtime) = &self.runtime {
            for (name, state) in runtime.states().await {
                if let Some(version) = name.strip_prefix("php-") {
                    php_status.push((version.to_string(), state));
                } else {
                    services.push(ServiceView {
                        name,
                        status: state.tag().to_string(),
                        detail: state.detail(),
                    });
                }
            }
            ports = runtime.ports().clone();
        }

        for (version, state) in &php_status {
            services.push(ServiceView {
                name: format!("php-{version}"),
                status: state.tag().to_string(),
                detail: state.detail(),
            });
        }

        let sites = self
            .config
            .sites
            .iter()
            .map(|site| {
                let state = php_status
                    .iter()
                    .find(|(version, _)| version == &site.php)
                    .map(|(_, state)| state.clone());

                let (status, detail) = match (&self.runtime, state) {
                    (None, _) => ("stopped".to_string(), None),
                    (Some(_), Some(state)) if site.enabled => {
                        (state.tag().to_string(), state.detail())
                    }
                    (Some(_), _) => ("stopped".to_string(), None),
                };

                SiteView {
                    id: site.id.clone(),
                    name: site.name.clone(),
                    domain: site.domain.clone(),
                    php: site.php.clone(),
                    framework: site.framework.clone().unwrap_or_else(|| "php".to_string()),
                    enabled: site.enabled,
                    path: site.path.to_string_lossy().to_string(),
                    root: site
                        .root
                        .clone()
                        .unwrap_or_else(|| site.path.clone())
                        .to_string_lossy()
                        .to_string(),
                    port: if site.enabled {
                        ports.get(&site.php).copied()
                    } else {
                        None
                    },
                    status,
                    detail,
                }
            })
            .collect();

        let composer = crate::composer::status(&self.paths);

        let database_backend = Mariadb::new(self.paths.clone());
        let database_service = services
            .iter()
            .find(|item| item.name == "mariadb")
            .map(|item| (item.status.clone(), item.detail.clone()));

        let database = DatabaseView {
            engine: database_backend.id().to_string(),
            label: database_backend.label().to_string(),
            version: database_backend.version().to_string(),
            installed: database_backend.is_installed(),
            initialized: database_backend.is_initialized(),
            port: self.config.database.port,
            autostart: self.config.database.autostart,
            status: database_service
                .as_ref()
                .map(|(status, _)| status.clone())
                .unwrap_or_else(|| "stopped".to_string()),
            detail: database_service.and_then(|(_, detail)| detail),
            data_path: database_backend.data_dir().to_string_lossy().to_string(),
            bin_path: database_backend.bin_dir().to_string_lossy().to_string(),
            has_password: !database_backend.password().is_empty(),
        };

        Snapshot {
            tld: self.config.tld.clone(),
            running: self.runtime.is_some(),
            caddy_installed: crate::caddy::CaddyBackend::new(self.paths.clone()).is_installed(),
            https_trusted: self.config.https_trusted,
            onboarded: self.config.onboarded,
            installed_php: crate::php::PhpManager::new(self.paths.clone()).installed(),
            default_php: self.default_php().unwrap_or_default(),
            php_path: crate::path_env::status(&self.paths),
            composer_installed: composer.installed,
            composer_version: composer.version,
            composer_php: composer
                .php
                .map(|choice| choice.path.to_string_lossy().to_string()),
            composer_php_missing: composer.php_missing,
            composer_global: crate::composer::global_status(),
            config_path: self.paths.config_file().to_string_lossy().to_string(),
            home_path: self.paths.root.to_string_lossy().to_string(),
            start_at_login: crate::autostart::is_enabled(&crate::autostart::running_exe()),
            sites,
            services,
            database,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.runtime.is_some() {
            return Ok(());
        }
        let runtime = Runtime::start(&self.config, self.paths.clone(), self.hook.clone()).await?;
        self.runtime = Some(runtime);
        Ok(())
    }

    pub async fn stop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown().await;
        }
    }

    pub async fn add_site(&mut self, draft: SiteDraft) -> Result<Site> {
        if !draft.path.exists() {
            bail!("folder {} tidak ada", draft.path.display());
        }

        // Penanda PHP hanya petunjuk, bukan syarat. Folder statis, project
        // Bun/Node, atau folder kosong tetap boleh dijadikan situs.
        let found = detect::detect(&draft.path);

        let (framework, document_root) = match &found {
            Some(project) => (
                draft
                    .framework
                    .clone()
                    .unwrap_or_else(|| project.framework.to_string()),
                project.document_root.clone(),
            ),
            None => (
                draft
                    .framework
                    .clone()
                    .unwrap_or_else(|| "lain".to_string()),
                draft.path.clone(),
            ),
        };

        let id = match draft.id.clone() {
            Some(id) if !id.trim().is_empty() => slug(&id),
            _ => self.unique_id(&slug(&folder_name(&draft.path))),
        };

        let name = draft
            .name
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| folder_name(&draft.path));

        let domain = draft
            .domain
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("{id}.{}", self.config.tld));

        let php = match draft.php.clone().filter(|value| !value.is_empty()) {
            Some(php) => php,
            None => self.default_php()?,
        };

        if self.config.sites.iter().any(|site| site.domain == domain) {
            bail!("domain {domain} sudah dipakai site lain");
        }

        let site = Site {
            id,
            name,
            path: document_root,
            domain,
            php,
            root: Some(
                found
                    .as_ref()
                    .map(|project| project.project_root.clone())
                    .unwrap_or_else(|| draft.path.clone()),
            ),
            framework: Some(framework),
            backend: None,
            enabled: true,
        };

        self.config.sites.push(site.clone());
        self.config.save(&self.paths)?;
        self.refresh().await?;
        Ok(site)
    }

    pub async fn remove_site(&mut self, id: &str) -> Result<()> {
        let before = self.config.sites.len();
        self.config.sites.retain(|site| site.id != id);
        if self.config.sites.len() == before {
            bail!("site {id} tidak ditemukan");
        }
        self.config.save(&self.paths)?;
        self.refresh().await
    }

    /// Berkas php.ini satu versi. Dibuat kalau belum ada, karena folder hasil
    /// ekstraksi manual bisa datang tanpa berkas ini.
    pub fn php_ini_path(&self, version: &str) -> Result<PathBuf> {
        let manager = crate::php::PhpManager::new(self.paths.clone());
        if !manager.installed().iter().any(|item| item == version) {
            bail!("PHP {version} belum terpasang");
        }
        manager.ensure_ini(version)
    }

    /// php-cgi membaca php.ini hanya saat proses dimulai, jadi perubahan baru
    /// berlaku setelah pool versi itu dinyalakan ulang.
    pub async fn restart_php(&mut self, version: &str) -> Result<()> {
        let manager = crate::php::PhpManager::new(self.paths.clone());
        if !manager.installed().iter().any(|item| item == version) {
            bail!("PHP {version} belum terpasang");
        }
        let Some(runtime) = self.runtime.as_mut() else {
            bail!("service belum berjalan");
        };
        let children = self.config.php.children;
        runtime
            .restart_version(version, children, &self.paths, self.hook.clone())
            .await?;
        Ok(())
    }

    /// Hapus satu versi PHP. Situs yang memakainya dinonaktifkan, bukan
    /// dipindah diam-diam ke versi lain.
    pub async fn remove_php(&mut self, version: &str) -> Result<PhpRemoval> {
        let manager = crate::php::PhpManager::new(self.paths.clone());
        let installed = manager.installed();
        if !installed.iter().any(|item| item == version) {
            bail!("PHP {version} belum terpasang");
        }

        let path_status = crate::path_env::status(&self.paths);
        if path_status.registered && path_status.shim_version.as_deref() == Some(version) {
            bail!(
                "PHP {version} dipakai shim php di PATH. Pakai hapus semua PHP kalau memang ingin dilepas."
            );
        }

        let remaining: Vec<String> = installed
            .iter()
            .filter(|item| item.as_str() != version)
            .cloned()
            .collect();

        let mut disabled_sites = Vec::new();
        for site in &mut self.config.sites {
            if site.php == version && site.enabled {
                site.enabled = false;
                disabled_sites.push(site.name.clone());
            }
        }

        if self.config.php.active == version {
            self.config.php.active = remaining.last().cloned().unwrap_or_default();
        }

        self.config.save(&self.paths)?;
        self.refresh().await?;
        remove_php_dir(&self.paths, version)?;

        let composer_moved = self.repoint_composer(version);
        if path_status.registered
            && let Ok(target) = self.default_php()
            && !target.is_empty()
        {
            crate::path_env::sync_shim(&self.paths, &target)?;
        }

        Ok(PhpRemoval {
            version: Some(version.to_string()),
            disabled_sites,
            default_php: self.default_php().unwrap_or_default(),
            composer_moved,
            removed_all: false,
        })
    }

    /// Buang semua PHP, termasuk yang dipakai shim PATH. Semua situs
    /// dinonaktifkan karena tidak ada lagi yang bisa melayaninya.
    pub async fn remove_all_php(&mut self) -> Result<PhpRemoval> {
        let manager = crate::php::PhpManager::new(self.paths.clone());
        let installed = manager.installed();
        if installed.is_empty() {
            bail!("tidak ada PHP terpasang");
        }

        let mut disabled_sites = Vec::new();
        for site in &mut self.config.sites {
            if site.enabled {
                site.enabled = false;
                disabled_sites.push(site.name.clone());
            }
        }
        self.config.php.active.clear();

        self.config.save(&self.paths)?;
        self.refresh().await?;
        for version in &installed {
            remove_php_dir(&self.paths, version)?;
        }
        crate::path_env::unregister(&self.paths)?;

        Ok(PhpRemoval {
            version: None,
            disabled_sites,
            default_php: String::new(),
            composer_moved: false,
            removed_all: true,
        })
    }

    /// Composer mencatat jalur php.exe saat dipasang. Kalau versi itu dihapus,
    /// penunjuknya dipindah ke default baru supaya Composer tetap jalan.
    fn repoint_composer(&self, removed: &str) -> bool {
        let marker = crate::composer::php_marker(&self.paths);
        let Ok(text) = std::fs::read_to_string(&marker) else {
            return false;
        };
        let recorded = PathBuf::from(text.trim());
        if !recorded.starts_with(self.paths.php_dir(removed)) {
            return false;
        }
        let Ok(version) = self.default_php() else {
            return false;
        };
        let php = self.paths.php_dir(&version).join("php.exe");
        crate::composer::set_php(&self.paths, &php).is_ok()
    }

    /// Menyiapkan datadir MariaDB. Dipanggil setelah binari selesai diekstrak,
    /// dan aman diulang.
    pub async fn prepare_database(&mut self) -> Result<()> {
        let backend = Mariadb::new(self.paths.clone());
        if !backend.is_installed() {
            bail!("MariaDB belum terpasang");
        }

        backend.initialize(self.config.database.port)?;
        if !self.paths.mariadb_password().exists() {
            backend.store_password("")?;
        }

        if self.config.database.autostart && self.runtime.is_some() {
            self.start_database().await?;
        }
        Ok(())
    }

    pub async fn start_database(&mut self) -> Result<()> {
        let backend = Mariadb::new(self.paths.clone());
        if !backend.is_installed() {
            bail!("MariaDB belum terpasang");
        }

        // Binari bisa datang tanpa datadir, misalnya setelah folder data
        // dibersihkan. Menyiapkan di sini membuat tombol Mulai selalu bekerja.
        backend.initialize(self.config.database.port)?;
        if !self.paths.mariadb_password().exists() {
            backend.store_password("")?;
        }

        let Some(runtime) = self.runtime.as_mut() else {
            bail!("service belum berjalan");
        };
        let port = self.config.database.port;
        runtime
            .ensure_database(port, &self.paths, self.hook.clone())
            .await?;
        Ok(())
    }

    pub async fn stop_database(&mut self) -> Result<()> {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.stop_database().await;
        }
        Ok(())
    }

    /// Ceklis autostart menentukan apakah database ikut hidup dan mati bersama
    /// service lain.
    pub async fn set_database_autostart(&mut self, enabled: bool) -> Result<()> {
        self.config.database.autostart = enabled;
        self.config.save(&self.paths)?;

        if enabled {
            if self.runtime.is_some() {
                self.start_database().await?;
            }
        } else {
            self.stop_database().await?;
        }
        Ok(())
    }

    /// Mengubah password root di semua akun root. Kalau catatan password DevKu
    /// sudah basi, server dinyalakan ulang tanpa grant table sebentar.
    pub async fn reset_database_password(&mut self, password: &str) -> Result<()> {
        let backend = Mariadb::new(self.paths.clone());
        if !backend.is_installed() || !backend.is_initialized() {
            bail!("database belum siap");
        }
        if self.runtime.is_none() {
            bail!("service belum berjalan");
        }

        let port = self.config.database.port;
        let current = backend.password();
        let direct = backend.set_password(port, &current, password).is_ok();

        if !direct {
            let recovery = backend.recovery_spec(port)?;
            let hook = self.hook.clone();
            self.runtime
                .as_mut()
                .expect("runtime sudah dicek")
                .start_database_spec(recovery, hook)
                .await?;

            let result = backend.set_password(port, "", password);

            if let Ok(spec) = backend.spec(port) {
                let hook = self.hook.clone();
                let _ = self
                    .runtime
                    .as_mut()
                    .expect("runtime masih ada")
                    .start_database_spec(spec, hook)
                    .await;
            }

            result?;
        }

        backend.store_password(password)?;
        Ok(())
    }

    /// Backup semua database buatan user ke berkas .sql bertanggal.
    pub async fn backup_database(&mut self) -> Result<PathBuf> {
        let backend = Mariadb::new(self.paths.clone());
        if !backend.is_installed() || !backend.is_initialized() {
            bail!("database belum siap");
        }
        if self.runtime.is_none() {
            bail!("service belum berjalan");
        }

        let running = self
            .runtime
            .as_ref()
            .map(|runtime| runtime.database_running())
            .unwrap_or(false);
        if !running {
            self.start_database().await?;
        }

        let port = self.config.database.port;
        let password = backend.password();
        let dest = self
            .paths
            .backups()
            .join(format!("mariadb-{}.sql", timestamp()));
        let target = dest.clone();

        tokio::task::spawn_blocking(move || backend.dump(port, &password, &target))
            .await
            .map_err(|err| anyhow::anyhow!("backup terhenti: {err}"))??;

        Ok(dest)
    }

    /// Menghapus binari MariaDB. Data hanya ikut terhapus kalau diminta.
    pub async fn remove_database(&mut self, remove_data: bool) -> Result<DatabaseRemoval> {
        let backend = Mariadb::new(self.paths.clone());
        if !backend.is_installed() && !backend.is_initialized() {
            bail!("MariaDB belum terpasang");
        }

        if let Some(runtime) = self.runtime.as_mut() {
            runtime.stop_database().await;
        }

        remove_dir_retry(&self.paths.mariadb_dir())?;
        let _ = std::fs::remove_file(self.paths.mariadb_password());
        let _ = std::fs::remove_file(self.paths.mariadb_config());

        if remove_data {
            backend.wipe()?;
        }

        Ok(DatabaseRemoval {
            removed_data: remove_data,
        })
    }

    pub async fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<()> {
        let site = self
            .config
            .sites
            .iter_mut()
            .find(|site| site.id == id)
            .with_context(|| format!("site {id} tidak ditemukan"))?;
        site.enabled = enabled;
        self.config.save(&self.paths)?;
        self.refresh().await
    }

    /// Mengganti versi PHP satu situs. Kalau runtime hidup, pool versi baru
    /// dinyalakan dan yang lama dimatikan lewat `refresh`.
    pub async fn set_php(&mut self, id: &str, php: &str) -> Result<()> {
        let installed = crate::php::PhpManager::new(self.paths.clone()).installed();
        if !installed.iter().any(|version| version == php) {
            bail!("PHP {php} belum terpasang");
        }

        let site = self
            .config
            .sites
            .iter_mut()
            .find(|site| site.id == id)
            .with_context(|| format!("site {id} tidak ditemukan"))?;
        if site.php == php {
            return Ok(());
        }

        site.php = php.to_string();
        self.config.save(&self.paths)?;
        self.refresh().await
    }

    /// Versi default untuk situs baru dan untuk shim `php` di PATH.
    pub fn set_default_php(&mut self, php: &str) -> Result<()> {
        let installed = crate::php::PhpManager::new(self.paths.clone()).installed();
        if !installed.iter().any(|version| version == php) {
            bail!("PHP {php} belum terpasang");
        }

        self.config.php.active = php.to_string();
        self.config.save(&self.paths)?;
        crate::path_env::sync_shim(&self.paths, php)?;
        Ok(())
    }

    /// Setelah `caddy trust` sukses. Disimpan bukan sebagai bukti live, tapi
    /// sebagai riwayat yang UI pakai untuk menampilkan statusnya.
    pub fn mark_https_trusted(&mut self) -> Result<()> {
        self.config.https_trusted = true;
        self.config.save(&self.paths)?;
        Ok(())
    }

    /// First-run ditutup, baik karena selesai maupun dilewati. Disimpan di
    /// config supaya panel panduan tidak muncul lagi di peluncuran berikutnya.
    pub fn mark_onboarded(&mut self) -> Result<()> {
        if self.config.onboarded {
            return Ok(());
        }

        self.config.onboarded = true;
        self.config.save(&self.paths)?;
        Ok(())
    }

    pub fn export_config(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(&self.config)?)
    }

    /// Impor config dari mesin lain. Bagian yang terikat mesin ini tidak ikut
    /// disalin mentah: kepercayaan CA tetap milik mesin ini, folder project
    /// yang tidak ada dilewati, dan situs dengan PHP belum terpasang dibuat
    /// nonaktif supaya pool tidak gagal dinyalakan.
    pub async fn import_config(&mut self, text: &str) -> Result<ImportReport> {
        let mut next: Config = serde_json::from_str(text).context("config tidak valid")?;
        let installed = crate::php::PhpManager::new(self.paths.clone()).installed();

        let mut skipped = Vec::new();
        next.sites.retain(|site| {
            let exists = site.path.exists();
            if !exists {
                skipped.push(site.name.clone());
            }
            exists
        });

        let mut disabled = Vec::new();
        for site in &mut next.sites {
            if !installed.iter().any(|version| version == &site.php) {
                site.enabled = false;
                disabled.push(format!("{} (PHP {})", site.name, site.php));
            }
        }

        let active_php_cleared = if !next.php.active.is_empty()
            && !installed.iter().any(|version| version == &next.php.active)
        {
            let cleared = next.php.active.clone();
            next.php.active.clear();
            Some(cleared)
        } else {
            None
        };

        next.https_trusted = self.config.https_trusted;
        next.onboarded = true;
        next.validate()?;

        self.config = next;
        self.config.save(&self.paths)?;
        self.refresh().await?;

        Ok(ImportReport {
            imported: self.config.sites.len(),
            skipped,
            disabled,
            active_php_cleared,
        })
    }

    /// Setelah config berubah: nyalakan versi PHP yang baru dipakai, matikan
    /// yang tidak lagi dipakai, lalu reload Caddy. Kalau runtime mati, cukup
    /// simpan config.
    async fn refresh(&mut self) -> Result<()> {
        let Some(runtime) = self.runtime.as_mut() else {
            return Ok(());
        };

        let versions = self.config.php_versions_in_use();
        let children = self.config.php.children;

        for version in &versions {
            runtime
                .ensure_version(version, children, &self.paths, self.hook.clone())
                .await?;
        }

        let in_use: BTreeSet<String> = versions.into_iter().collect();
        runtime.stop_unused(&in_use).await?;
        runtime.reload(&self.config, self.paths.clone()).await?;
        Ok(())
    }

    pub fn tail_log(&self, name: &str, lines: usize) -> Result<Vec<String>> {
        let path = self.paths.logs().join(format!("{name}.log"));
        if !path.exists() {
            return Ok(Vec::new());
        }

        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("gagal membaca {}", path.display()))?;
        let all: Vec<&str> = raw.lines().collect();
        let start = all.len().saturating_sub(lines);
        Ok(all[start..].iter().map(|line| line.to_string()).collect())
    }

    fn default_php(&self) -> Result<String> {
        let installed = crate::php::PhpManager::new(self.paths.clone()).installed();
        if installed.is_empty() {
            bail!("belum ada PHP terpasang; pasang satu versi dulu");
        }
        if !self.config.php.active.is_empty() && installed.contains(&self.config.php.active) {
            return Ok(self.config.php.active.clone());
        }
        Ok(installed[installed.len() - 1].clone())
    }

    fn unique_id(&self, base: &str) -> String {
        if !self.config.sites.iter().any(|site| site.id == base) {
            return base.to_string();
        }
        for n in 2..1000 {
            let candidate = format!("{base}-{n}");
            if !self.config.sites.iter().any(|site| site.id == candidate) {
                return candidate;
            }
        }
        format!("{base}-{}", self.config.sites.len() + 1)
    }
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("site")
        .to_string()
}

/// Windows kadang masih memegang handle sebentar setelah proses mati, jadi
/// penghapusan dicoba beberapa kali sebelum menyerah.
fn remove_php_dir(paths: &Paths, version: &str) -> Result<()> {
    remove_dir_retry(&paths.php_dir(version))
}

fn remove_dir_retry(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    let mut last: Option<std::io::Error> = None;
    for attempt in 0..5 {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last = Some(err);
                if attempt < 4 {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            }
        }
    }

    let err = last.expect("setiap percobaan mengisi galat terakhir");
    Err(anyhow::Error::from(err)).with_context(|| format!("gagal menghapus {}", dir.display()))
}

/// Nama berkas backup memakai waktu lokal mesin, bukan UTC, supaya mudah dicari.
#[cfg(windows)]
fn timestamp() -> String {
    use windows::Win32::Foundation::SYSTEMTIME;
    use windows::Win32::System::SystemInformation::GetLocalTime;

    let now: SYSTEMTIME = unsafe { GetLocalTime() };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond
    )
}

#[cfg(not(windows))]
fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0);
    secs.to_string()
}

/// Id dipakai sebagai nama file log dan bagian domain, jadi dibatasi ke huruf,
/// angka, dan tanda hubung.
pub fn slug(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;

    for ch in input.chars().flat_map(|ch| ch.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }

    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "site".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_cleans_folder_names() {
        assert_eq!(slug("App A"), "app-a");
        assert_eq!(slug("my_project"), "my-project");
        assert_eq!(slug("--My Project!!"), "my-project");
        assert_eq!(slug("sudah-ok"), "sudah-ok");
        assert_eq!(slug("!!!"), "site");
    }

    #[tokio::test]
    async fn site_php_can_be_switched_and_missing_version_is_rejected() {
        let root = std::env::temp_dir().join(format!("devku-session-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths { root: root.clone() };
        paths.ensure().unwrap();

        for version in ["7.4.33", "8.3.36"] {
            let dir = paths.php_dir(version);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("php-cgi.exe"), b"").unwrap();
        }

        let mut config = Config::default();
        config.sites.push(Site {
            id: "app".to_string(),
            name: "App".to_string(),
            path: root.join("app"),
            domain: "app.localhost".to_string(),
            php: "8.3.36".to_string(),
            root: Some(root.join("app")),
            framework: None,
            backend: None,
            enabled: true,
        });
        config.save(&paths).unwrap();

        let mut session = Session::new(paths.clone(), None).unwrap();

        session.set_php("app", "7.4.33").await.unwrap();
        assert_eq!(session.config().sites[0].php, "7.4.33");

        let err = session.set_php("app", "5.6.9").await.unwrap_err();
        assert!(err.to_string().contains("belum terpasang"), "{err}");

        session.set_default_php("7.4.33").unwrap();
        assert_eq!(session.config().php.active, "7.4.33");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn removing_php_disables_the_sites_that_used_it() {
        let root = std::env::temp_dir().join(format!("devku-remove-php-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths { root: root.clone() };
        paths.ensure().unwrap();

        for version in ["8.2.0", "8.3.0"] {
            let dir = paths.php_dir(version);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("php-cgi.exe"), b"").unwrap();
        }

        let project = root.join("app");
        std::fs::create_dir_all(&project).unwrap();

        let mut config = Config::default();
        config.php.active = "8.2.0".to_string();
        config.sites.push(Site {
            id: "app".to_string(),
            name: "App".to_string(),
            path: project.clone(),
            domain: "app.localhost".to_string(),
            php: "8.2.0".to_string(),
            root: Some(project),
            framework: None,
            backend: None,
            enabled: true,
        });
        config.save(&paths).unwrap();

        let mut session = Session::new(paths.clone(), None).unwrap();
        let removal = session.remove_php("8.2.0").await.unwrap();

        assert_eq!(removal.disabled_sites, vec!["App".to_string()]);
        assert!(!removal.removed_all);
        assert!(
            !paths.php_dir("8.2.0").exists(),
            "folder versi yang dihapus harus hilang"
        );
        assert!(paths.php_dir("8.3.0").exists(), "versi lain tidak disentuh");
        assert!(!session.config().sites[0].enabled);
        assert_eq!(session.config().php.active, "8.3.0");
        assert_eq!(removal.default_php, "8.3.0");

        let _ = std::fs::remove_dir_all(&root);
    }
}
