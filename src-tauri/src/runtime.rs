use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::caddy::{CaddyBackend, WebServerBackend};
use crate::config::Config;
use crate::paths::Paths;
use crate::php::PhpManager;
use crate::ports;
use crate::proc::{ServiceSpec, ServiceState, Supervisor};

pub const PHP_PORT_START: u16 = 9001;

/// Sekumpulan proses yang sedang melayani config tertentu. Drop = matikan semua,
/// jadi tidak ada jalur di mana php-cgi tertinggal hidup.
pub struct Runtime {
    pub ports: BTreeMap<String, u16>,
    php: Vec<Supervisor>,
    caddy: Supervisor,
}

pub type Hook = Arc<dyn Fn(&str, &ServiceState) + Send + Sync>;

impl Runtime {
    pub async fn start(config: &Config, paths: Paths, hook: Option<Hook>) -> Result<Self> {
        paths.ensure()?;
        config.validate()?;

        let manager = PhpManager::new(paths.clone());
        let backend = CaddyBackend::new(paths.clone());

        let versions = config.php_versions_in_use();
        if versions.is_empty() {
            anyhow::bail!("tidak ada site aktif; tidak ada yang perlu dijalankan");
        }

        let allocation = ports::allocate(&versions, PHP_PORT_START)?;
        let port_map: BTreeMap<String, u16> = allocation.into_iter().collect();

        let mut php = Vec::new();
        for (version, port) in &port_map {
            let spec = manager
                .spec(version, *port, config.php.children)
                .with_context(|| format!("menyiapkan PHP {version}"))?;
            tracing_note(&format!("php {version} -> 127.0.0.1:{port}"));
            php.push(Supervisor::start(spec, hook.clone())?);
        }

        for supervisor in &php {
            if let Err(reason) = supervisor.wait_ready(Duration::from_secs(20)).await {
                shutdown_all(php).await;
                anyhow::bail!("PHP gagal siap: {reason}");
            }
        }

        let caddyfile = backend.render(config, &port_map)?;
        crate::paths::write_atomic(&paths.caddyfile(), &caddyfile)?;

        let spec: ServiceSpec = backend.spec(config, &port_map)?;
        let caddy = Supervisor::start(spec, hook)?;
        if let Err(reason) = caddy.wait_ready(Duration::from_secs(20)).await {
            shutdown_all(php).await;
            anyhow::bail!("Caddy gagal siap: {reason}");
        }

        Ok(Self {
            ports: port_map,
            php,
            caddy,
        })
    }

    /// Reload config tanpa mematikan apa pun. Dipakai setelah site ditambah,
    /// diubah, atau dimatikan.
    pub async fn reload(&self, config: &Config, paths: Paths) -> Result<u16> {
        let backend = CaddyBackend::new(paths);
        backend.reload(config, &self.ports).await
    }

    pub async fn states(&self) -> Vec<(String, ServiceState)> {
        let mut out = vec![("caddy".to_string(), self.caddy.state().await)];
        for supervisor in &self.php {
            out.push((supervisor.name.clone(), supervisor.state().await));
        }
        out
    }

    pub async fn shutdown(self) {
        let Self { php, caddy, .. } = self;
        caddy.shutdown().await;
        shutdown_all(php).await;
    }
}

async fn shutdown_all(supervisors: Vec<Supervisor>) {
    for supervisor in supervisors {
        supervisor.shutdown().await;
    }
}

fn tracing_note(message: &str) {
    eprintln!("[devku] {message}");
}
