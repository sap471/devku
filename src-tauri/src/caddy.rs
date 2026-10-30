use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;

use crate::api;
use crate::config::Config;
use crate::paths::{Paths, write_atomic};
use crate::proc::ServiceSpec;

/// Core tidak tahu backend mana yang dipakai. Semua yang dibutuhkan untuk
/// menjalankan sebuah situs lewat interface ini.
#[async_trait]
pub trait WebServerBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn render(&self, config: &Config, php_ports: &BTreeMap<String, u16>) -> Result<String>;
    fn spec(&self, config: &Config, php_ports: &BTreeMap<String, u16>) -> Result<ServiceSpec>;
    async fn reload(&self, config: &Config, php_ports: &BTreeMap<String, u16>) -> Result<u16>;
    async fn healthy(&self) -> bool;
}

pub struct CaddyBackend {
    paths: Paths,
}

impl CaddyBackend {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    pub fn binary(&self) -> PathBuf {
        self.paths.caddy_binary()
    }

    pub fn is_installed(&self) -> bool {
        self.binary().exists()
    }

    /// Dipakai Fase 2 saat unduhan selesai: taruh binary di `engines/caddy`.
    pub fn install_binary(&self, source: &Path) -> Result<PathBuf> {
        let target = self.binary();
        std::fs::create_dir_all(self.paths.caddy_dir())?;
        std::fs::copy(source, &target)
            .with_context(|| format!("gagal menyalin caddy ke {}", target.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&target)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&target, perms)?;
        }

        Ok(target)
    }
}

#[async_trait]
impl WebServerBackend for CaddyBackend {
    fn name(&self) -> &'static str {
        "caddy"
    }

    fn render(&self, config: &Config, php_ports: &BTreeMap<String, u16>) -> Result<String> {
        let sites: Vec<_> = config.served_sites().collect();
        if sites.is_empty() {
            return Ok(empty_caddyfile());
        }

        let mut out = String::from("{\n\tadmin localhost:2019\n\tauto_https off\n}\n");

        for site in sites {
            let port = php_ports.get(&site.php).with_context(|| {
                format!(
                    "site {} butuh PHP {}, tapi tidak ada port teralokasi",
                    site.id, site.php
                )
            })?;

            if !site.path.exists() {
                bail!("site {}: folder {} tidak ada", site.id, site.path.display());
            }

            let root = quote(&site.path);
            let log = quote(&self.paths.logs().join(format!("{}.log", site.id)));

            out.push_str(&format!(
                "\nhttp://{domain} {{\n\
                 \troot * {root}\n\
                 \tencode gzip\n\
                 \tphp_fastcgi 127.0.0.1:{port}\n\
                 \tfile_server\n\
                 \tlog {{\n\
                 \t\toutput file {log}\n\
                 \t\tformat console\n\
                 \t}}\n\
                 }}\n",
                domain = site.domain,
                root = root,
                port = port,
                log = log,
            ));
        }

        Ok(out)
    }

    fn spec(&self, _config: &Config, _php_ports: &BTreeMap<String, u16>) -> Result<ServiceSpec> {
        if !self.is_installed() {
            bail!(
                "Caddy belum terpasang di {}; jalankan `devku-cli install-caddy <path>`",
                self.binary().display()
            );
        }

        Ok(ServiceSpec {
            name: "caddy".to_string(),
            program: self.binary(),
            args: vec![
                "run".to_string(),
                "--config".to_string(),
                self.paths.caddyfile().to_string_lossy().to_string(),
                "--adapter".to_string(),
                "caddyfile".to_string(),
            ],
            env: vec![
                (
                    "XDG_DATA_HOME".to_string(),
                    self.paths.runtime().to_string_lossy().to_string(),
                ),
                (
                    "XDG_CONFIG_HOME".to_string(),
                    self.paths.runtime().to_string_lossy().to_string(),
                ),
            ],
            cwd: Some(self.paths.runtime()),
            port: Some(api::ADMIN_PORT),
            log_dir: self.paths.logs(),
        })
    }

    async fn reload(&self, config: &Config, php_ports: &BTreeMap<String, u16>) -> Result<u16> {
        let caddyfile = self.render(config, php_ports)?;
        write_atomic(&self.paths.caddyfile(), &caddyfile)?;
        api::post_caddyfile(&caddyfile).await
    }

    async fn healthy(&self) -> bool {
        api::ping().await
    }
}

fn empty_caddyfile() -> String {
    "{\n\tadmin localhost:2019\n\tauto_https off\n}\n".to_string()
}

/// Caddyfile memakai spasi sebagai pemisah, jadi path dengan spasi harus dikutip.
fn quote(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    format!("\"{text}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Backend, PhpSettings, Site};
    use std::collections::BTreeSet;

    fn sample_config() -> Config {
        Config {
            version: 1,
            backend: Backend::Caddy,
            tld: "localhost".to_string(),
            php: PhpSettings {
                installed: BTreeSet::new(),
                active: "8.3.36".to_string(),
                children: 4,
            },
            sites: vec![
                Site {
                    id: "app-a".to_string(),
                    name: "App A".to_string(),
                    path: std::env::temp_dir(),
                    domain: "app-a.localhost".to_string(),
                    php: "8.3.36".to_string(),
                    framework: Some("laravel".to_string()),
                    backend: None,
                    enabled: true,
                },
                Site {
                    id: "app-b".to_string(),
                    name: "App B".to_string(),
                    path: std::env::temp_dir(),
                    domain: "app-b.localhost".to_string(),
                    php: "7.4.33".to_string(),
                    framework: Some("codeigniter".to_string()),
                    backend: None,
                    enabled: true,
                },
            ],
        }
    }

    fn ports() -> BTreeMap<String, u16> {
        BTreeMap::from([
            ("7.4.33".to_string(), 9002u16),
            ("8.3.36".to_string(), 9001u16),
        ])
    }

    #[test]
    fn renders_one_block_per_site_with_its_own_fastcgi_port() {
        let paths = Paths {
            root: std::env::temp_dir(),
        };
        let caddy = CaddyBackend::new(paths);
        let text = caddy.render(&sample_config(), &ports()).unwrap();

        assert!(text.contains("http://app-a.localhost"));
        assert!(text.contains("php_fastcgi 127.0.0.1:9001"));
        assert!(text.contains("http://app-b.localhost"));
        assert!(text.contains("php_fastcgi 127.0.0.1:9002"));
        assert_eq!(text.matches("php_fastcgi").count(), 2);
    }

    #[test]
    fn disabled_sites_are_not_served() {
        let paths = Paths {
            root: std::env::temp_dir(),
        };
        let caddy = CaddyBackend::new(paths);
        let mut config = sample_config();
        config.sites[0].enabled = false;

        let text = caddy.render(&config, &ports()).unwrap();
        assert!(!text.contains("app-a.localhost"));
        assert!(text.contains("app-b.localhost"));
    }

    #[test]
    fn missing_php_port_is_an_error() {
        let paths = Paths {
            root: std::env::temp_dir(),
        };
        let caddy = CaddyBackend::new(paths);
        let mut map = ports();
        map.remove("7.4.33");
        assert!(caddy.render(&sample_config(), &map).is_err());
    }

    #[test]
    fn empty_config_still_valid() {
        let paths = Paths {
            root: std::env::temp_dir(),
        };
        let caddy = CaddyBackend::new(paths);
        let config = Config::default();
        let text = caddy.render(&config, &BTreeMap::new()).unwrap();
        assert!(text.contains("admin localhost:2019"));
        assert!(!text.contains("php_fastcgi"));
    }
}
