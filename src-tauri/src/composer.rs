use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Serialize;

use crate::paths::{Paths, write_atomic};
use crate::php::PhpManager;

const LATEST_URL: &str = "https://getcomposer.org/download/latest-stable/composer.phar";
const CHECKSUM_URL: &str = "https://getcomposer.org/download/latest-stable/composer.phar.sha256sum";
const PHAR: &str = "composer.phar";

/// PHP yang dipakai Composer. `source` hanya untuk menjelaskan asal pilihan,
/// bukan memengaruhi cara shim bekerja.
#[derive(Clone, Debug)]
pub struct PhpChoice {
    pub path: PathBuf,
    pub source: &'static str,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub installed: bool,
    pub version: Option<String>,
    /// PHP yang akan dipakai shim sekarang, bukan yang tercatat saat pasang.
    pub php: Option<PhpChoice>,
    pub php_missing: bool,
}

pub fn dir(paths: &Paths) -> PathBuf {
    paths.engines().join("composer")
}

pub fn phar(paths: &Paths) -> PathBuf {
    dir(paths).join(PHAR)
}

pub fn version_marker(paths: &Paths) -> PathBuf {
    dir(paths).join("version")
}

pub fn php_marker(paths: &Paths) -> PathBuf {
    dir(paths).join("php.path")
}

pub fn shim(paths: &Paths) -> PathBuf {
    crate::path_env::bin_dir(paths).join(if cfg!(windows) {
        "composer.bat"
    } else {
        "composer"
    })
}

/// Folder yang dipakai Composer untuk paket global. Di Windows defaultnya
/// `%APPDATA%\Composer\vendor\bin`, tempat `laravel.bat` dan tool global lain
/// dipasang.
pub fn global_bin_dir() -> PathBuf {
    if let Some(dir) = env_path("COMPOSER_BIN_DIR") {
        return dir;
    }

    let home = composer_home();
    configured_bin_dir(&home).unwrap_or_else(|| home.join("vendor").join("bin"))
}

fn composer_home() -> PathBuf {
    if let Some(home) = env_path("COMPOSER_HOME") {
        return home;
    }

    #[cfg(windows)]
    if let Some(appdata) = env_path("APPDATA") {
        return appdata.join("Composer");
    }

    #[cfg(not(windows))]
    if let Some(home) = env_path("HOME") {
        return home.join(".composer");
    }

    PathBuf::from(".composer")
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
}

