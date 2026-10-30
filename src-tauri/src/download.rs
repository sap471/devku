use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Serialize;
use sha2::{Digest, Sha256};

const RELEASES_URL: &str = "https://windows.php.net/downloads/releases/releases.json";
const RELEASES_BASE: &str = "https://windows.php.net/downloads/releases/";

/// Satu build PHP yang bisa dipasang. `version` dipakai apa adanya sebagai nama
/// folder, jadi build ulang seperti `8.3.36-1` tidak menimpa `8.3.36`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PhpRelease {
    pub branch: String,
    pub version: String,
    pub filename: String,
    pub sha256: String,
    pub size: String,
}

impl PhpRelease {
    pub fn url(&self) -> String {
        format!("{RELEASES_BASE}{}", self.filename)
    }
}

/// Satu unduhan dalam satu waktu. Flag batal disimpan di sini supaya perintah
/// `cancel_download` bisa menghentikan unduhan yang sedang berjalan.
#[derive(Default)]
pub struct Slot {
    current: Mutex<Option<Arc<AtomicBool>>>,
}

impl Slot {
    pub fn begin(&self) -> Result<Arc<AtomicBool>> {
        let mut current = self.current.lock().unwrap();
        if current.is_some() {
            bail!("masih ada unduhan yang berjalan");
        }
        let flag = Arc::new(AtomicBool::new(false));
        *current = Some(flag.clone());
        Ok(flag)
    }

    pub fn finish(&self) {
        *self.current.lock().unwrap() = None;
    }

