use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::paths::Paths;

/// One entry in the user PATH. Written verbatim (including `%USERPROFILE%`
/// that is not yet expanded) so other entries stay unchanged when we add
/// a folder.
#[derive(Clone, Debug, Serialize)]
pub struct PathStatus {
    pub dir: String,
    pub registered: bool,
    /// Version the `php.bat` shim points at, if the shim exists.
    pub shim_version: Option<String>,
    /// True when the shim points at a version whose folder/binary is gone.
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
        registered: is_registered(&dir),
        shim_version,
        broken,
    }
}

/// Whether a folder is already in the user PATH. Also used by the global
/// Composer path, which does not go through the DevKu shim.
pub fn is_registered(dir: &Path) -> bool {
    user_path()
        .map(|text| contains(&text, dir))
        .unwrap_or(false)
}

/// Adds one folder to the user PATH without touching other entries. If it is
/// already there, nothing is rewritten.
pub fn add_dir(dir: &Path) -> Result<()> {
    let text = load_user_path()?;
    if !contains(&text, dir) {
        save_user_path(&append(&text, dir))?;
        broadcast_change();
    }
    Ok(())
}

/// Removes one folder from the user PATH. Other entries are left as they are.
pub fn remove_dir(dir: &Path) -> Result<()> {
    let text = load_user_path()?;
    let next = without(&text, dir);
    if next != text {
        save_user_path(&next)?;
        broadcast_change();
    }
    Ok(())
}

/// Installs a `php.bat` shim that forwards every argument to a given version,
/// then makes sure the `bin` folder is in the user PATH.
pub fn register(paths: &Paths, version: &str) -> Result<()> {
    let binary = paths.php_dir(version).join("php.exe");
    if !binary.exists() {
        anyhow::bail!("PHP {version} tidak terpasang di {}", binary.display());
    }

    write_shim(paths, version, &binary)?;
    add_dir(&bin_dir(paths))
}

/// Deletes the `bin` folder from the user PATH and discards its shim. Other
/// PATH entries are left alone.
pub fn unregister(paths: &Paths) -> Result<()> {
    remove_dir(&bin_dir(paths))?;

    for file in [shim_file(paths), marker_file(paths)] {
        if file.exists() {
            std::fs::remove_file(&file)
                .with_context(|| format!("gagal menghapus {}", file.display()))?;
        }
    }

    Ok(())
}

/// Called when the default PHP version changes. If the PATH entry is not
/// registered, there is nothing to do.
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

    // `%*` forwards arguments unchanged, so `php -r "..."`, `php -S`,
    // and `php artisan` behave as if calling the real binary.
    let script = format!("@echo off\r\n\"{}\" %*\r\n", binary.display());
    std::fs::write(shim_file(paths), script)?;
    std::fs::write(marker_file(paths), format!("{version}\n"))?;
    Ok(())
}

/// Checks a single PATH entry ignoring case, spaces, quotes, and a trailing
/// slash.
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
            // The key exists but its value was never created.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(err) => Err(err).context("gagal membaca PATH pengguna"),
        }
    }

    /// The user PATH is always written as REG_EXPAND_SZ holding UTF-16, the form
    /// Windows itself uses. REG_SZ plus UTF-16 bytes is a combination seen on
    /// real machines that makes PATH read as garbage, so we never produce that
    /// representation.
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

    /// A REG_SZ value should hold ANSI bytes, but some machines store UTF-16 in
    /// it; the PATH on this machine was once like that and read as garbage. If a
    /// quarter of the content is NUL, it is UTF-16, not ANSI text.
    fn looks_utf16(bytes: &[u8]) -> bool {
        if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
            return false;
        }
        let zeros = bytes.iter().filter(|byte| **byte == 0).count();
        zeros * 4 > bytes.len()
    }

    /// Explorer (and terminals it spawns) only read the environment when they
    /// receive this message. Without a broadcast, `php` only appears after logoff.
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
        // This kind of value was found in a user PATH: UTF-16 bytes labelled
        // REG_SZ. Read as ANSI the result is full of NULs.
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

    /// Run with `cargo test --lib register_then_unregister -- --ignored`.
    /// This test writes the real user PATH, then restores it exactly as it was
    /// (Drop still restores even if an assert fails).
    #[cfg(windows)]
    #[test]
    #[ignore = "writes the real user PATH"]
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