/// Composer menyimpan `bin-dir` di config global. Nilai relatif dihitung dari
/// COMPOSER_HOME, sama seperti yang dilakukan Composer sendiri.
fn configured_bin_dir(home: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(home.join("config.json")).ok()?;
    let root: serde_json::Value = serde_json::from_str(&text).ok()?;
    let value = root.get("config")?.get("bin-dir")?.as_str()?.trim();
    if value.is_empty() {
        return None;
    }

    let path = PathBuf::from(value);
    Some(if path.is_absolute() {
        path
    } else {
        home.join(path)
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct GlobalStatus {
    pub dir: String,
    pub registered: bool,
    pub exists: bool,
    /// Nama tool tanpa ekstensi, mis. `laravel` dan `carbon`.
    pub tools: Vec<String>,
}

pub fn global_status() -> GlobalStatus {
    let dir = global_bin_dir();
    GlobalStatus {
        dir: dir.to_string_lossy().to_string(),
        registered: crate::path_env::is_registered(&dir),
        exists: dir.is_dir(),
        tools: tools_in(&dir),
    }
}

fn tools_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .map(|name| {
            name.strip_suffix(".bat")
                .or_else(|| name.strip_suffix(".cmd"))
                .or_else(|| name.strip_suffix(".exe"))
                .unwrap_or(&name)
                .to_string()
        })
        .collect();

    names.sort();
    names.dedup();
    names
}

pub fn status(paths: &Paths) -> Status {
    let installed = phar(paths).exists();
    let php = runtime_php(paths);
    Status {
        installed,
        version: read_trimmed(&version_marker(paths)),
        php_missing: installed && php.is_none(),
        php,
    }
}

/// PHP untuk memasang Composer. PATH menang, lalu shim PHP DevKu, lalu versi
/// bawaan terbaru. Urutan ini yang membuat mesin dengan PHP sendiri tidak
/// dipaksa memakai PHP unduhan DevKu.
pub fn php_for_composer(paths: &Paths) -> Option<PhpChoice> {
    if let Some(path) = php_in_path() {
        return Some(PhpChoice {
            path,
            source: "PATH",
        });
    }

    if let Some(version) = crate::path_env::status(paths).shim_version {
        let path = paths.php_dir(&version).join(cli_binary_name());
        if path.exists() {
            return Some(PhpChoice {
                path,
                source: "DevKu",
            });
        }
    }

    let manager = PhpManager::new(paths.clone());
    let version = manager.installed().last()?.clone();
    let path = paths.php_dir(&version).join(cli_binary_name());
    path.exists().then_some(PhpChoice {
        path,
        source: "DevKu",
    })
}

/// PHP yang benar-benar dipakai shim sekarang: `php` di PATH kalau ada, kalau
/// tidak jalur yang tercatat saat pemasangan. Versi bawaan terbaru tidak ikut
/// dihitung karena shim tidak mencarinya sendiri.
fn runtime_php(paths: &Paths) -> Option<PhpChoice> {
    if let Some(path) = php_in_path() {
        return Some(PhpChoice {
            path,
            source: "PATH",
        });
    }

    let stored = PathBuf::from(read_trimmed(&php_marker(paths))?);
    stored.exists().then_some(PhpChoice {
        path: stored,
        source: "tercatat",
    })
}

fn php_in_path() -> Option<PathBuf> {
    let value = std::env::var("PATH").ok()?;
    php_in_path_from(&value, |path| path.exists())
}

fn php_in_path_from(value: &str, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    value
        .split(separator)
        .map(|part| part.trim().trim_matches('"'))
        .filter(|part| !part.is_empty())
        .map(|part| Path::new(part).join(cli_binary_name()))
        .find(|path| exists(path))
}

fn cli_binary_name() -> &'static str {
    if cfg!(windows) { "php.exe" } else { "php" }
}

pub struct Installed {
    pub version: String,
    pub php: PhpChoice,
}

/// Unduh, verifikasi, uji jalan dengan PHP terpilih, lalu pasang shim. Phar
/// hanya dipindahkan ke `engines/composer` setelah PHP benar-benar bisa
/// menjalankannya, jadi kegagalan tidak meninggalkan Composer setengah jadi.
pub async fn install(
    paths: &Paths,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&'static str, u64, Option<u64>),
) -> Result<Installed> {
    let php = php_for_composer(paths).context(
        "PHP tidak ditemukan. Pasang PHP lewat DevKu atau taruh php di PATH terlebih dahulu.",
    )?;

    progress("daftar", 0, None);
    let client = crate::download::client()?;
    let checksum = latest_checksum(&client).await?;

    let temp = paths.downloads().join(PHAR);
    crate::download::download(
        &client,
        LATEST_URL,
        &temp,
        Some(&checksum),
        cancel,
        |received, total| progress("unduh", received, total),
    )
    .await?;

    progress("pasang", 0, None);
    let version = match phar_version(&php.path, &temp).await {
        Ok(version) => version,
        Err(err) => {
            let _ = std::fs::remove_file(&temp);
            return Err(err);
        }
    };

    let target = phar(paths);
    let marker = version_marker(paths);
    let marker_php = php_marker(paths);
    let marker_version = version.clone();
    let marker_path = php.path.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if target.exists() {
            std::fs::remove_file(&target)?;
        }
        std::fs::rename(&temp, &target)
            .with_context(|| format!("gagal memindahkan {}", target.display()))?;
        write_atomic(&marker, &format!("{marker_version}\n"))?;
        write_atomic(&marker_php, &format!("{}\n", marker_path.display()))?;
        Ok(())
    })
    .await
    .map_err(|err| anyhow::anyhow!("pemasangan terhenti: {err}"))??;

    write_shim(paths, &php.path)?;
    progress("selesai", 0, None);

    Ok(Installed { version, php })
}

async fn latest_checksum(client: &Client) -> Result<String> {
    let text = client
        .get(CHECKSUM_URL)
        .send()
        .await
        .context("gagal menghubungi getcomposer.org")?
        .error_for_status()
        .context("getcomposer.org menolak permintaan checksum")?
        .text()
        .await
        .context("gagal membaca checksum Composer")?;

    let digest = crate::download::checksum_for(&text, PHAR)
        .context("berkas checksum Composer tidak berisi composer.phar")?;
    if digest.len() != 64 || !digest.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("checksum Composer bukan SHA-256: {digest}");
    }
    Ok(digest)
}

