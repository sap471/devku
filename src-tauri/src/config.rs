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
    /// Akar project, yang belum tentu sama dengan akar web (`public/` untuk
    /// Laravel). Dipakai terminal dan script runner.
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
    #[serde(default)]
    pub installed: BTreeSet<String>,
    #[serde(default)]
    pub active: String,
    /// Jumlah worker per versi. 0 = biarkan PHP memakai default-nya.
    #[serde(default = "default_children")]
    pub children: u16,
}

fn default_children() -> u16 {
    4
}

impl Default for PhpSettings {
    fn default() -> Self {
        Self {
            installed: BTreeSet::new(),
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
    /// Menyalakan service begitu aplikasi dibuka. Bisa dimatikan di config.
    #[serde(default = "yes")]
    pub autostart: bool,
    /// Catatan bahwa CA lokal Caddy sudah dipercaya sistem. Bukan hasil cek
    /// live, jadi tombol Percayai tetap boleh dijalankan ulang kapan saja.
    #[serde(default)]
    pub https_trusted: bool,
    #[serde(default)]
    pub php: PhpSettings,
    #[serde(default)]
    pub sites: Vec<Site>,
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
            autostart: true,
            https_trusted: false,
            php: PhpSettings {
                installed: BTreeSet::new(),
                active: String::new(),
                children: default_children(),
            },
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

    /// Site yang benar-benar dilayani: enabled + backend cocok dengan backend aktif.
    pub fn served_sites(&self) -> impl Iterator<Item = &Site> {
        self.sites
            .iter()
            .filter(move |s| s.enabled && s.backend.unwrap_or(self.backend) == self.backend)
    }

    pub fn domain_for(&self, id: &str) -> String {
        format!("{id}.{}", self.tld)
    }

    /// Versi PHP yang dipakai minimal satu site aktif, terurut supaya port stabil.
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
}
