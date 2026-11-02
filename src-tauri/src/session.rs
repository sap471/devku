use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{Backend, Config, Site};
use crate::database::{DatabaseBackend, Mariadb};
use crate::detect;
use crate::paths::Paths;
use crate::proc::ServiceState;
use crate::runtime::{Hook, Runtime};

/// Add-site request from the UI. Everything is optional except the folder, because
/// scanning the folder can fill in the rest.
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
    /// "caddy" or "apache" for this site. Both are reached through Caddy
    /// on 80/443; Apache is only its internal backend.
    pub backend: String,
    /// Public site URL, never includes a port.
    pub url: String,
    pub enabled: bool,
    pub path: String,
    /// Project root, used by the terminal. For Laravel it differs from `path`.
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

#[derive(Clone, Debug, Serialize)]
pub struct PhpExtensionView {
    pub name: String,
    pub enabled: bool,
    /// A true value means the php.ini directive uses `zend_extension=`.
    pub zend: bool,
    /// The php_*.dll file lives in the ext folder. An entry with no dll behind it must not be switchable on.
    pub available: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PhpExtensionsView {
    pub version: String,
    pub ini_path: String,
    pub entries: Vec<PhpExtensionView>,
    /// The php-cgi pool for this version is up: a change only takes effect after
    /// the pool is restarted.
    pub running: bool,
}

/// Summary of one PHP removal, so the UI can say what was affected
/// instead of only "berhasil".
#[derive(Clone, Debug, Serialize)]
pub struct PhpRemoval {
    /// The removed version. Empty means every version at once.
    pub version: Option<String>,
    pub disabled_sites: Vec<String>,
    pub default_php: String,
    pub composer_moved: bool,
    pub removed_all: bool,
}

/// Local database server state. Enough to render the panel without calling
/// other commands.
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
    /// Config note: Caddy's local CA was once trusted through DevKu.
    pub https_trusted: bool,
    /// First run has completed or been skipped.
    pub onboarded: bool,
    pub installed_php: Vec<String>,
    /// Version used for new sites and for the `php` shim on PATH.
    pub default_php: String,
    pub php_path: crate::path_env::PathStatus,
    pub composer_installed: bool,
    pub composer_version: Option<String>,
    /// PHP the Composer shim currently uses.
    pub composer_php: Option<String>,
    pub composer_php_missing: bool,
    /// Composer global package folder, where `laravel` and other tools live.
    pub composer_global: crate::composer::GlobalStatus,
    /// Config location in use, so the UI can name it.
    pub config_path: String,
    /// DevKu data folder: config, engines, runtime, and logs.
    pub home_path: String,
    /// Project parent folder for the wizard and the open button.
    pub workspace: WorkspaceView,
    /// Where the data folder came from, to explain how the value was resolved.
    pub data_location: DataLocationView,
    /// Registry Run value for the binary that is running now, not just
    /// a config note: the user can remove it through Task Manager.
    pub start_at_login: bool,
    pub sites: Vec<SiteView>,
    pub services: Vec<ServiceView>,
    /// Local database server, including when it is not installed.
    pub database: DatabaseView,
    /// Optional Apache backend, always present so the UI can show
    /// its state even before it is configured.
    pub apache: ApacheView,
    /// Stored engine archives for reinstalling.
    pub downloads: DownloadsView,
}

/// Apache state: installed version, port, and httpd status.
#[derive(Clone, Debug, Serialize)]
pub struct ApacheView {
    /// Installation folder in use, null when not installed.
    pub root: Option<String>,
    /// Newest folder version under `engines/apache`, for example `2.4.69-261002`.
    pub version: Option<String>,
    pub installed: bool,
    pub port: u16,
    /// Service status while Apache is running, "stopped" when unused.
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DownloadsView {
    pub files: usize,
    pub bytes: u64,
}

/// Project parent folder. The wizard uses it as a starting point and the open button
/// uses it to open folders, not as a site list: sites are still registered explicitly.
#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceView {
    pub path: String,
    /// true if the user picked it, false if it is still the default.
    pub custom: bool,
    pub exists: bool,
}

/// Where the data folder lives. The pointer is split from `home_path` so
/// the UI can explain the value origin and refuse changes while it is locked
/// by the environment.
#[derive(Clone, Debug, Serialize)]
pub struct DataLocationView {
    /// Default data folder, where the pointer file lives.
    pub default_home: String,
    /// Custom location pointer, null while data stays in the default folder.
    pub pointer: Option<String>,
    /// "default", "environment", "custom", or "invalid".
    pub source: String,
    /// `DEVKU_HOME` is set, so the location cannot be changed from the UI.
    pub locked: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ImportReport {
    pub imported: usize,
    /// Sites whose folder is missing on this machine.
    pub skipped: Vec<String>,
    /// Sites whose PHP is not installed, so they were disabled.
    pub disabled: Vec<String>,
    pub active_php_cleared: Option<String>,
}

/// Application state owner: config on disk plus a runtime that may be up.
/// Every UI command goes through here, so no path touches the
/// supervisor directly.
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

