use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::config::Config;

pub const BEGIN: &str = "# devku:begin";
pub const END: &str = "# devku:end";

pub fn hosts_path() -> PathBuf {
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        PathBuf::from(root).join("System32/drivers/etc/hosts")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/hosts")
    }
}

/// `*.localhost` sudah resolve native, jadi menulis hosts hanya perlu saat user
/// memilih TLD lain. Mengembalikan None berarti tidak ada yang perlu ditulis.
pub fn plan(config: &Config) -> Option<String> {
    if config.tld == "localhost" {
        return None;
    }

    let mut body = String::new();
    for site in config.served_sites() {
        body.push_str(&format!("127.0.0.1 {}\n", site.domain));
    }
    if body.is_empty() {
        return None;
    }

    Some(format!("{BEGIN}\n{body}{END}\n"))
}

/// Ganti blok DevKu saja. Entri lain di hosts tidak boleh tersentuh — file itu
/// dipakai juga oleh blocker iklan, lisensi software, dan setelan jaringan user.
pub fn merge(existing: &str, block: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut inside = false;

    for line in existing.lines() {
        if line.trim() == BEGIN {
            inside = true;
            continue;
        }
        if line.trim() == END {
            inside = false;
            continue;
        }
        if !inside {
            kept.push(line);
        }
    }

    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }

    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(block);
    out
}

pub fn write(config: &Config) -> Result<Option<PathBuf>> {
    let Some(block) = plan(config) else {
        return Ok(None);
    };

    let path = hosts_path();
    let existing = std::fs::read_to_string(&path)
        .with_context(|| format!("tidak bisa membaca {}", path.display()))?;
    let next = merge(&existing, &block);

    std::fs::write(&path, next).map_err(|err| {
        if err.kind() == std::io::ErrorKind::PermissionDenied {
            anyhow::anyhow!(
                "menulis {} butuh hak administrator. Pakai TLD `localhost` untuk menghindari ini, \
                 atau jalankan DevKu sebagai administrator.",
                path.display()
            )
        } else {
            anyhow::Error::from(err).context(format!("gagal menulis {}", path.display()))
        }
    })?;

    Ok(Some(path))
}

pub fn read_current() -> Result<String> {
    let path = hosts_path();
    if !path.exists() {
        bail!("hosts tidak ditemukan di {}", path.display());
    }
    std::fs::read_to_string(&path).with_context(|| format!("gagal membaca {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_preserves_foreign_entries() {
        let existing = "127.0.0.1 localhost\n0.0.0.0 ads.example\n";
        let block = "# devku:begin\n127.0.0.1 app.localhost\n# devku:end\n";
        let out = merge(existing, block);

        assert!(out.contains("0.0.0.0 ads.example"));
        assert!(out.contains("127.0.0.1 app.localhost"));
        assert_eq!(out.matches(BEGIN).count(), 1);
    }

    #[test]
    fn merge_replaces_previous_block_without_duplicating() {
        let existing = "127.0.0.1 localhost\n# devku:begin\n127.0.0.1 old.local\n# devku:end\n";
        let block = "# devku:begin\n127.0.0.1 new.local\n# devku:end\n";
        let out = merge(existing, block);

        assert!(!out.contains("old.local"));
        assert!(out.contains("new.local"));
        assert_eq!(out.matches(BEGIN).count(), 1);
        assert_eq!(out.matches(END).count(), 1);
    }

    #[test]
    fn merge_is_idempotent() {
        let block = "# devku:begin\n127.0.0.1 a.local\n# devku:end\n";
        let once = merge("127.0.0.1 localhost\n", block);
        let twice = merge(&once, block);
        assert_eq!(once, twice);
    }

    #[test]
    fn localhost_tld_needs_no_hosts_entry() {
        let config = Config::default();
        assert_eq!(config.tld, "localhost");
        assert_eq!(plan(&config), None);
    }

    #[test]
    fn custom_tld_plans_entries() {
        let config = Config {
            tld: "test".to_string(),
            sites: vec![crate::config::Site {
                id: "app".to_string(),
                name: "App".to_string(),
                path: std::env::temp_dir(),
                domain: "app.test".to_string(),
                php: "8.3.36".to_string(),
                root: None,
                framework: None,
                backend: None,
                enabled: true,
            }],
            ..Default::default()
        };

        let block = plan(&config).unwrap();
        assert!(block.contains("127.0.0.1 app.test"));
    }
}
