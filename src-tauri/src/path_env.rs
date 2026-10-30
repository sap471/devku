use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::paths::Paths;

/// Satu baris di PATH pengguna. Ditulis apa adanya (termasuk `%USERPROFILE%`
/// yang belum di-expand) supaya entri lain tidak berubah saat kita menambah
/// satu folder.
#[derive(Clone, Debug, Serialize)]
pub struct PathStatus {
    pub dir: String,
    pub registered: bool,
    /// Versi yang ditunjuk shim `php.bat`, kalau shim ada.
    pub shim_version: Option<String>,
    /// True kalau shim menunjuk versi yang folder/binary-nya sudah hilang.
    pub broken: bool,
}

pub fn bin_dir(paths: &Paths) -> PathBuf {
    paths.root.join("bin")
}

fn shim_file(paths: &Paths) -> PathBuf {
    bin_dir(paths).join("php.bat")
}

fn marker_file(paths: &Paths) -> PathBuf {
    bin_dir(paths).join("php.version")
}

pub fn status(paths: &Paths) -> PathStatus {
    let dir = bin_dir(paths);
    let shim_version = std::fs::read_to_string(marker_file(paths))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());

    let broken = match &shim_version {
        Some(version) => !paths.php_dir(version).join("php.exe").exists(),
        None => false,
    };

    PathStatus {
        dir: dir.to_string_lossy().to_string(),
        registered: user_path()
            .map(|text| contains(&text, &dir))
            .unwrap_or(false),
        shim_version,
        broken,
    }
}

/// Memasang shim `php.bat` yang meneruskan semua argumen ke versi tertentu,
/// lalu memastikan folder `bin` ada di PATH pengguna.
pub fn register(paths: &Paths, version: &str) -> Result<()> {
    let binary = paths.php_dir(version).join("php.exe");
    if !binary.exists() {
        anyhow::bail!("PHP {version} tidak terpasang di {}", binary.display());
    }

    write_shim(paths, version, &binary)?;

    let dir = bin_dir(paths);
    let text = load_user_path()?;
    if !contains(&text, &dir) {
        save_user_path(&append(&text, &dir))?;
        broadcast_change();
    }

    Ok(())
}

/// Menghapus folder `bin` dari PATH pengguna dan membuang shim-nya. Entri PATH
/// lain tidak disentuh.
pub fn unregister(paths: &Paths) -> Result<()> {
    let dir = bin_dir(paths);
    let text = load_user_path()?;
    let next = without(&text, &dir);
    if next != text {
        save_user_path(&next)?;
        broadcast_change();
    }

    for file in [shim_file(paths), marker_file(paths)] {
        if file.exists() {
            std::fs::remove_file(&file)
                .with_context(|| format!("gagal menghapus {}", file.display()))?;
        }
    }

    Ok(())
}

/// Dipanggil saat versi PHP default berubah. Kalau PATH belum terdaftar,
/// tidak ada yang perlu dilakukan.
pub fn sync_shim(paths: &Paths, version: &str) -> Result<()> {
    if !status(paths).registered {
        return Ok(());
    }
    let binary = paths.php_dir(version).join("php.exe");
    if binary.exists() {
        write_shim(paths, version, &binary)?;
    }
    Ok(())
}

fn write_shim(paths: &Paths, version: &str, binary: &Path) -> Result<()> {
    let dir = bin_dir(paths);
    std::fs::create_dir_all(&dir)?;

    // `%*` meneruskan argumen tanpa diubah, jadi `php -r "..."`, `php -S`,
    // dan `php artisan` berperilaku sama seperti memanggil binary aslinya.
    let script = format!("@echo off\r\n\"{}\" %*\r\n", binary.display());
    std::fs::write(shim_file(paths), script)?;
    std::fs::write(marker_file(paths), format!("{version}\n"))?;
    Ok(())
}

/// Memeriksa satu entri PATH tanpa peduli huruf besar/kecil, spasi, tanda
/// kutip, atau garis miring di ujung.
pub fn contains(path: &str, dir: &Path) -> bool {
    let wanted = normalize(&dir.to_string_lossy());
    path.split(';').any(|entry| normalize(entry) == wanted)
}

pub fn append(path: &str, dir: &Path) -> String {
    let entry = dir.to_string_lossy();
    if path.trim().is_empty() {
        return entry.to_string();
    }
    let separator = if path.trim_end().ends_with(';') {
        ""
    } else {
        ";"
    };
    format!("{}{separator}{entry}", path.trim_end())
}

