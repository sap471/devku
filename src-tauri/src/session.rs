use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{Config, Site};
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
    /// Nilai Run di registry untuk binary yang sedang berjalan, bukan sekadar
    /// catatan config: user bisa mencabutnya lewat Task Manager.
    pub start_at_login: bool,
    pub sites: Vec<SiteView>,
    pub services: Vec<ServiceView>,
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
            start_at_login: crate::autostart::is_enabled(&crate::autostart::running_exe()),
            sites,
            services,
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
}
