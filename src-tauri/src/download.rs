use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Serialize;
use sha2::{Digest, Sha256, Sha512};

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

/// Berkas rilis GitHub yang siap diunduh. Caddy dan Bun memakai alur yang sama,
/// hanya nama berkasnya yang berbeda.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RemoteAsset {
    pub target: &'static str,
    pub version: String,
    pub filename: String,
    pub url: String,
    pub size: u64,
}

const GITHUB_API: &str = "https://api.github.com/repos";

pub async fn caddy_latest(client: &Client) -> Result<RemoteAsset> {
    let release = github_release(client, "caddyserver/caddy").await?;
    let suffix = if cfg!(target_arch = "aarch64") {
        "windows_arm64.zip"
    } else {
        "windows_amd64.zip"
    };

    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(suffix))
        .with_context(|| format!("rilis Caddy {} tidak menyediakan {suffix}", release.tag))?;

    Ok(RemoteAsset {
        target: "caddy",
        version: release.tag.trim_start_matches('v').to_string(),
        filename: asset.name.clone(),
        url: asset.url.clone(),
        size: asset.size,
    })
}

pub async fn bun_latest(client: &Client) -> Result<RemoteAsset> {
    let release = github_release(client, "oven-sh/bun").await?;
    let wanted = if cfg!(target_arch = "aarch64") {
        "bun-windows-aarch64.zip"
    } else {
        "bun-windows-x64.zip"
    };

    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == wanted)
        .with_context(|| format!("rilis Bun {} tidak menyediakan {wanted}", release.tag))?;

    Ok(RemoteAsset {
        target: "bun",
        // Tag Bun berbentuk `bun-v1.4.4`.
        version: release
            .tag
            .trim_start_matches("bun-")
            .trim_start_matches('v')
            .to_string(),
        filename: asset.name.clone(),
        url: asset.url.clone(),
        size: asset.size,
    })
}

struct GhRelease {
    tag: String,
    assets: Vec<GhAsset>,
}

struct GhAsset {
    name: String,
    url: String,
    size: u64,
}

async fn github_release(client: &Client, repo: &str) -> Result<GhRelease> {
    let text = client
        .get(format!("{GITHUB_API}/{repo}/releases/latest"))
        .send()
        .await
        .with_context(|| format!("gagal menghubungi api.github.com untuk {repo}"))?
        .error_for_status()
        .with_context(|| format!("GitHub menolak permintaan rilis {repo}"))?
        .text()
        .await
        .context("gagal membaca data rilis GitHub")?;

    parse_github_release(&text)
}

fn parse_github_release(text: &str) -> Result<GhRelease> {
    let root: serde_json::Value =
        serde_json::from_str(text).context("bentuk rilis GitHub tidak dikenali")?;

    let tag = root
        .get("tag_name")
        .and_then(|value| value.as_str())
        .context("rilis GitHub tanpa tag")?
        .to_string();

    let assets = root
        .get("assets")
        .and_then(|value| value.as_array())
        .context("rilis GitHub tanpa daftar berkas")?
        .iter()
        .filter_map(|asset| {
            Some(GhAsset {
                name: asset.get("name")?.as_str()?.to_string(),
                url: asset.get("browser_download_url")?.as_str()?.to_string(),
                size: asset
                    .get("size")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0),
            })
        })
        .collect();

    Ok(GhRelease { tag, assets })
}

/// Caddy menerbitkan `caddy_<versi>_checksums.txt` berisi satu baris per berkas.
pub async fn caddy_checksum(client: &Client, version: &str, filename: &str) -> Result<String> {
    let url = format!(
        "https://github.com/caddyserver/caddy/releases/download/v{version}/caddy_{version}_checksums.txt"
    );
    let text = client
        .get(url)
        .send()
        .await
        .context("gagal mengunduh daftar checksum Caddy")?
        .error_for_status()
        .context("daftar checksum Caddy tidak tersedia")?
        .text()
        .await
        .context("gagal membaca daftar checksum Caddy")?;

    checksum_for(&text, filename)
        .with_context(|| format!("{filename} tidak ada di daftar checksum Caddy"))
}

pub fn checksum_for(text: &str, filename: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let digest = parts.next()?;
        let name = parts.next()?;
        // `sha256sum` menulis `*nama` untuk mode biner dan `./nama` untuk
        // daftar yang dibuat dari dalam folder, seperti milik MariaDB.
        let name = name.trim_start_matches('*').trim_start_matches("./");
        (name == filename).then(|| digest.to_string())
    })
}