pub fn without(path: &str, dir: &Path) -> String {
    let wanted = normalize(&dir.to_string_lossy());
    path.split(';')
        .filter(|entry| !entry.trim().is_empty() && normalize(entry) != wanted)
        .collect::<Vec<_>>()
        .join(";")
}

fn normalize(entry: &str) -> String {
    entry
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

/* ------------------------------------------------------------- Windows */

#[cfg(windows)]
mod store {
    use anyhow::{Context, Result};
    use winreg::RegKey;
    use winreg::RegValue;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, RegType};

    const ENV_KEY: &str = "Environment";
    const PATH_VALUE: &str = "Path";

    pub fn load() -> Result<String> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        load_at(&root, ENV_KEY, PATH_VALUE)
    }

    pub fn save(text: &str) -> Result<()> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        save_at(&root, ENV_KEY, PATH_VALUE, text)
    }

    pub(super) fn load_at(root: &RegKey, key: &str, value: &str) -> Result<String> {
        let key = root
            .open_subkey_with_flags(key, KEY_READ | KEY_WRITE)
            .context("gagal membuka Environment pengguna")?;

        match key.get_raw_value(value) {
            Ok(raw) => Ok(decode(&raw)),
            // Kunci ada tapi nilainya belum pernah dibuat.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(err) => Err(err).context("gagal membaca PATH pengguna"),
        }
    }

    /// PATH pengguna selalu ditulis sebagai REG_EXPAND_SZ berisi UTF-16, bentuk
    /// yang dipakai Windows sendiri. REG_SZ + byte UTF-16 adalah kombinasi yang
    /// pernah ditemukan di mesin nyata dan membuat PATH terbaca sebagai sampah,
    /// jadi representasi itu tidak pernah kita hasilkan.
    pub(super) fn save_at(root: &RegKey, key: &str, value: &str, text: &str) -> Result<()> {
        let key = root
            .create_subkey(key)
            .map(|(key, _)| key)
            .context("gagal membuka Environment pengguna")?;

        let mut bytes = Vec::with_capacity(text.len() * 2 + 2);
        for unit in text.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }

        key.set_raw_value(
            value,
            &RegValue {
                bytes: bytes.into(),
                vtype: RegType::REG_EXPAND_SZ,
            },
        )
        .context("gagal menulis PATH pengguna")
    }

    pub(super) fn decode(raw: &RegValue) -> String {
        let text = if raw.vtype == RegType::REG_SZ && !looks_utf16(&raw.bytes) {
            String::from_utf8_lossy(&raw.bytes).to_string()
        } else {
            let units: Vec<u16> = raw
                .bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        };
        text.trim_end_matches('\0').to_string()
    }

    /// Nilai REG_SZ semestinya berisi byte ANSI, tapi ada mesin yang menyimpan
    /// UTF-16 di dalamnya; PATH di mesin ini pernah begitu dan terbaca sebagai
    /// sampah. Bila seperempat isinya NUL, itu UTF-16, bukan teks ANSI.
    fn looks_utf16(bytes: &[u8]) -> bool {
        if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
            return false;
        }
        let zeros = bytes.iter().filter(|byte| **byte == 0).count();
        zeros * 4 > bytes.len()
    }

    /// Explorer (dan terminal yang dibukanya) hanya membaca environment saat
    /// menerima pesan ini. Tanpa broadcast, `php` baru terlihat setelah logoff.
    pub fn broadcast() {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
        };
        use windows::core::HSTRING;

        let payload = HSTRING::from("Environment");
        let mut result = 0usize;
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(payload.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                Some(&mut result),
            );
        }
    }
}

#[cfg(windows)]
use store::{load as load_env, save as save_env};

#[cfg(windows)]
fn load_user_path() -> Result<String> {
    load_env()
}

#[cfg(windows)]
fn save_user_path(text: &str) -> Result<()> {
    save_env(text)
}

#[cfg(windows)]
fn user_path() -> Result<String> {
    load_env()
}

#[cfg(windows)]
fn broadcast_change() {
    store::broadcast();
}

#[cfg(not(windows))]
fn user_path() -> Result<String> {
    std::env::var("PATH").context("PATH tidak terbaca")
}

#[cfg(not(windows))]
fn load_user_path() -> Result<String> {
    user_path()
}

#[cfg(not(windows))]
fn save_user_path(_text: &str) -> Result<()> {
    anyhow::bail!("mengubah PATH otomatis hanya didukung di Windows")
}

