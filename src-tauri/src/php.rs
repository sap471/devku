use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::paths::Paths;
use crate::proc::ServiceSpec;

/// Ekstensi yang hampir selalu dibutuhkan Laravel/CI. Sisanya bisa dinyalakan
/// manual lewat php.ini — lebih baik default kecil daripada semua hidup.
const DEFAULT_EXTENSIONS: &[&str] = &[
    "curl",
    "fileinfo",
    "gd",
    "intl",
    "mbstring",
    "openssl",
    "pdo_mysql",
    "pdo_pgsql",
    "pdo_sqlite",
    "sqlite3",
    "zip",
];

pub struct PhpManager {
    paths: Paths,
}

impl PhpManager {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    pub fn binary_name() -> &'static str {
        if cfg!(windows) {
            "php-cgi.exe"
        } else {
            "php-cgi"
        }
    }

    pub fn binary(&self, version: &str) -> PathBuf {
        self.paths.php_dir(version).join(Self::binary_name())
    }

    /// Berkas konfigurasi yang dibaca php-cgi lewat `PHPRC`.
    pub fn ini_path(&self, version: &str) -> PathBuf {
        self.paths.php_dir(version).join("php.ini")
    }

    /// Versi yang benar-benar bisa dipakai: direktorinya punya binary php-cgi.
    /// Urutannya numerik dan menaik, jadi `last()` selalu versi terbaru
    /// (leksikografis akan menaruh 8.9 di atas 8.10).
    pub fn installed(&self) -> Vec<String> {
        let base = self.paths.engines().join("php");
        let mut out = BTreeSet::new();
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                if !entry.path().join(Self::binary_name()).exists() {
                    continue;
                }
                if let Some(name) = entry.file_name().to_str() {
                    out.insert(name.to_string());
                }
            }
        }

        let mut versions: Vec<String> = out.into_iter().collect();
        versions.sort_by_key(|version| version_key(version));
        versions
    }

    pub fn extract(&self, zip_path: &Path, version: &str) -> Result<PathBuf> {
        let dest = self.paths.php_dir(version);
        crate::archive::extract(zip_path, &dest)?;

        if !self.binary(version).exists() {
            bail!(
                "zip tidak berisi {}; isi paket tidak dikenali",
                Self::binary_name()
            );
        }

        self.ensure_ini(version)?;
        Ok(dest)
    }

    /// php.ini-development disalin sekali, lalu dirapikan seperlunya. Kalau user
    /// sudah punya php.ini, jangan sentuh.
    pub fn ensure_ini(&self, version: &str) -> Result<PathBuf> {
        let dir = self.paths.php_dir(version);
        let ini = self.ini_path(version);
        if ini.exists() {
            return Ok(ini);
        }

        let template = ["php.ini-development", "php.ini-production"]
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.exists())
            .with_context(|| format!("tidak ada template php.ini di {}", dir.display()))?;

        let raw = std::fs::read_to_string(&template)?;
        let mut lines = Vec::new();
        for line in raw.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with(";extension_dir") {
                lines.push("extension_dir = \"ext\"".to_string());
                continue;
            }
            if let Some(ext) = trimmed.strip_prefix(";extension=") {
                let name = ext.trim();
                if DEFAULT_EXTENSIONS.contains(&name) {
                    lines.push(format!("extension={name}"));
                    continue;
                }
            }
            lines.push(line.to_string());
        }

        std::fs::write(&ini, lines.join("\n"))?;
        Ok(ini)
    }

    pub fn spec(&self, version: &str, port: u16, children: u16) -> Result<ServiceSpec> {
        let binary = self.binary(version);
        if !binary.exists() {
            bail!("PHP {version} tidak terpasang di {}", binary.display());
        }
        self.ensure_ini(version)?;

        let mut env = vec![(
            "PHPRC".to_string(),
            self.paths.php_dir(version).to_string_lossy().to_string(),
        )];
        if children > 0 {
            env.push(("PHP_FCGI_CHILDREN".to_string(), children.to_string()));
        }

        Ok(ServiceSpec {
            name: format!("php-{version}"),
            program: binary,
            args: vec!["-b".to_string(), format!("127.0.0.1:{port}")],
            env,
            cwd: Some(self.paths.php_dir(version)),
            port: Some(port),
            log_dir: self.paths.logs(),
        })
    }
}

/// `8.10.2` > `8.9.0`, juga untuk nama build `8.5.12-1`. Bagian non-angka
/// diabaikan, cukup untuk memilih kronologi rilis PHP.
pub fn version_key(version: &str) -> Vec<u64> {
    version
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_is_sorted_numerically() {
        let root = std::env::temp_dir().join(format!("devku-php-order-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for version in ["8.10.2", "8.9.0", "7.4.33"] {
            let dir = root.join("engines/php").join(version);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(PhpManager::binary_name()), "").unwrap();
        }

        let manager = PhpManager::new(Paths { root: root.clone() });
        assert_eq!(manager.installed(), vec!["7.4.33", "8.9.0", "8.10.2"]);
        assert_eq!(manager.installed().last().unwrap(), "8.10.2");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ini_rewrites_extension_dir_and_enables_known_exts() {
        let root = std::env::temp_dir().join(format!("devku-ini-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let php_dir = root.join("engines/php/8.3.36");
        std::fs::create_dir_all(&php_dir).unwrap();
        std::fs::write(
            php_dir.join("php.ini-development"),
            ";extension_dir = \"ext\"\n;extension=mbstring\n;extension=snmp\n",
        )
        .unwrap();

        let manager = PhpManager::new(Paths { root: root.clone() });
        let ini = manager.ensure_ini("8.3.36").unwrap();
        let text = std::fs::read_to_string(ini).unwrap();

        assert!(text.contains("extension_dir = \"ext\""));
        assert!(text.contains("extension=mbstring"));
        // Ekstensi di luar daftar default harus tetap terkomen.
        assert!(text.contains(";extension=snmp"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
