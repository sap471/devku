use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::database::DatabaseEngine;
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

/// Local database server settings. The engine is an explicit entry so
/// `config.json` says which server it describes instead of implying MariaDB.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct DatabaseSettings {
    pub engines: Vec<DbEngineSettings>,
}

impl<'de> Deserialize<'de> for DatabaseSettings {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// Configs written before the engine list hold the settings directly on
        /// the database object, so both shapes have to be accepted.
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            engines: Option<Vec<DbEngineSettings>>,
            #[serde(default)]
            port: Option<u16>,
            #[serde(default)]
            autostart: Option<bool>,
        }

        let raw = Raw::deserialize(deserializer)?;
        if let Some(engines) = raw.engines {
            return Ok(Self { engines });
        }

        Ok(Self {
            engines: vec![DbEngineSettings {
                engine: DatabaseEngine::Mariadb,
                port: raw.port.unwrap_or_else(default_db_port),
                autostart: raw.autostart.unwrap_or(false),
            }],
        })
    }
}

/// One engine the user chose to use. `autostart` is per engine because a
/// second engine may be wanted only on demand.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DbEngineSettings {
    #[serde(default)]
    pub engine: DatabaseEngine,
    #[serde(default = "default_port_for")]
    pub port: u16,
    #[serde(default)]
    pub autostart: bool,
}

impl DatabaseSettings {
    pub fn entry(&self, engine: DatabaseEngine) -> Option<&DbEngineSettings> {
        self.engines.iter().find(|item| item.engine == engine)
    }

    pub fn entry_mut(&mut self, engine: DatabaseEngine) -> Option<&mut DbEngineSettings> {
        self.engines.iter_mut().find(|item| item.engine == engine)
    }

    /// Port and autostart of one engine, or its defaults when it is not
    /// configured. Callers that still deal with a single engine use this so
    /// they do not have to spell out "not configured" everywhere.
    pub fn port_of(&self, engine: DatabaseEngine) -> u16 {
        self.entry(engine)
            .map(|item| item.port)
            .unwrap_or_else(|| engine.default_port())
    }

    pub fn autostart_of(&self, engine: DatabaseEngine) -> bool {
        self.entry(engine).is_some_and(|item| item.autostart)
    }

    /// The two MySQL-family servers cannot share a machine: they speak the same
    /// protocol, default to the same port, and a user cannot tell which one
    /// answered. PostgreSQL has its own port and auth model, so it may run
    /// alongside either.
    pub fn validate(&self) -> Result<()> {
        let has_mariadb = self.entry(DatabaseEngine::Mariadb).is_some();
        let has_mysql = self.entry(DatabaseEngine::Mysql).is_some();
        if has_mariadb && has_mysql {
            bail!("MariaDB dan MySQL tidak bisa dipakai bersamaan: pilih salah satu");
        }

        let mut seen = BTreeSet::new();
        for item in &self.engines {
            if !seen.insert(item.engine) {
                bail!("engine {} tercantum lebih dari sekali", item.engine.key());
            }
        }
        Ok(())
    }
}

fn default_port_for() -> u16 {
    // Only used when a hand-written entry omits `port`; `DbEngineSettings::default`
    // is what picks the per-engine port for a new entry.
    default_db_port()
}

impl Default for DbEngineSettings {
    fn default() -> Self {
        Self {
            engine: DatabaseEngine::Mariadb,
            port: default_db_port(),
            autostart: false,
        }
    }
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            engines: vec![DbEngineSettings::default()],
        }
    }
}

fn default_db_port() -> u16 {
    DatabaseEngine::Mariadb.default_port()
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
        self.database
            .validate()
            .context("pengaturan database tidak valid")?;

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

    #[test]
    fn old_config_without_engines_loads_as_mariadb() {
        let config: Config =
            serde_json::from_str(r#"{"version":1,"database":{"port":3306,"autostart":true}}"#)
                .unwrap();
        assert_eq!(config.database.engines.len(), 1);
        assert_eq!(config.database.engines[0].engine, DatabaseEngine::Mariadb);
        assert_eq!(config.database.engines[0].port, 3306);
        assert!(config.database.engines[0].autostart);
    }

    #[test]
    fn maria_and_mysql_together_is_refused() {
        let settings = DatabaseSettings {
            engines: vec![
                DbEngineSettings {
                    engine: DatabaseEngine::Mariadb,
                    port: 3306,
                    autostart: true,
                },
                DbEngineSettings {
                    engine: DatabaseEngine::Mysql,
                    port: 3307,
                    autostart: true,
                },
            ],
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn postgres_alongside_maria_is_accepted() {
        let settings = DatabaseSettings {
            engines: vec![
                DbEngineSettings {
                    engine: DatabaseEngine::Mariadb,
                    port: 3306,
                    autostart: true,
                },
                DbEngineSettings {
                    engine: DatabaseEngine::Postgres,
                    port: 5432,
                    autostart: true,
                },
            ],
        };
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn config_validate_rejects_two_mysql_family_engines() {
        let mut config = Config::default();
        config.database.engines = vec![
            DbEngineSettings {
                engine: DatabaseEngine::Mariadb,
                port: 3306,
                autostart: false,
            },
            DbEngineSettings {
                engine: DatabaseEngine::Mysql,
                port: 3307,
                autostart: false,
            },
        ];
        assert!(config.validate().is_err());
    }

    #[test]
    fn duplicate_engine_entries_are_refused() {
        let settings = DatabaseSettings {
            engines: vec![DbEngineSettings::default(), DbEngineSettings::default()],
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn database_engines_round_trip_stably() {
        let config = Config::default();
        let text = serde_json::to_string_pretty(&config).unwrap();
        let again: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(again.database.engines, config.database.engines);
        assert!(text.contains("\"engine\": \"mariadb\""));
    }
}