#[cfg(not(windows))]
fn broadcast_change() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_adds_separator_only_when_needed() {
        let dir = Path::new(r"C:\Users\aku\AppData\Roaming\DevKu\bin");
        assert_eq!(append("", dir), dir.to_string_lossy());
        assert_eq!(
            append(r"C:\Windows;", dir),
            format!(r"C:\Windows;{}", dir.display())
        );
        assert_eq!(
            append(r"C:\Windows", dir),
            format!(r"C:\Windows;{}", dir.display())
        );
    }

    #[test]
    fn contains_ignores_case_quotes_and_trailing_slash() {
        let dir = Path::new(r"C:\Users\aku\AppData\Roaming\DevKu\bin");
        let quoted = format!(
            "C:\\Windows;\"{}\"",
            r"C:\USERS\aku\appdata\roaming\devku\bin\"
        );
        assert!(contains(&quoted, dir));
        assert!(!contains(r"C:\Windows;C:\DevKu\bin", dir));
    }

    #[test]
    fn without_keeps_other_entries_and_drops_empty_ones() {
        let dir = Path::new(r"C:\DevKu\bin");
        assert_eq!(
            without(r"C:\Windows;;C:\DevKu\bin;C:\Tools", dir),
            r"C:\Windows;C:\Tools"
        );
    }

    #[cfg(windows)]
    #[test]
    fn registry_round_trip_keeps_expand_variables() {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;

        let sub = r"Software\DevKu\test-env";
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let dir = Path::new(r"%USERPROFILE%\DevKu\bin");

        let original = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps";
        store::save_at(&root, sub, "Path", original).unwrap();

        let text = store::load_at(&root, sub, "Path").unwrap();
        assert_eq!(text, original);

        let updated = append(&text, dir);
        store::save_at(&root, sub, "Path", &updated).unwrap();

        let again = store::load_at(&root, sub, "Path").unwrap();
        assert_eq!(again, format!(r"{original};%USERPROFILE%\DevKu\bin"));

        let cleaned = without(&again, dir);
        assert_eq!(cleaned, original);

        let _ = root.delete_subkey_all(sub);
    }

    #[test]
    fn utf16_bytes_inside_reg_sz_still_decode_as_text() {
        // Nilai seperti ini pernah ditemukan di PATH pengguna: byte UTF-16
        // dengan label REG_SZ. Dibaca sebagai ANSI hasilnya penuh NUL.
        let text = r"C:\Windows;C:\Tools";
        let mut bytes = Vec::new();
        for unit in text.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let raw = winreg::RegValue {
            bytes: bytes.into(),
            vtype: winreg::enums::RegType::REG_SZ,
        };
        assert_eq!(store::decode(&raw), text);
    }

    #[test]
    fn plain_ansi_reg_sz_is_not_mistaken_for_utf16() {
        let raw = winreg::RegValue {
            bytes: b"C:\\Windows\\System32\0".to_vec().into(),
            vtype: winreg::enums::RegType::REG_SZ,
        };
        assert_eq!(store::decode(&raw), r"C:\Windows\System32");
    }

    /// Jalankan dengan `cargo test --lib register_then_unregister -- --ignored`.
    /// Test ini menulis PATH pengguna sungguhan, lalu mengembalikannya persis
    /// seperti semula (Drop tetap memulihkan walau assert gagal).
    #[cfg(windows)]
    #[test]
    #[ignore = "menulis PATH pengguna sungguhan"]
    fn register_then_unregister_restores_user_path() {
        let root = std::env::temp_dir().join(format!("devku-pathenv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths { root: root.clone() };
        let version = "8.3.36";
        let dir = paths.php_dir(version);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("php.exe"), b"").unwrap();

        let original = load_user_path().unwrap();
        struct Restore {
            original: String,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = save_user_path(&self.original);
            }
        }
        let _restore = Restore {
            original: original.clone(),
        };

        register(&paths, version).unwrap();

        let current = load_user_path().unwrap();
        assert!(contains(&current, &bin_dir(&paths)));
        let state = status(&paths);
        assert!(state.registered);
        assert_eq!(state.shim_version.as_deref(), Some(version));
        assert!(!state.broken);

        let shim = std::fs::read_to_string(shim_file(&paths)).unwrap();
        assert!(shim.contains("php.exe"));
        assert!(shim.contains("%*"));

        unregister(&paths).unwrap();

        let after = load_user_path().unwrap();
        assert_eq!(after, original);
        assert!(!status(&paths).registered);
        assert!(!shim_file(&paths).exists());

        let _ = std::fs::remove_dir_all(&root);
    }
}