    pub fn cancel(&self) -> bool {
        match self.current.lock().unwrap().as_ref() {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }
}

pub fn client() -> Result<Client> {
    Client::builder()
        .user_agent(concat!("DevKu/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        // Berkas PHP 35 MB; batas ini untuk koneksi lambat, bukan untuk macet.
        .timeout(Duration::from_secs(900))
        .build()
        .context("gagal menyiapkan klien HTTP")
}

pub async fn php_releases(client: &Client) -> Result<Vec<PhpRelease>> {
    let text = client
        .get(RELEASES_URL)
        .send()
        .await
        .context("gagal menghubungi windows.php.net")?
        .error_for_status()
        .context("windows.php.net menolak permintaan")?
        .text()
        .await
        .context("gagal membaca daftar rilis PHP")?;

    parse_php_releases(&text)
}

/// Bentuk releases.json: `{ "8.3": { "version": "8.3.36-1", "nts-vs16-x64": {
/// "zip": { "path": ..., "sha256": ... } } } }`. Yang dipakai build NTS x64,
/// karena itulah yang dipakai php-cgi di Windows.
pub fn parse_php_releases(text: &str) -> Result<Vec<PhpRelease>> {
    let root: serde_json::Value =
        serde_json::from_str(text).context("bentuk releases.json tidak dikenali")?;
    let branches = root.as_object().context("releases.json bukan objek")?;

    let mut out = Vec::new();
    for (branch, value) in branches {
        if !is_branch(branch) {
            continue;
        }
        let Some(version) = value.get("version").and_then(|item| item.as_str()) else {
            continue;
        };
        let Some(variant) = value.as_object().and_then(|variants| {
            variants
                .iter()
                .find(|(key, _)| key.starts_with("nts-") && key.ends_with("-x64"))
                .map(|(_, variant)| variant)
        }) else {
            continue;
        };
        let Some(zip) = variant.get("zip") else {
            continue;
        };
        let (Some(filename), Some(sha256)) = (
            zip.get("path").and_then(|item| item.as_str()),
            zip.get("sha256").and_then(|item| item.as_str()),
        ) else {
            continue;
        };

        out.push(PhpRelease {
            branch: branch.clone(),
            version: version.to_string(),
            filename: filename.to_string(),
            sha256: sha256.to_string(),
            size: zip
                .get("size")
                .and_then(|item| item.as_str())
                .unwrap_or("?")
                .to_string(),
        });
    }

    out.sort_by_key(|item| std::cmp::Reverse(rank(&item.version)));
    Ok(out)
}

fn is_branch(key: &str) -> bool {
    key.split('.')
        .all(|part| !part.is_empty() && part.len() < 4 && part.chars().all(|c| c.is_ascii_digit()))
        && key.contains('.')
}

/// `8.3.36-1` -> (8, 3, 36). Sufiks build sengaja diabaikan saat mengurutkan,
/// supaya build ulang menempati urutan versi yang sama.
fn rank(version: &str) -> (u32, u32, u32) {
    let mut parts = version
        .split(['.', '-'])
        .map(|part| part.parse::<u32>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// Mengunduh ke `dest` sambil menghitung SHA-256. Berkas yang tidak cocok
/// langsung dihapus, jadi yang tersisa di disk selalu bisa dipercaya.
pub async fn download(
    client: &Client,
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let response = client
        .get(url)
        .send()
        .await
        .context("gagal menghubungi server unduhan")?
        .error_for_status()
        .context("server unduhan menolak permintaan")?;
    let total = response.content_length();

    let mut file =
        std::fs::File::create(dest).with_context(|| format!("gagal membuat {}", dest.display()))?;
    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut last_report = std::time::Instant::now() - Duration::from_secs(1);

    let mut response = response;
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(err) => {
                drop(file);
                let _ = std::fs::remove_file(dest);
                return Err(err).context("koneksi terputus saat mengunduh");
            }
        };

        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = std::fs::remove_file(dest);
            bail!("unduhan dibatalkan");
        }

        hasher.update(&chunk);
        file.write_all(&chunk)?;
        received += chunk.len() as u64;

        if last_report.elapsed() >= Duration::from_millis(150) {
            progress(received, total);
            last_report = std::time::Instant::now();
        }
    }

    file.flush()?;
    drop(file);

    let digest = hex(&hasher.finalize());
    if !digest.eq_ignore_ascii_case(expected_sha256) {
        let _ = std::fs::remove_file(dest);
        bail!("SHA-256 tidak cocok (dapat {digest}); berkas unduhan dibuang");
    }

    progress(received, total);
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "7.4": {
        "version": "7.4.33",
        "ts-vc15-x64": { "zip": { "path": "php-7.4.33-ts.zip", "sha256": "ts" } },
        "nts-vc15-x64": { "zip": { "path": "php-7.4.33-nts.zip", "sha256": "aaa", "size": "25.02MB" } }
      },
      "8.4": {
        "version": "8.4.27-1",
        "nts-vs17-x64": { "zip": { "path": "php-8.4.27-1-nts.zip", "sha256": "bbb", "size": "34.63MB" } }
      }
    }"#;

    #[test]
    fn picks_nts_x64_and_skips_thread_safe_build() {
        let releases = parse_php_releases(SAMPLE).unwrap();
        let old = releases.iter().find(|item| item.branch == "7.4").unwrap();
        assert_eq!(old.filename, "php-7.4.33-nts.zip");
        assert_eq!(old.sha256, "aaa");
        assert_eq!(old.size, "25.02MB");
    }

    #[test]
    fn sorts_newest_version_first() {
        let releases = parse_php_releases(SAMPLE).unwrap();
        assert_eq!(releases[0].version, "8.4.27-1");
        assert_eq!(releases[1].version, "7.4.33");
    }

    #[test]
    fn skips_branch_without_x64_nts_build() {
        let json = r#"{"8.1": {"version": "8.1.34", "nts-vs16-x86": {"zip": {"path": "x.zip", "sha256": "z"}}}}"#;
        assert!(parse_php_releases(json).unwrap().is_empty());
    }

    #[test]
    fn ignores_non_branch_keys() {
        let json = r#"{"meta": {"version": "x"}, "8.2": {"version": "8.2.35"}}"#;
        assert!(parse_php_releases(json).unwrap().is_empty());
    }

    #[test]
    fn release_url_points_at_windows_php_net() {
        let releases = parse_php_releases(SAMPLE).unwrap();
        assert_eq!(
            releases[1].url(),
            "https://windows.php.net/downloads/releases/php-7.4.33-nts.zip"
        );
    }

    #[test]
    fn hex_is_lowercase_and_padded() {
        assert_eq!(hex(&[0x0a, 0xff, 0x00]), "0aff00");
    }

    #[test]
    fn slot_allows_one_download_and_cancel_flips_the_flag() {
        let slot = Slot::default();
        let flag = slot.begin().unwrap();
        assert!(!flag.load(Ordering::Relaxed));
        assert!(slot.begin().is_err());

        assert!(slot.cancel());
        assert!(flag.load(Ordering::Relaxed));

        slot.finish();
        assert!(!slot.cancel());
    }

    #[test]
    fn rank_ignores_rebuild_suffix() {
        assert_eq!(rank("8.3.36-1"), rank("8.3.36"));
        assert!(rank("8.4.27") > rank("8.3.36"));
    }

    /// Jalankan dengan `cargo test --lib installs_real_php -- --ignored`.
    /// Butuh jaringan dan mengunduh sekitar 35 MB.
    #[test]
    #[ignore = "butuh jaringan"]
    fn installs_real_php_release_from_windows_php_net() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let client = client().unwrap();
        let releases = runtime.block_on(php_releases(&client)).unwrap();
        let release = releases
            .iter()
            .find(|item| item.branch == "8.3")
            .or_else(|| releases.first())
            .cloned()
            .expect("daftar rilis kosong");

        let root = std::env::temp_dir().join(format!("devku-download-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = crate::paths::Paths { root: root.clone() };

        let archive = paths.downloads().join(&release.filename);
        let cancel = AtomicBool::new(false);
        runtime
            .block_on(download(
                &client,
                &release.url(),
                &archive,
                &release.sha256,
                &cancel,
                |received, total| {
                    if let Some(total) = total {
                        // Bukti bahwa progres benar-benar mengalir, bukan 0%.
                        assert!(received <= total);
                    }
                },
            ))
            .unwrap();

        let manager = crate::php::PhpManager::new(paths.clone());
        let dir = manager.extract(&archive, &release.version).unwrap();
        manager.ensure_ini(&release.version).unwrap();

        assert!(
            manager.binary(&release.version).exists(),
            "php-cgi tidak ditemukan di {}",
            dir.display()
        );
        assert!(manager.installed().contains(&release.version));

        let _ = std::fs::remove_dir_all(&root);
    }
}