        let apache_service = services
            .iter()
            .find(|item| item.name == "apache")
            .map(|item| (item.status.clone(), item.detail.clone()));

        let sites = self
            .config
            .sites
            .iter()
            .map(|site| {
                let state = php_status
                    .iter()
                    .find(|(version, _)| version == &site.php)
                    .map(|(_, state)| state.clone());
                let backend = self.config.backend_of(site);

                let (status, detail) = match (&self.runtime, state) {
                    (None, _) => ("stopped".to_string(), None),
                    (Some(_), Some(state)) if site.enabled => {
                        // An Apache site follows httpd: if httpd is not ready
                        // or has failed, a "ready" PHP status is misleading.
                        match (&apache_service, backend) {
                            (Some((status, detail)), Backend::Apache) if status != "ready" => {
                                (status.clone(), detail.clone())
                            }
                            _ => (state.tag().to_string(), state.detail()),
                        }
                    }
                    (Some(_), _) => ("stopped".to_string(), None),
                };

                SiteView {
                    id: site.id.clone(),
                    name: site.name.clone(),
                    domain: site.domain.clone(),
                    php: site.php.clone(),
                    framework: site.framework.clone().unwrap_or_else(|| "php".to_string()),
                    backend: match backend {
                        Backend::Caddy => "caddy".to_string(),
                        Backend::Apache => "apache".to_string(),
                    },
                    url: self.config.site_url(site),
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

        let apache_backend = crate::apache::ApacheBackend::from_config(&self.paths, &self.config);
        let apache = ApacheView {
            root: apache_backend
                .root()
                .map(|path| path.to_string_lossy().to_string()),
            version: apache_backend.version(),
            installed: apache_backend.is_installed(),
            port: self.config.apache.port,
            status: apache_service
                .as_ref()
                .map(|(status, _)| status.clone())
                .unwrap_or_else(|| "stopped".to_string()),
            detail: apache_service
                .as_ref()
                .and_then(|(_, detail)| detail.clone()),
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
            workspace: {
                let dir = self.config.workspace_dir();
                WorkspaceView {
                    exists: dir.is_dir(),
                    path: dir.to_string_lossy().to_string(),
                    custom: self.config.workspace.is_some(),
                }
            },
            data_location: {
                let default_home = crate::paths::default_home();
                let (_, source) =
                    crate::location::resolve(&default_home, crate::location::env_home());
                let pointer = crate::location::pointer_file(&default_home);
                DataLocationView {
                    pointer: pointer
                        .is_file()
                        .then(|| pointer.to_string_lossy().to_string()),
                    default_home: default_home.to_string_lossy().to_string(),
                    source: source.tag().to_string(),
                    locked: source == crate::location::Source::Env,
                }
            },
            start_at_login: crate::autostart::is_enabled(&crate::autostart::running_exe()),
            sites,
            services,
            database,
            apache,
            downloads: {
                let (files, bytes) = crate::download::cached_archives(&self.paths);
                DownloadsView { files, bytes }
            },
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

        // A PHP marker is only a hint, not a requirement. A static folder, a
        // Bun/Node project, or an empty folder may still become a site.
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

    /// Switch one site's backend. Apache must be configured first so
    /// the site is not pointed at a port with no listener.
    pub async fn set_site_backend(&mut self, id: &str, backend: Backend) -> Result<()> {
        if backend == Backend::Apache {
            let apache = crate::apache::ApacheBackend::from_config(&self.paths, &self.config);
            if !apache.is_installed() {
                bail!("Apache belum dipasang; pasang lewat Pengaturan > Apache");
            }
        }

        let Some(site) = self.config.sites.iter_mut().find(|site| site.id == id) else {
            bail!("site {id} tidak ditemukan");
        };
        site.backend = Some(backend);
        self.config.save(&self.paths)?;
        self.refresh().await
    }

    /// Apache internal port, separate from Caddy's 80/443 and 2019. Caddy
    /// forwards Apache sites to this port, so the value never appears
    /// in a URL. Colliding values are rejected so startup does not fail.
    pub async fn set_apache_port(&mut self, port: u16) -> Result<()> {
        if port < 1024 {
            bail!("port Apache harus 1024 atau lebih");
        }
        if matches!(port, 80 | 443 | 2019) {
            bail!("port {port} dipakai Caddy; pilih port lain untuk Apache");
        }

        self.config.apache.port = port;
        self.config.save(&self.paths)?;
        self.refresh().await
    }

    /// Sets the project parent folder. `None` returns it to the default
    /// location. The folder is created now so a UI choice never points
    /// at a place that does not exist.
    pub async fn set_workspace(&mut self, path: Option<String>) -> Result<()> {
        let chosen = path
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        match chosen {
            Some(text) => {
                let dir = PathBuf::from(&text);
                if !dir.is_absolute() {
                    bail!("folder workspace harus jalur absolut");
                }
                std::fs::create_dir_all(&dir)
                    .with_context(|| format!("gagal membuat {}", dir.display()))?;
                self.config.workspace = Some(dir);
            }
            None => self.config.workspace = None,
        }

        self.config.save(&self.paths)
    }

    /// Stops without touching other services; used before reinstalling
    /// Apache because a running httpd.exe cannot be overwritten.
    pub async fn stop_apache(&mut self) {        if let Some(runtime) = self.runtime.as_mut() {
            runtime.stop_apache().await;
        }
    }

    /// The php.ini for one version. Created when missing, because a folder from
    /// a manual extraction can arrive without this file.
    pub fn php_ini_path(&self, version: &str) -> Result<PathBuf> {
        let manager = crate::php::PhpManager::new(self.paths.clone());
        if !manager.installed().iter().any(|item| item == version) {
            bail!("PHP {version} belum terpasang");
        }
        manager.ensure_ini(version)
    }

    /// php-cgi reads php.ini only at process start, so a change only
    /// takes effect after that version's pool is restarted.
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

    /// Extension list for one version: two sources (dll and php.ini) merged.
    pub async fn php_extensions(&self, version: &str) -> Result<PhpExtensionsView> {
        let manager = crate::php::PhpManager::new(self.paths.clone());
        if !manager.installed().iter().any(|item| item == version) {
            bail!("PHP {version} belum terpasang");
        }
        manager.ensure_ini(version)?;
        let ini = manager.ini_path(version);

        Ok(PhpExtensionsView {
            version: version.to_string(),
            ini_path: ini.display().to_string(),
            entries: manager
                .extensions(version)?
                .into_iter()
                .map(|entry| PhpExtensionView {
                    name: entry.name,
                    enabled: entry.enabled,
                    zend: entry.zend,
                    available: entry.dll,
                })
                .collect(),
            running: self.php_running(version).await,
        })
    }

    /// Enable or disable one extension, then return the fresh list. The pool is not
    /// restarted automatically: the UI states the need for a restart from the view.
    pub async fn set_php_extension(
        &mut self,
        version: &str,
        name: &str,
        enable: bool,
    ) -> Result<PhpExtensionsView> {
        let manager = crate::php::PhpManager::new(self.paths.clone());
        if !manager.installed().iter().any(|item| item == version) {
            bail!("PHP {version} belum terpasang");
        }
        manager.set_extension(version, name, enable)?;
        self.php_extensions(version).await
    }

    async fn php_running(&self, version: &str) -> bool {
        let name = format!("php-{version}");
        self.snapshot().await.services.iter().any(|service| {
            service.name == name
                && matches!(service.status.as_str(), "starting" | "ready" | "backoff")
        })
    }

    /// Delete one PHP version. Sites using it are disabled rather than
    /// silently moved to another version.
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

    /// Remove every PHP, including the one the PATH shim uses. Every site
    /// is disabled because nothing is left to serve them.
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

    /// Composer records the php.exe path at install time. If that version is deleted,
    /// its pointer moves to the new default so Composer keeps working.
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

    /// Prepares the MariaDB datadir. Called after the binaries are extracted,
    /// and safe to repeat.
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

        // Binaries can arrive without a datadir, for example after the data folder
        // was cleaned. Preparing here keeps the Mulai button working.
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

    /// The autostart checkbox decides whether the database starts and stops with
    /// the other services.
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

    /// Changes the root password on every root account. If the DevKu password note
    /// is stale, the server is restarted briefly without the grant tables.
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

    /// Back up every user-created database to a dated .sql file.
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

    /// Deletes the MariaDB binaries. Data is deleted only when requested.
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

    /// Switches one site's PHP version. If the runtime is up, the new version's pool
    /// is started and the old one stopped through `refresh`.
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

    /// Default version for new sites and for the `php` shim on PATH.
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

    /// After `caddy trust` succeeds. Stored not as live proof but
    /// as history the UI uses to show the status.
    pub fn mark_https_trusted(&mut self) -> Result<()> {
        self.config.https_trusted = true;
        self.config.save(&self.paths)?;
        Ok(())
    }

    /// First run closed, whether finished or skipped. Stored in
    /// config so the guide panel does not appear on the next launch.
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

    /// Import config from another machine. Parts bound to this machine are not
    /// copied verbatim: CA trust stays with this machine, project folders
    /// that are missing are skipped, and sites with uninstalled PHP are made
    /// inactive so the pool does not fail to start.
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

    /// After config changes: start the newly used PHP version, stop
    /// the one no longer used, then reload Caddy and Apache. If the runtime
    /// is down, saving the config is enough.
    pub(crate) async fn refresh(&mut self) -> Result<()> {
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
        match runtime
            .apply_apache(&self.config, &self.paths, self.hook.clone())
            .await
        {
            Ok(()) => runtime.clear_apache_error(),
            Err(err) => runtime.set_apache_error(format!("{err:#}")),
        }
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

/// Windows sometimes holds a handle briefly after a process dies, so
/// deletion is retried a few times before giving up.
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

/// Backup file names use the machine's local time, not UTC, so they are easy to find.
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

/// The id is used as a log file name and a domain part, so it is limited to letters,
/// digits, and hyphens.
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
