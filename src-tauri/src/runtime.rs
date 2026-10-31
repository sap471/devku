use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::caddy::{CaddyBackend, WebServerBackend};
use crate::config::Config;
use crate::database::{DatabaseBackend, Mariadb};
use crate::paths::Paths;
use crate::php::PhpManager;
use crate::ports;
use crate::proc::{ServiceSpec, ServiceState, Supervisor};

pub const PHP_PORT_START: u16 = 9001;

/// Sekumpulan proses yang sedang melayani config tertentu. Drop = matikan semua,
/// jadi tidak ada jalur di mana php-cgi tertinggal hidup.
pub struct Runtime {
    ports: BTreeMap<String, u16>,
    php: Vec<(String, Supervisor)>,
    caddy: Option<Supervisor>,
    database: Option<Supervisor>,
    /// Database terpasang dan datadir-nya sudah siap, jadi layak ditampilkan
    /// sebagai service meski sedang berhenti.
    database_present: bool,
    database_error: Option<String>,
}

pub type Hook = Arc<dyn Fn(&str, &ServiceState) + Send + Sync>;

impl Runtime {
    pub async fn start(config: &Config, paths: Paths, hook: Option<Hook>) -> Result<Self> {
        paths.ensure()?;
        config.validate()?;

        // Tanpa site pun runtime tetap berguna: Caddy menyajikan dasbor di
        // port 80, jadi user melihat sesuatu alih-alih koneksi ditolak.
        let versions = config.php_versions_in_use();

        let allocation = ports::allocate(&versions, PHP_PORT_START)?;
        let port_map: BTreeMap<String, u16> = allocation.into_iter().collect();

        let mut runtime = Self {
            ports: BTreeMap::new(),
            php: Vec::new(),
            caddy: None,
            database: None,
            database_present: false,
            database_error: None,
        };

        let database = Mariadb::new(paths.clone());
        runtime.database_present = database.is_installed() && database.is_initialized();
        if runtime.database_present
            && config.database.autostart
            && let Err(err) = runtime
                .ensure_database(config.database.port, &paths, hook.clone())
                .await
        {
            // Database tidak menghalangi situs: kegagalan dicatat, service
            // lain tetap menyala.
            runtime.database_error = Some(format!("{err:#}"));
        }

        for version in port_map.keys() {
            runtime
                .ensure_version(version, config.php.children, &paths, hook.clone())
                .await?;
        }

        let backend = CaddyBackend::new(paths.clone());
        backend.write_config(config, &runtime.ports).await?;

        let spec: ServiceSpec = backend.spec(config, &runtime.ports)?;
        let caddy = Supervisor::start(spec, hook)?;
        if let Err(reason) = caddy.wait_ready(Duration::from_secs(20)).await {
            runtime.shutdown_php().await;
            anyhow::bail!("Caddy gagal siap: {reason}");
        }
        runtime.caddy = Some(caddy);

        Ok(runtime)
    }

    /// Menyalakan pool untuk satu versi kalau belum ada. Dipakai saat site baru
    /// memakai versi PHP yang belum berjalan; versi lain tidak terganggu.
    pub async fn ensure_version(
        &mut self,
        version: &str,
        children: u16,
        paths: &Paths,
        hook: Option<Hook>,
    ) -> Result<u16> {
        if let Some(port) = self.ports.get(version) {
            return Ok(*port);
        }

        let taken: BTreeSet<u16> = self.ports.values().copied().collect();
        let port = ports::first_free(PHP_PORT_START, &taken)?;

        let manager = PhpManager::new(paths.clone());
        let spec = manager
            .spec(version, port, children)
            .with_context(|| format!("menyiapkan PHP {version}"))?;

        let supervisor = Supervisor::start(spec, hook)?;
        if let Err(reason) = supervisor.wait_ready(Duration::from_secs(20)).await {
            supervisor.shutdown().await;
            anyhow::bail!("PHP {version} gagal siap: {reason}");
        }

        self.ports.insert(version.to_string(), port);
        self.php.push((version.to_string(), supervisor));
        Ok(port)
    }

    /// Hentikan pool yang tidak lagi dipakai site mana pun.
    pub async fn stop_unused(&mut self, in_use: &BTreeSet<String>) -> Result<()> {
        let mut keep = Vec::new();
        let mut stop = Vec::new();

        for (version, supervisor) in self.php.drain(..) {
            if in_use.contains(&version) {
                keep.push((version, supervisor));
            } else {
                stop.push((version, supervisor));
            }
        }
        self.php = keep;

        for (version, supervisor) in stop {
            supervisor.shutdown().await;
            self.ports.remove(&version);
        }
        Ok(())
    }

