use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::{Paths, write_atomic};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Caddy,
    Apache,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub domain: String,
    pub php: String,
    /// Project root, which is not necessarily the web root (`public/` for
    /// Laravel). Used by the terminal and the script runner.
    #[serde(default)]
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub framework: Option<String>,
    #[serde(default)]
    pub backend: Option<Backend>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PhpSettings {
    /// The version list is deliberately not stored here: what is installed is
    /// read straight from `engines/php`, so a manually extracted folder is
    /// still detected without registering it first.
    #[serde(default)]
    pub active: String,
    /// Number of workers per version. 0 = let PHP use its default.
    #[serde(default = "default_children")]
    pub children: u16,
}

fn default_children() -> u16 {
    4
}

impl Default for PhpSettings {
    fn default() -> Self {
        Self {
            active: String::new(),
            children: default_children(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub backend: Backend,
    #[serde(default = "default_tld")]
    pub tld: String,
    /// Parent folder for projects. Empty means `%USERPROFILE%\DevKu\sites`.
    /// Project contents deliberately stay out of the data folder so moving app
    /// data does not touch the projects.
    #[serde(default)]
    pub workspace: Option<PathBuf>,
    /// Starts services as soon as the app opens. Can be disabled in config.
    #[serde(default = "yes")]
    pub autostart: bool,
    /// Record that the system already trusts Caddy's local CA. Not a live
    /// check, so the Percayai button may still be run again at any time.
    #[serde(default)]
    pub https_trusted: bool,
    /// First run is done or skipped. Old configs without this field count as
    /// not done, but onboarding only shows while there is no site yet.
    #[serde(default)]
    pub onboarded: bool,
    #[serde(default)]
    pub php: PhpSettings,
    #[serde(default)]
    pub database: DatabaseSettings,
    #[serde(default)]
    pub apache: ApacheSettings,
    #[serde(default)]
    pub sites: Vec<Site>,
}

/// Local database server settings.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DatabaseSettings {
    /// TCP port. 3306 is the MariaDB default and what Laravel `.env` files use,
    /// so this number is the one most often correct.
    #[serde(default = "default_db_port")]
    pub port: u16,
    /// The database starts only when checked. Off by default because it is an
    /// add-on feature, not a requirement for running a site.
    #[serde(default)]
    pub autostart: bool,
}

fn default_db_port() -> u16 {
    3306
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            port: default_db_port(),
            autostart: false,
        }
    }
}

/// DevKu installs Apache in `engines/apache` and it listens on loopback only.
/// Caddy on 80/443 forwards Apache sites to this internal port.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApacheSettings {
    #[serde(default = "default_apache_port")]
    pub port: u16,
}

fn default_apache_port() -> u16 {
    8080
}

impl Default for ApacheSettings {
    fn default() -> Self {
        Self {
            port: default_apache_port(),
        }
    }
}

fn one() -> u32 {
    1
}

fn default_tld() -> String {
    "localhost".into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            backend: Backend::Caddy,
            tld: default_tld(),
            workspace: None,
            autostart: true,
            https_trusted: false,
            onboarded: false,
            php: PhpSettings {
                active: String::new(),
                children: default_children(),
            },
            database: DatabaseSettings::default(),
            apache: ApacheSettings::default(),
            sites: Vec::new(),
        }
    }
}

impl Config {
    pub fn load(paths: &Paths) -> Result<Self> {
        let file = paths.config_file();
        if !file.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&file)
            .with_context(|| format!("gagal membaca {}", file.display()))?;
        let cfg: Config = serde_json::from_str(&raw)
            .with_context(|| format!("config rusak: {}", file.display()))?;
        Ok(cfg)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        paths.ensure()?;
        let json = serde_json::to_string_pretty(self)?;
        write_atomic(&paths.config_file(), &json)
    }

    /// Effective backend for one site: the site's choice, or the global default
    /// for an old site that never chose.
    pub fn backend_of(&self, site: &Site) -> Backend {
        site.backend.unwrap_or(self.backend)
    }

    /// All active sites, for the hosts file and PHP pool allocation. The server
    /// serving each site is decided by `backend_of`.
    pub fn served_sites(&self) -> impl Iterator<Item = &Site> {
        self.sites.iter().filter(|site| site.enabled)
    }

    /// Sites Caddy forwards to the internal httpd.
    pub fn apache_sites(&self) -> impl Iterator<Item = &Site> {
        self.served_sites()
            .filter(|site| self.backend_of(site) == Backend::Apache)
    }

    /// All sites are reached through a domain without a port: Caddy on 80/443
    /// is the entry point, including for sites whose backend is Apache.
    pub fn site_url(&self, site: &Site) -> String {
        format!("http://{}/", site.domain)
    }

    /// The project parent folder in effect. An empty value means the default
    /// location, not "no workspace".
    pub fn workspace_dir(&self) -> PathBuf {
        self.workspace
            .clone()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(crate::paths::default_workspace)
    }

    pub fn domain_for(&self, id: &str) -> String {
        format!("{id}.{}", self.tld)
    }

    /// PHP versions used by at least one active site, sorted so ports stay stable.
    pub fn php_versions_in_use(&self) -> Vec<String> {
        let mut set: BTreeSet<String> = BTreeSet::new();
        for site in self.served_sites() {
            set.insert(site.php.clone());
        }
        set.into_iter().collect()
    }

    pub fn site(&self, id: &str) -> Option<&Site> {
        self.sites.iter().find(|s| s.id == id)
    }

    pub fn validate(&self) -> Result<()> {
        for site in &self.sites {
            if site.id.is_empty() {
                anyhow::bail!("site tanpa id");
            }
            if site.domain.is_empty() {
                anyhow::bail!("site {} tanpa domain", site.id);
            }
            if !site.path.exists() {
                anyhow::bail!("site {}: folder {} tidak ada", site.id, site.path.display());
            }
        }
        Ok(())
    }
}

pub fn load_or_init(paths: &Paths) -> Result<Config> {
    paths.ensure()?;
    Config::load(paths)
}

pub fn touch(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_autostart_and_localhost_tld() {
        let config = Config::default();
        assert!(config.autostart);
        assert_eq!(config.tld, "localhost");
        assert!(config.sites.is_empty());
    }

    #[test]
    fn old_config_without_autostart_defaults_to_on() {
        let config: Config = serde_json::from_str(r#"{"version":1}"#).unwrap();
        assert!(config.autostart);
    }

    #[test]
    fn autostart_can_be_turned_off() {
        let config: Config = serde_json::from_str(r#"{"autostart":false}"#).unwrap();
        assert!(!config.autostart);
    }

    #[test]
    fn old_config_without_onboarded_starts_unonboarded() {
        let config: Config = serde_json::from_str(r#"{"version":1}"#).unwrap();
        assert!(!config.onboarded);
    }

    #[test]
    fn workspace_uses_choice_and_falls_back_to_default() {
        let config: Config = serde_json::from_str(r#"{"version":1}"#).unwrap();
        assert_eq!(config.workspace_dir(), crate::paths::default_workspace());

        let config: Config =
            serde_json::from_str(r#"{"version":1,"workspace":"D:\\projects"}"#).unwrap();
        assert_eq!(config.workspace_dir(), PathBuf::from("D:\\projects"));
    }

    #[test]
    fn onboarded_flag_survives_round_trip() {
        let config = Config {
            onboarded: true,
            ..Config::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert!(back.onboarded);
    }
}