pub async fn mariadb_checksum(client: &Client) -> Result<String> {
    let version = crate::database::VERSION;
    let filename = crate::database::archive_name(version);

    let text = client
        .get(crate::database::checksums_url(version))
        .send()
        .await
        .context("gagal mengunduh daftar checksum MariaDB")?
        .error_for_status()
        .context("daftar checksum MariaDB tidak tersedia")?
        .text()
        .await
        .context("gagal membaca daftar checksum MariaDB")?;

    checksum_for(&text, &filename)
        .with_context(|| format!("{filename} tidak ada di daftar checksum MariaDB"))
}

/// Mengunduh ke `dest` sambil menghitung checksum. Algoritmanya dipilih dari
/// panjang nilai yang diumumkan (64 = SHA-256, 128 = SHA-512). Berkas yang tidak
/// cocok langsung dihapus, jadi yang tersisa di disk selalu bisa dipercaya.
///
/// Arsip yang sudah ada dan cocok dilewati tanpa menyentuh jaringan. Pemasangan
/// ulang versi yang sama jadi tidak memakai kuota.
pub async fn download(
    client: &Client,
    url: &str,
    dest: &Path,
    expected: Option<&str>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if archive_is_trusted(dest, expected) {
        let size = std::fs::metadata(dest).map(|meta| meta.len()).unwrap_or(0);
        progress(size, Some(size));
        return Ok(());
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
    let mut hasher = Hasher::for_expected(expected)?;
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

    hasher.verify(dest, expected)?;

    // Catat sha256 supaya pemasangan berikutnya bisa memakai berkas ini tanpa
    // mengunduh lagi, termasuk paket yang tidak punya checksum resmi.
    if let Ok(digest) = digest_of(dest, 64) {
        let _ = crate::paths::write_atomic(&sidecar(dest), &format!("{digest}\n"));
    }

    progress(received, total);
    Ok(())
}

/// Arsip dianggap siap pakai kalau checksum-nya cocok dengan yang diumumkan,
/// atau dengan catatan dari unduhan sebelumnya kalau tidak ada yang diumumkan.
fn archive_is_trusted(dest: &Path, expected: Option<&str>) -> bool {
    if !dest.exists() {
        return false;
    }

    if let Some(value) = expected.map(str::trim).filter(|value| !value.is_empty()) {
        return digest_of(dest, value.len())
            .map(|digest| digest.eq_ignore_ascii_case(value))
            .unwrap_or(false);
    }

    let recorded = std::fs::read_to_string(sidecar(dest))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());

    let Some(recorded) = recorded else {
        return false;
    };

    digest_of(dest, 64)
        .map(|digest| digest.eq_ignore_ascii_case(&recorded))
        .unwrap_or(false)
}

fn sidecar(path: &Path) -> PathBuf {
    path.with_extension("sha256")
}

fn digest_of(path: &Path, hex_length: usize) -> Result<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut buffer = vec![0u8; 64 * 1024];

    let digest = if hex_length == 128 {
        let mut hasher = Sha512::new();
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        hex(&hasher.finalize())
    } else {
        let mut hasher = Sha256::new();
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        hex(&hasher.finalize())
    };

    Ok(digest)
}

/// Jumlah dan ukuran arsip yang tersimpan, untuk ditampilkan di UI.
pub fn cached_archives(paths: &crate::paths::Paths) -> (usize, u64) {
    let mut files = 0;
    let mut bytes = 0;

    if let Ok(entries) = std::fs::read_dir(paths.downloads()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("sha256") {
                continue;
            }
            if let Ok(meta) = entry.metadata()
                && meta.is_file()
            {
                files += 1;
                bytes += meta.len();
            }
        }
    }

    (files, bytes)
}

/// Membuang arsip tersimpan. Semuanya bisa diunduh lagi saat dibutuhkan.
pub fn clear_archives(paths: &crate::paths::Paths) -> Result<()> {
    let dir = paths.downloads();
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().is_file() {
            std::fs::remove_file(entry.path())
                .with_context(|| format!("gagal menghapus {}", entry.path().display()))?;
        }
    }

    Ok(())
}

enum Hasher {
    Sha256(Sha256),
    Sha512(Sha512),
    Nothing,
}

impl Hasher {
    fn for_expected(expected: Option<&str>) -> Result<Self> {
        match expected.map(str::trim).filter(|value| !value.is_empty()) {
            None => Ok(Self::Nothing),
            Some(value) if value.len() == 64 => Ok(Self::Sha256(Sha256::new())),
            Some(value) if value.len() == 128 => Ok(Self::Sha512(Sha512::new())),
            Some(value) => bail!("checksum tidak dikenali: {value}"),
        }
    }

