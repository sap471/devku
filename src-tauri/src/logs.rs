use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

/// Bytes read from the end of a large log. A full `wordpress.log` reaches a
/// megabyte, and the pane only ever shows a tail.
pub const TAIL_WINDOW: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogKind {
    Server,
    Database,
    Php,
    Site,
}

impl LogKind {
    pub fn tag(self) -> &'static str {
        match self {
            LogKind::Server => "server",
            LogKind::Database => "database",
            LogKind::Php => "php",
            LogKind::Site => "site",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LogSource {
    /// File name including `.log`, which is also what `tail_log` takes
    /// without the extension.
    pub name: String,
    pub kind: String,
    pub bytes: u64,
    /// Seconds since the Unix epoch; `None` when the platform cannot tell.
    pub modified: Option<u64>,
}

/// Fixed names come from the supervisor's service specs; prefixed names come
/// from PHP pools and sites.
pub fn classify(file_name: &str) -> Option<LogKind> {
    if !file_name.ends_with(".log") {
        return None;
    }
    match file_name {
        "caddy.log" | "apache.log" | "apache-error.log" => return Some(LogKind::Server),
        "postgres.log" | "mariadb.log" | "mysql.log" => return Some(LogKind::Database),
        _ => {}
    }
    if file_name.starts_with("php-") {
        return Some(LogKind::Php);
    }
    Some(LogKind::Site)
}

pub fn sources(log_dir: &Path) -> Vec<LogSource> {
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return Vec::new();
    };

    let mut found: Vec<LogSource> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let kind = classify(&name)?;
            let meta = entry.metadata().ok()?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_secs());
            Some(LogSource {
                name,
                kind: kind.tag().to_string(),
                bytes: meta.len(),
                modified,
            })
        })
        .collect();

    found.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.name.cmp(&b.name))
    });
    found
}

pub fn tail(path: &Path, lines: usize) -> Result<Vec<String>> {
    if lines == 0 || !path.exists() {
        return Ok(Vec::new());
    }

    let mut file =
        std::fs::File::open(path).with_context(|| format!("gagal membuka {}", path.display()))?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(TAIL_WINDOW);
    file.seek(SeekFrom::Start(start))?;

    let mut buffer = Vec::new();
    file.take(TAIL_WINDOW)
        .read_to_end(&mut buffer)
        .with_context(|| format!("gagal membaca {}", path.display()))?;

    // Starting mid-file lands mid-line; that first fragment is not a log line.
    if start > 0 {
        match buffer.iter().position(|byte| *byte == b'\n') {
            Some(index) => {
                buffer.drain(..=index);
            }
            None => buffer.clear(),
        }
    }

    let text = String::from_utf8_lossy(&buffer);
    let all: Vec<String> = text
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect();
    let from = all.len().saturating_sub(lines);
    Ok(all[from..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("devku-logs-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn classifies_every_file_the_supervisor_and_sites_write() {
        assert_eq!(classify("caddy.log"), Some(LogKind::Server));
        assert_eq!(classify("apache.log"), Some(LogKind::Server));
        assert_eq!(classify("apache-error.log"), Some(LogKind::Server));
        assert_eq!(classify("postgres.log"), Some(LogKind::Database));
        assert_eq!(classify("mariadb.log"), Some(LogKind::Database));
        assert_eq!(classify("mysql.log"), Some(LogKind::Database));
        assert_eq!(classify("php-8.5.12-1.log"), Some(LogKind::Php));
        assert_eq!(classify("apache-wordpress-access.log"), Some(LogKind::Site));
        assert_eq!(classify("wordpress.log"), Some(LogKind::Site));
        assert_eq!(classify("notes.txt"), None);
    }

    #[test]
    fn lists_only_log_files_with_size_and_time() {
        let dir = temp("sources");
        std::fs::write(dir.join("caddy.log"), "a\n").unwrap();
        std::fs::write(dir.join("postgres.log"), "b\nc\n").unwrap();
        std::fs::write(dir.join("ignore.txt"), "x").unwrap();

        let found = sources(&dir);
        let names: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"caddy.log"));
        assert!(names.contains(&"postgres.log"));
        let pg = found.iter().find(|s| s.name == "postgres.log").unwrap();
        assert_eq!(pg.kind, "database");
        assert_eq!(pg.bytes, 4);
        assert!(pg.modified.is_some());
    }

    #[test]
    fn missing_log_directory_is_empty_not_an_error() {
        let dir = temp("missing");
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(sources(&dir).is_empty());
        assert!(tail(&dir.join("postgres.log"), 10).unwrap().is_empty());
    }

    #[test]
    fn tail_returns_the_last_lines_in_order() {
        let dir = temp("tail");
        let path = dir.join("app.log");
        std::fs::write(&path, "1\n2\n3\n4\n5\n").unwrap();
        assert_eq!(
            tail(&path, 2).unwrap(),
            vec!["4".to_string(), "5".to_string()]
        );
        assert_eq!(tail(&path, 99).unwrap().len(), 5);
    }

    #[test]
    fn tail_over_the_window_drops_the_partial_first_line() {
        let dir = temp("window");
        let path = dir.join("big.log");
        // Each line is 100 bytes; the file is larger than TAIL_WINDOW.
        let line = format!("{}\n", "x".repeat(99));
        let count = (TAIL_WINDOW / 100) as usize + 500;
        std::fs::write(&path, line.repeat(count)).unwrap();

        let got = tail(&path, 10).unwrap();
        assert_eq!(got.len(), 10);
        assert!(got.iter().all(|l| l.len() == 99));
    }

    #[test]
    fn tail_never_returns_more_lines_than_requested_on_wide_files() {
        let dir = temp("wide");
        let path = dir.join("big.log");
        let line = format!("{}\n", "y".repeat(399));
        std::fs::write(&path, line.repeat(4000)).unwrap();
        assert_eq!(tail(&path, 5).unwrap().len(), 5);
    }
}
