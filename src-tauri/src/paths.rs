use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Semua state DevKu tinggal di satu root supaya bisa dipindah/dihapus utuh.
pub fn home() -> PathBuf {
    if let Some(custom) = std::env::var("DEVKU_HOME")
        .ok()
        .filter(|value| !value.is_empty())
    {
        return PathBuf::from(custom);
    }

    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("DevKu");
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join("devku");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".local/share/devku");
        }
    }

    PathBuf::from(".devku")
}

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        Self { root: home() }
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.json")
    }

    pub fn engines(&self) -> PathBuf {
        self.root.join("engines")
    }

    pub fn php_dir(&self, version: &str) -> PathBuf {
        self.engines().join("php").join(version)
    }

    pub fn caddy_binary(&self) -> PathBuf {
        self.engines()
            .join("caddy")
            .join(if cfg!(windows) { "caddy.exe" } else { "caddy" })
    }

    pub fn caddy_dir(&self) -> PathBuf {
        self.engines().join("caddy")
    }

    pub fn mariadb_dir(&self) -> PathBuf {
        self.engines().join("mariadb")
    }

    /// `bin/` harus tetap di samping `server.dll`: stub mysqld memuat server
    /// dari folder yang sama.
    pub fn mariadb_bin(&self) -> PathBuf {
        self.mariadb_dir().join("bin")
    }

    /// Data user sengaja terpisah dari `engines/`, supaya memasang ulang binari
    /// tidak menyentuh database.
    pub fn mariadb_data(&self) -> PathBuf {
        self.root.join("databases").join("mariadb")
    }

    pub fn mariadb_runtime(&self) -> PathBuf {
        self.runtime().join("mariadb")
    }

    pub fn mariadb_config(&self) -> PathBuf {
        self.mariadb_runtime().join("my.ini")
    }

    /// Password root disimpan di luar `config.json` supaya tidak ikut ter-ekspor.
    pub fn mariadb_password(&self) -> PathBuf {
        self.mariadb_runtime().join("root.pass")
    }

    pub fn backups(&self) -> PathBuf {
        self.root.join("backups")
    }

    pub fn runtime(&self) -> PathBuf {
        self.root.join("runtime")
    }

    pub fn logs(&self) -> PathBuf {
        self.runtime().join("logs")
    }

    /// Berkas unduhan sementara (zip PHP/Caddy) sebelum dipasang.
    pub fn downloads(&self) -> PathBuf {
        self.runtime().join("downloads")
    }

    pub fn caddyfile(&self) -> PathBuf {
        self.runtime().join("Caddyfile")
    }

    pub fn dashboard_dir(&self) -> PathBuf {
        self.runtime().join("dashboard")
    }

    /// Binary yang DevKu sediakan sendiri kalau mesin tidak punya versinya.
    pub fn bundled_binary(&self, name: &str) -> PathBuf {
        let file = if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        };
        self.engines().join(name).join(file)
    }

    pub fn ensure(&self) -> Result<()> {
        for dir in [
            self.root.clone(),
            self.engines(),
            self.runtime(),
            self.logs(),
        ] {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("gagal membuat {}", dir.display()))?;
        }
        Ok(())
    }
}

/// Tulis file lewat temp + rename supaya config tidak pernah setengah jadi.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents).with_context(|| format!("gagal menulis {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("gagal memindahkan {}", path.display()))?;
    Ok(())
}