    fn update(&mut self, chunk: &[u8]) {
        match self {
            Self::Sha256(hasher) => hasher.update(chunk),
            Self::Sha512(hasher) => hasher.update(chunk),
            Self::Nothing => {}
        }
    }

    fn verify(self, dest: &Path, expected: Option<&str>) -> Result<()> {
        let Some(expected) = expected else {
            return Ok(());
        };

        let digest = match self {
            Self::Sha256(hasher) => hex(&hasher.finalize()),
            Self::Sha512(hasher) => hex(&hasher.finalize()),
            Self::Nothing => return Ok(()),
        };

        if !digest.eq_ignore_ascii_case(expected.trim()) {
            let _ = std::fs::remove_file(dest);
            bail!("checksum tidak cocok (dapat {digest}); berkas unduhan dibuang");
        }
        Ok(())
    }
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

    const GH: &str = r#"{"tag_name":"v2.11.7","assets":[
        {"name":"caddy_2.11.7_checksums.txt","browser_download_url":"https://x/checksums","size":10},
        {"name":"caddy_2.11.7_windows_amd64.zip","browser_download_url":"https://x/win.zip","size":18479498},
        {"name":"caddy_2.11.7_windows_amd64.sbom","browser_download_url":"https://x/sbom","size":20}
    ]}"#;

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
    fn github_release_reads_tag_and_assets() {
        let release = parse_github_release(GH).unwrap();
        assert_eq!(release.tag, "v2.11.7");
        assert_eq!(release.assets.len(), 3);
        assert_eq!(release.assets[1].size, 18479498);
        assert_eq!(release.assets[1].url, "https://x/win.zip");
    }

    #[test]
    fn caddy_asset_ends_with_windows_amd64_zip() {
        let release = parse_github_release(GH).unwrap();
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name.ends_with("windows_amd64.zip"))
            .unwrap();
        assert_eq!(asset.name, "caddy_2.11.7_windows_amd64.zip");
    }

    #[test]
    fn checksum_for_matches_exact_filename_only() {
        let text = "abc  caddy_2.11.7_linux_amd64.tar.gz\ndef  caddy_2.11.7_windows_amd64.zip\n";
        assert_eq!(
            checksum_for(text, "caddy_2.11.7_windows_amd64.zip").as_deref(),
            Some("def")
        );
        assert!(checksum_for(text, "caddy_2.11.7_windows_amd64.sbom").is_none());
    }

    #[test]
    fn hasher_uses_algorithm_from_digest_length() {
        assert!(matches!(
            Hasher::for_expected(Some(&"a".repeat(64))).unwrap(),
            Hasher::Sha256(_)
        ));
        assert!(matches!(
            Hasher::for_expected(Some(&"a".repeat(128))).unwrap(),
            Hasher::Sha512(_)
        ));
        assert!(matches!(
            Hasher::for_expected(None).unwrap(),
            Hasher::Nothing
        ));
        assert!(Hasher::for_expected(Some("abc")).is_err());
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

    /// Jalankan dengan `cargo test --lib installs_real_caddy -- --ignored`.
    /// Mengunduh zip Caddy (~18 MB), memverifikasi SHA-512, lalu menjalankan
    /// binary hasil ekstraksi.
    #[test]
    #[ignore = "butuh jaringan"]
    fn installs_real_caddy_release() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let client = client().unwrap();

        runtime.block_on(async {
            let asset = caddy_latest(&client).await.unwrap();
            let checksum = caddy_checksum(&client, &asset.version, &asset.filename)
                .await
                .unwrap();

            let root = std::env::temp_dir().join(format!("devku-caddy-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let paths = crate::paths::Paths { root: root.clone() };

            let archive = paths.downloads().join(&asset.filename);
            let cancel = AtomicBool::new(false);
            download(
                &client,
                &asset.url,
                &archive,
                Some(&checksum),
                &cancel,
                |_, _| {},
            )
            .await
            .unwrap();

            crate::archive::extract(&archive, &paths.caddy_dir()).unwrap();
            assert!(paths.caddy_binary().exists(), "caddy.exe tidak ada");

            let output = std::process::Command::new(paths.caddy_binary())
                .arg("version")
                .output()
                .unwrap();
            assert!(output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stdout).contains(&asset.version),
                "versi tidak cocok"
            );

            let _ = std::fs::remove_dir_all(&root);
        });
    }

    /// Jalankan dengan `cargo test --lib installs_real_bun -- --ignored`.
    /// Mengunduh zip Bun (~55 MB) dan menjalankan binary hasil ekstraksi.
    #[test]
    #[ignore = "butuh jaringan"]
    fn installs_real_bun_release() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let client = client().unwrap();

        runtime.block_on(async {
            let asset = bun_latest(&client).await.unwrap();

            let root = std::env::temp_dir().join(format!("devku-bun-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let paths = crate::paths::Paths { root: root.clone() };

            let archive = paths.downloads().join(&asset.filename);
            let cancel = AtomicBool::new(false);
            download(&client, &asset.url, &archive, None, &cancel, |_, _| {})
                .await
                .unwrap();

            crate::archive::extract(&archive, &paths.engines().join("bun")).unwrap();
            let binary = paths.bundled_binary("bun");
            assert!(binary.exists(), "bun.exe tidak ada di {}", binary.display());

            let output = std::process::Command::new(&binary)
                .arg("--version")
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                asset.version
            );

            let _ = std::fs::remove_dir_all(&root);
        });
    }

    /// Jalankan dengan `cargo test --lib resolves_caddy -- --ignored`.
    /// Hanya membaca metadata rilis, tidak mengunduh zip-nya.
    #[test]
    #[ignore = "butuh jaringan"]
    fn resolves_caddy_and_bun_releases() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let client = client().unwrap();

        let caddy = runtime.block_on(caddy_latest(&client)).unwrap();
        assert!(caddy.version.starts_with("2."), "versi {}", caddy.version);
        assert!(caddy.filename.ends_with("windows_amd64.zip"));
        assert!(caddy.size > 10_000_000, "ukuran {}", caddy.size);

        let checksum = runtime
            .block_on(caddy_checksum(&client, &caddy.version, &caddy.filename))
            .unwrap();
        assert_eq!(checksum.len(), 128, "Caddy memakai SHA-512");

        let bun = runtime.block_on(bun_latest(&client)).unwrap();
        assert_eq!(bun.version.split('.').count(), 3, "versi {}", bun.version);
        assert!(bun.filename.starts_with("bun-windows-"));
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
                Some(&release.sha256),
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

    fn cache_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("devku-download-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn matching_archive_is_reused_without_touching_the_network() {
        let dir = cache_dir("cache-hit");
        let dest = dir.join("paket.zip");
        std::fs::write(&dest, b"isi arsip uji").unwrap();
        let digest = digest_of(&dest, 64).unwrap();

        let client = reqwest::Client::new();
        let cancel = AtomicBool::new(false);
        let mut reported = 0u64;

        // URL yang tidak melayani apa pun: kalau unduhan tetap dicoba, tes gagal.
        let result = download(
            &client,
            "http://127.0.0.1:9/paket.zip",
            &dest,
            Some(&digest),
            &cancel,
            |received, _| reported = received,
        )
        .await;

        assert!(result.is_ok(), "{result:?}");
        assert_eq!(reported, std::fs::metadata(&dest).unwrap().len());
        assert_eq!(std::fs::read(&dest).unwrap(), b"isi arsip uji");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn archive_with_wrong_checksum_is_downloaded_again() {
        let dir = cache_dir("cache-miss");
        let dest = dir.join("paket.zip");
        std::fs::write(&dest, b"isi arsip uji").unwrap();
        let wrong = "0".repeat(64);

        let client = reqwest::Client::new();
        let cancel = AtomicBool::new(false);

        let result = download(
            &client,
            "http://127.0.0.1:9/paket.zip",
            &dest,
            Some(&wrong),
            &cancel,
            |_, _| {},
        )
        .await;

        assert!(result.is_err(), "checksum salah tidak boleh dipercaya");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn recorded_digest_is_enough_when_no_checksum_is_announced() {
        let dir = cache_dir("cache-sidecar");
        let dest = dir.join("bun.zip");
        std::fs::write(&dest, b"bun").unwrap();
        let digest = digest_of(&dest, 64).unwrap();
        std::fs::write(sidecar(&dest), format!("{digest}\n")).unwrap();

        let client = reqwest::Client::new();
        let cancel = AtomicBool::new(false);

        let result = download(
            &client,
            "http://127.0.0.1:9/bun.zip",
            &dest,
            None,
            &cancel,
            |_, _| {},
        )
        .await;

        assert!(result.is_ok(), "{result:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn archive_without_record_is_never_trusted() {
        let dir = cache_dir("cache-unknown");
        let dest = dir.join("bun.zip");
        std::fs::write(&dest, b"bun").unwrap();

        let client = reqwest::Client::new();
        let cancel = AtomicBool::new(false);

        let result = download(
            &client,
            "http://127.0.0.1:9/bun.zip",
            &dest,
            None,
            &cancel,
            |_, _| {},
        )
        .await;

        assert!(
            result.is_err(),
            "tanpa catatan checksum, unduhan harus dicoba"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
