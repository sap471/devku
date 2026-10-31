use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::paths::Paths;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RuntimeInfo {
    pub name: &'static str,
    pub version: Option<String>,
    pub source: &'static str,
    pub path: Option<String>,
}

impl RuntimeInfo {
    fn missing(name: &'static str) -> Self {
        Self {
            name,
            version: None,
            source: "tidak ada",
            path: None,
        }
    }
}

/// Urutan penting: runtime sistem menang, binary bawaan hanya cadangan. Ini yang
/// membuat DevKu tidak menambah salinan Node atau Bun di mesin yang sudah punya.
/// PHP juga jatuh ke folder `engines/php` kalau `php` tidak ada di PATH: versi
/// yang sudah diunduh lewat DevKu tidak boleh tampak "tidak terdeteksi" hanya
/// karena PATH di proses yang sudah lama hidup belum ikut berubah.
pub fn detect(paths: &Paths) -> Vec<RuntimeInfo> {
    let php = crate::php::PhpManager::new(paths.clone());
    let bundled_php = php.installed().last().map(|version| php.binary(version));

    vec![
        probe("bun", Some(paths.bundled_binary("bun"))),
        probe("node", None),
        composer_runtime(paths),
        probe("php", bundled_php),
    ]
}

/// Composer bawaan DevKu berupa shim `.bat`, bukan binary yang bisa dijalankan
/// langsung dengan `--version`. Versinya dibaca dari catatan pemasangan, dan
/// shim DevKu tidak boleh dianggap sebagai Composer sistem.
fn composer_runtime(paths: &Paths) -> RuntimeInfo {
    let managed = crate::composer::shim(paths);

    if let Some(path) = find_in_path("composer")
        && !same_path(&path, &managed)
    {
        return RuntimeInfo {
            name: "composer",
            version: version_of("composer", &path),
            source: "sistem",
            path: Some(path.to_string_lossy().to_string()),
        };
    }

    let status = crate::composer::status(paths);
    if status.installed {
        return RuntimeInfo {
            name: "composer",
            version: status.version,
            source: "bawaan",
            path: Some(managed.to_string_lossy().to_string()),
        };
    }

    RuntimeInfo::missing("composer")
}

fn same_path(left: &Path, right: &Path) -> bool {
    fn normalize(path: &Path) -> String {
        path.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    }
    normalize(left) == normalize(right)
}

fn probe(name: &'static str, bundled: Option<PathBuf>) -> RuntimeInfo {
    if let Some(path) = find_in_path(name) {
        return RuntimeInfo {
            name,
            version: version_of(name, &path),
            source: "sistem",
            path: Some(path.to_string_lossy().to_string()),
        };
    }

    if let Some(path) = bundled.filter(|path| path.exists()) {
        return RuntimeInfo {
            name,
            version: version_of(name, &path),
            source: "bawaan",
            path: Some(path.to_string_lossy().to_string()),
        };
    }

    RuntimeInfo::missing(name)
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let (program, args) = ("where.exe", vec![name]);
    #[cfg(not(windows))]
    let (program, args) = ("which", vec![name]);

    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
}

fn version_of(name: &str, path: &PathBuf) -> Option<String> {
    let output = Command::new(path).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    Some(normalize_version(name, line))
}

/// Setiap tool menulis versinya dengan bentuk berbeda. Yang ditampilkan ke user
/// hanya nomornya, bukan seluruh baris pembuka.
pub fn normalize_version(name: &str, line: &str) -> String {
    let line = line.trim();

    match name {
        "node" => line.trim_start_matches('v').to_string(),
        "php" => line
            .split_whitespace()
            .find(|token| token.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .unwrap_or(line)
            .to_string(),
        "composer" => line
            .split_whitespace()
            .nth(2)
            .unwrap_or(line)
            .trim_start_matches('v')
            .to_string(),
        _ => line.trim_start_matches('v').to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_node_without_v() {
        assert_eq!(normalize_version("node", "v24.22.1"), "24.22.1");
    }

    #[test]
    fn normalizes_php_banner() {
        assert_eq!(normalize_version("php", "PHP 8.3.36 (cli)"), "8.3.36");
    }

    #[test]
    fn normalizes_composer_banner() {
        assert_eq!(
            normalize_version("composer", "Composer version 2.8.4 2024-12-11 12:00:00"),
            "2.8.4"
        );
    }

    #[test]
    fn keeps_bun_number_as_is() {
        assert_eq!(normalize_version("bun", "1.4.4"), "1.4.4");
    }

    #[test]
    fn missing_runtime_reports_absence() {
        let info = RuntimeInfo::missing("bun");
        assert_eq!(info.source, "tidak ada");
        assert!(info.version.is_none());
    }
}