    /// Menyalakan ulang satu pool di port yang sama. Dipakai setelah php.ini
    /// diedit: php-cgi hanya membaca konfigurasi saat prosesnya dimulai, dan
    /// port yang tidak berubah berarti Caddy tidak perlu di-reload.
    pub async fn restart_version(
        &mut self,
        version: &str,
        children: u16,
        paths: &Paths,
        hook: Option<Hook>,
    ) -> Result<u16> {
        let previous = self.ports.remove(version);
        let mut keep = Vec::new();
        for (name, supervisor) in self.php.drain(..) {
            if name == version {
                supervisor.shutdown().await;
            } else {
                keep.push((name, supervisor));
            }
        }
        self.php = keep;

        let Some(port) = previous else {
            return self.ensure_version(version, children, paths, hook).await;
        };

        let manager = PhpManager::new(paths.clone());
        let spec = manager
            .spec(version, port, children)
            .with_context(|| format!("menyiapkan PHP {version}"))?;

        let supervisor = Supervisor::start(spec, hook)?;
        if let Err(reason) = supervisor.wait_ready(Duration::from_secs(20)).await {
            supervisor.shutdown().await;
            anyhow::bail!("PHP {version} gagal siap: {reason}");
        }

        self.ports.insert(version.to_string(), port);
        self.php.push((version.to_string(), supervisor));
        Ok(port)
    }

    /// Menyalakan database di port yang diminta. Port yang bentrok dilaporkan
    /// apa adanya, bukan digeser diam-diam, supaya `.env` tidak rusak sendiri.
    pub async fn ensure_database(
        &mut self,
        port: u16,
        paths: &Paths,
        hook: Option<Hook>,
    ) -> Result<u16> {
        let backend = Mariadb::new(paths.clone());
        if !backend.is_installed() || !backend.is_initialized() {
            anyhow::bail!("database belum siap");
        }
        if !ports::is_free(port) {
            anyhow::bail!("port {port} sudah dipakai proses lain");
        }

        let spec = backend.spec(port)?;
        self.start_database_spec(spec, hook).await?;
        self.database_present = true;
        Ok(port)
    }

    /// Menjalankan spec database apa adanya, termasuk spec pemulihan password.
    pub async fn start_database_spec(
        &mut self,
        spec: ServiceSpec,
        hook: Option<Hook>,
    ) -> Result<()> {
        self.stop_database().await;

        let supervisor = Supervisor::start(spec, hook)?;
        if let Err(reason) = supervisor.wait_ready(Duration::from_secs(30)).await {
            supervisor.shutdown().await;
            anyhow::bail!("database gagal siap: {reason}");
        }
        self.database = Some(supervisor);
        self.database_error = None;
        Ok(())
    }

    pub async fn stop_database(&mut self) {
        if let Some(supervisor) = self.database.take() {
            supervisor.shutdown().await;
        }
    }

    pub fn database_running(&self) -> bool {
        self.database.is_some()
    }

    /// Reload config tanpa mematikan apa pun.
    pub async fn reload(&self, config: &Config, paths: Paths) -> Result<u16> {
        let backend = CaddyBackend::new(paths);
        backend.reload(config, &self.ports).await
    }

    pub fn ports(&self) -> &BTreeMap<String, u16> {
        &self.ports
    }

    pub fn port_of(&self, version: &str) -> Option<u16> {
        self.ports.get(version).copied()
    }

    pub async fn states(&self) -> Vec<(String, ServiceState)> {
        let mut out = Vec::new();
        if let Some(caddy) = &self.caddy {
            out.push(("caddy".to_string(), caddy.state().await));
        }
        for (version, supervisor) in &self.php {
            out.push((format!("php-{version}"), supervisor.state().await));
        }
        if let Some(database) = &self.database {
            out.push(("mariadb".to_string(), database.state().await));
        } else if let Some(reason) = &self.database_error {
            out.push((
                "mariadb".to_string(),
                ServiceState::Failed {
                    reason: reason.clone(),
                },
            ));
        } else if self.database_present {
            out.push(("mariadb".to_string(), ServiceState::Stopped));
        }
        out
    }

    pub async fn shutdown(self) {
        let Self {
            caddy,
            php,
            database,
            ..
        } = self;
        if let Some(database) = database {
            database.shutdown().await;
        }
        if let Some(caddy) = caddy {
            caddy.shutdown().await;
        }
        for (_, supervisor) in php {
            supervisor.shutdown().await;
        }
    }

    async fn shutdown_php(&mut self) {
        for (_, supervisor) in self.php.drain(..) {
            supervisor.shutdown().await;
        }
    }
}
