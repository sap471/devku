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

/// Order matters: the system runtime wins, the bundled binary is a fallback only. This is what
/// keeps DevKu from adding another Node or Bun copy on a machine that already has one.
/// PHP also falls back to the `engines/php` folder when `php` is absent from PATH: a version
/// already downloaded through DevKu must not look "undetected" only
/// because the PATH of a long-lived process has not picked up the change yet.
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

/// DevKu's bundled Composer is a `.bat` shim, not a binary that runs
/// directly with `--version`. Its version comes from the install record, and
/// the DevKu shim must not count as the system Composer.
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

pub fn find_in_path(name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    let (program, args) = ("where.exe", vec![name]);
    #[cfg(not(windows))]
    let (program, args) = ("which", vec![name]);

    let mut command = Command::new(program);
    command.args(args);
    crate::subprocess::hide_console_std(&mut command);
    let output = command.output().ok()?;
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
    let mut command = Command::new(path);
    command.arg("--version");
    crate::subprocess::hide_console_std(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next()?.trim();
    Some(normalize_version(name, line))
}

/// Each tool prints its version in a different shape. What the user sees
/// is only the number, not the whole opening line.
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