async fn phar_version(php: &Path, phar: &Path) -> Result<String> {
    let mut command = tokio::process::Command::new(php);
    command.arg(phar).arg("--version");
    crate::subprocess::hide_console_tokio(&mut command);
    let output = command
        .output()
        .await
        .with_context(|| format!("gagal menjalankan {}", php.display()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        bail!(
            "Composer gagal dijalankan dengan {}: {}",
            php.display(),
            first_line(&stderr).unwrap_or("tanpa pesan")
        );
    }

    parse_version(&format!("{stdout}\n{stderr}")).with_context(|| {
        format!(
            "Composer tidak mencetak versi; keluaran: {}",
            first_line(&stdout).unwrap_or("kosong")
        )
    })
}

fn parse_version(text: &str) -> Option<String> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("Composer version"))?;
    line.split_whitespace().nth(2).map(str::to_string)
}

fn first_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

/// Shim sengaja memanggil `php` dari PATH lebih dulu. Kalau PATH tidak punya
/// PHP, jalur yang dipilih saat pemasangan dipakai supaya Composer tetap jalan
/// di terminal DevKu.
fn shim_script(php: &Path, phar: &Path) -> String {
    if cfg!(windows) {
        format!(
            "@echo off\r\n\
             where php >nul 2>nul\r\n\
             if %errorlevel%==0 (\r\n\
               php \"{}\" %*\r\n\
             ) else (\r\n\
               \"{}\" \"{}\" %*\r\n\
             )\r\n",
            phar.display(),
            php.display(),
            phar.display()
        )
    } else {
        format!(
            "#!/bin/sh\n\
             if command -v php >/dev/null 2>&1; then\n\
               exec php \"{}\" \"$@\"\n\
             fi\n\
             exec \"{}\" \"{}\" \"$@\"\n",
            phar.display(),
            php.display(),
            phar.display()
        )
    }
}

fn write_shim(paths: &Paths, php: &Path) -> Result<()> {
    let file = shim(paths);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&file, shim_script(php, &phar(paths)))
        .with_context(|| format!("gagal menulis {}", file.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

fn read_trimmed(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_from_banner() {
        let output = "Composer version 2.11.1 2026-02-04 12:34:56\n";
        assert_eq!(parse_version(output).as_deref(), Some("2.11.1"));
    }

    #[test]
    fn ignores_warning_before_version() {
        let output = "PHP Warning: something\nComposer version 2.9.3 2025-01-01 00:00:00\n";
        assert_eq!(parse_version(output).as_deref(), Some("2.9.3"));
    }

    #[test]
    fn missing_version_is_none() {
        assert_eq!(parse_version("Command not found"), None);
    }

    #[test]
    fn finds_php_in_path_entries() {
        let root = std::env::temp_dir().join(format!("devku-php-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let with_php = root.join("a");
        let without = root.join("b");
        std::fs::create_dir_all(&with_php).unwrap();
        std::fs::create_dir_all(&without).unwrap();
        std::fs::write(with_php.join(cli_binary_name()), "").unwrap();

        let value = format!("\"{}\";{}", without.display(), with_php.display());
        let found = php_in_path_from(&value, |path| path.exists()).unwrap();
        assert_eq!(found, with_php.join(cli_binary_name()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolves_bin_dir_from_global_config() {
        let root = std::env::temp_dir().join(format!("devku-composer-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("config.json"),
            r#"{"config":{"bin-dir":"vendor/bin"}}"#,
        )
        .unwrap();

        assert_eq!(
            configured_bin_dir(&root).unwrap(),
            root.join("vendor").join("bin")
        );

        std::fs::write(
            root.join("config.json"),
            r#"{"config":{"bin-dir":"C:\\tools\\composer"}}"#,
        )
        .unwrap();
        assert_eq!(
            configured_bin_dir(&root).unwrap(),
            PathBuf::from(r"C:\tools\composer")
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn config_without_bin_dir_falls_back() {
        let root = std::env::temp_dir().join(format!("devku-composer-cfg2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("config.json"), r#"{"config":{}}"#).unwrap();

        assert_eq!(configured_bin_dir(&root), None);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(windows)]
    fn shim_falls_back_when_path_has_no_php() {
        let php = Path::new(r"C:\php\php.exe");
        let phar = Path::new(r"C:\DevKu\engines\composer\composer.phar");
        let script = shim_script(php, phar);

        assert!(script.contains("where php"));
        assert!(script.contains(r#""C:\php\php.exe""#));
        assert!(script.contains(r#""C:\DevKu\engines\composer\composer.phar" %*"#));
    }
}
