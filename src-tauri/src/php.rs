use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

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

    /// Versi yang benar-benar bisa dipakai: direktorinya punya binary php-cgi.
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
        out.into_iter().collect()
    }

    pub fn extract(&self, zip_path: &Path, version: &str) -> Result<PathBuf> {
        let dest = self.paths.php_dir(version);
        std::fs::create_dir_all(&dest)?;

        let file = std::fs::File::open(zip_path)
            .with_context(|| format!("gagal membuka {}", zip_path.display()))?;
        let mut archive = zip::ZipArchive::new(file)
            .with_context(|| format!("bukan zip: {}", zip_path.display()))?;

        let names: Vec<String> = archive
            .file_names()
            .filter_map(|name| name.ok().map(|cow| cow.to_string()))
            .collect();
        let strip = common_root(&names);

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let raw = entry.name().map(|cow| cow.to_string()).unwrap_or_default();
            let relative = match &strip {
                Some(root) => raw
                    .strip_prefix(root)
                    .unwrap_or(&raw)
                    .trim_start_matches('/')
                    .to_string(),
                None => raw.clone(),
            };
            if relative.is_empty() {
                continue;
            }
            let target = safe_join(&dest, &relative)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&target)
                .with_context(|| format!("gagal menulis {}", target.display()))?;
            std::io::copy(&mut entry, &mut out)?;
        }

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
        let ini = dir.join("php.ini");
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

/// PHP zip resmi biasanya datar, tapi unduhan pihak ketiga sering membungkus
/// semuanya dalam satu folder. Deteksi itu supaya tidak jadi `8.3/8.3/php-cgi.exe`.
fn common_root(names: &[String]) -> Option<String> {
    // Ada file di akar zip? Berarti sudah datar, jangan strip apa pun.
    if names.iter().any(|name| !name.contains('/')) {
        return None;
    }

    let mut parts = names
        .iter()
        .map(|name| name.split('/').next().unwrap_or(""));
    let first = parts.next()?.to_string();
    if first.is_empty() {
        return None;
    }
    if parts.all(|root| root == first) {
        Some(first)
    } else {
        None
    }
}

fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    let candidate = Path::new(relative);
    let mut out = base.to_path_buf();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => bail!("entri zip berbahaya: {relative}"),
        }
    }
    if !out.starts_with(base) {
        bail!("entri zip keluar dari folder tujuan: {relative}");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_root_detects_single_wrapper() {
        let names = vec![
            "php-8.3/php-cgi.exe".to_string(),
            "php-8.3/ext/php_mbstring.dll".to_string(),
        ];
        assert_eq!(common_root(&names).as_deref(), Some("php-8.3"));
    }

    #[test]
    fn common_root_none_when_flat() {
        let names = vec!["php-cgi.exe".to_string(), "ext/mbstring.dll".to_string()];
        assert_eq!(common_root(&names), None);
    }

    #[test]
    fn safe_join_rejects_traversal() {
        let base = Path::new("/tmp/php");
        assert!(safe_join(base, "../evil.exe").is_err());
        assert!(safe_join(base, "/abs/evil.exe").is_err());
        assert!(safe_join(base, "ext/php_gd.dll").is_ok());
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
