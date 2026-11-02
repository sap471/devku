use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::apache::ApacheBackend;
use crate::caddy::{CaddyBackend, WebServerBackend};
use crate::config::Config;
use crate::database::{DatabaseBackend, Mariadb};
use crate::paths::Paths;
use crate::php::PhpManager;
use crate::ports;
use crate::proc::{ServiceSpec, ServiceState, Supervisor};

pub const PHP_PORT_START: u16 = 9001;

/// The set of processes currently serving a given config. Drop = shut everything down,
/// so there is no path where php-cgi is left running.
pub struct Runtime {
    ports: BTreeMap<String, u16>,
    php: Vec<(String, Supervisor)>,
    caddy: Option<Supervisor>,
    apache: Option<Supervisor>,
    /// Last httpd.conf used by the live Apache process. Used
    /// to avoid a restart when the config has not changed.
    apache_config: Option<String>,
    apache_error: Option<String>,
    database: Option<Supervisor>,
    /// The database is installed and its datadir is ready, so it is worth showing
    /// as a service even while it is stopped.
    database_present: bool,
    database_error: Option<String>,
}

pub type Hook = Arc<dyn Fn(&str, &ServiceState) + Send + Sync>;

impl Runtime {
    pub async fn start(config: &Config, paths: Paths, hook: Option<Hook>) -> Result<Self> {
        paths.ensure()?;
        config.validate()?;

        // The runtime is useful even with no sites: Caddy serves the dashboard on
        // port 80, so the user sees something instead of a refused connection.
        let versions = config.php_versions_in_use();

        let allocation = ports::allocate(&versions, PHP_PORT_START)?;
        let port_map: BTreeMap<String, u16> = allocation.into_iter().collect();

        let mut runtime = Self {
            ports: BTreeMap::new(),
            php: Vec::new(),
            caddy: None,
            apache: None,
            apache_config: None,
            apache_error: None,
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
            // The database never blocks sites: failures are recorded and other
            // services keep running.
            runtime.database_error = Some(format!("{err:#}"));
        }

        for version in port_map.keys() {
            runtime
                .ensure_version(version, config.php.children, &paths, hook.clone())
                .await?;
        }

        let backend = CaddyBackend::new(paths.clone());
        backend.write_config(config, &runtime.ports).await?;

        // Apache first so Caddy, which forwards to its internal port, does not
        // hit a backend that is not ready. Its failure is recorded like the
        // database: Caddy sites keep running and the UI shows the failed service.
        if let Err(err) = runtime.apply_apache(config, &paths, hook.clone()).await {
            runtime.apache_error = Some(format!("{err:#}"));
        }

        let spec: ServiceSpec = backend.spec(config, &runtime.ports)?;
        let caddy = Supervisor::start(spec, hook)?;
        if let Err(reason) = caddy.wait_ready(Duration::from_secs(20)).await {
            runtime.shutdown_php().await;
            runtime.stop_apache().await;
            anyhow::bail!("Caddy gagal siap: {reason}");
        }
        runtime.caddy = Some(caddy);

        Ok(runtime)
    }

    /// Starts the pool for one version if it is not running yet. Used when a new site
    /// uses a PHP version that is not running; other versions are untouched.
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

    /// Starts, stops, or restarts httpd to match the config. A restart is only
    /// performed when the generated httpd.conf changed, because Apache has no
    /// admin API like Caddy's.
    pub async fn apply_apache(
        &mut self,
        config: &Config,
        paths: &Paths,
        hook: Option<Hook>,
    ) -> Result<()> {
        if config.apache_sites().next().is_none() {
            self.stop_apache().await;
            return Ok(());
        }

        let backend = ApacheBackend::from_config(paths, config);
        let text = backend.render(config, &self.ports)?;
        if self.apache.is_some() && self.apache_config.as_deref() == Some(text.as_str()) {
            return Ok(());
        }

        self.stop_apache().await;
        backend.write_config(config, &self.ports)?;
        let supervisor = Supervisor::start(backend.spec()?, hook)?;
        if let Err(reason) = supervisor.wait_ready(Duration::from_secs(15)).await {
            supervisor.shutdown().await;
            anyhow::bail!("Apache gagal siap: {reason}");
        }
        self.apache = Some(supervisor);
        self.apache_config = Some(text);
        Ok(())
    }

    pub async fn stop_apache(&mut self) {
        if let Some(supervisor) = self.apache.take() {
            supervisor.shutdown().await;
        }
        self.apache_config = None;
    }

    pub fn set_apache_error(&mut self, reason: String) {
        self.apache_error = Some(reason);
    }

    pub fn clear_apache_error(&mut self) {
        self.apache_error = None;
    }

    /// Stops a pool that no site uses anymore.
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

    /// Restarts one pool on the same port. Used after php.ini
    /// is edited: php-cgi only reads configuration at process start, and an
    /// unchanged port means Caddy does not need a reload.
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

    /// Starts the database on the requested port. A conflicting port is reported
    /// as is rather than silently shifted, so `.env` does not break on its own.
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

    /// Runs the database spec as is, including the password recovery spec.
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

    /// Reloads config without stopping anything.
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
        if let Some(apache) = &self.apache {
            out.push(("apache".to_string(), apache.state().await));
        } else if let Some(reason) = &self.apache_error {
            out.push((
                "apache".to_string(),
                ServiceState::Failed {
                    reason: reason.clone(),
                },
            ));
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
            apache,
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
        if let Some(apache) = apache {
            apache.shutdown().await;
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
