use std::path::Path;

use anyhow::Result;

/// Nilai Run milik DevKu. Satu-satunya tempat yang menyuruh Windows membuka
/// aplikasi saat user login.
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "DevKu";
/// Tanpa flag ini jendela akan muncul saat login. Servis tetap dinyalakan
/// oleh aplikasi, jadi tray langsung aktif.
pub const HIDDEN_FLAG: &str = "--hidden";

/// Perintah yang ditulis ke registry. Path diberi tanda kutip karena lokasi
/// Program Files mengandung spasi.
pub fn command_line(exe: &Path) -> String {
    format!("\"{}\" {HIDDEN_FLAG}", exe.display())
}

/// Isi nilai Run saat ini, kalau ada. Dipakai juga oleh pengujian.
#[cfg(windows)]
pub fn current() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey(RUN_KEY).ok()?;
    key.get_value::<String, _>(VALUE_NAME).ok()
}

#[cfg(not(windows))]
pub fn current() -> Option<String> {
    None
}

/// Bandingkan tanpa peduli huruf besar/kecil karena Windows menyimpan path
/// apa adanya dan bisa mengubah kapitalisasi.
pub fn same_command(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

pub fn is_enabled(exe: &Path) -> bool {
    current()
        .map(|value| same_command(&value, &command_line(exe)))
        .unwrap_or(false)
}

/// Binary yang sedang berjalan. Di aplikasi ini `devku.exe`, jadi nilai Run
/// dibandingkan dengan jalur yang benar-benar dipakai sekarang.
pub fn running_exe() -> std::path::PathBuf {
    std::env::current_exe().unwrap_or_default()
}

#[cfg(windows)]
pub fn set_enabled(exe: &Path, enabled: bool) -> Result<()> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE)?;

    if enabled {
        key.set_value(VALUE_NAME, &command_line(exe))?;
    } else if key.get_value::<String, _>(VALUE_NAME).is_ok() {
        key.delete_value(VALUE_NAME)?;
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn set_enabled(_exe: &Path, _enabled: bool) -> Result<()> {
    anyhow::bail!("autostart saat login hanya didukung di Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_quotes_path_and_adds_hidden_flag() {
        let line = command_line(Path::new(r"C:\Program Files\DevKu\devku.exe"));
        assert_eq!(line, r#""C:\Program Files\DevKu\devku.exe" --hidden"#);
    }

    #[test]
    fn comparison_ignores_case_and_outer_spaces() {
        let line = command_line(Path::new(r"C:\Tools\devku.exe"));
        assert!(same_command(&format!(" {line} "), &line));
        assert!(same_command(&line.to_uppercase(), &line));
        assert!(!same_command(&format!("{line} --other"), &line));
    }

    /// Menulis nilai Run sungguhan, lalu memulihkannya (termasuk menghapus
    /// kalau sebelumnya tidak ada).
    #[test]
    #[cfg(windows)]
    #[ignore = "menulis key Run milik pengguna sungguhan"]
    fn set_then_clear_restores_registry() {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let before = hkcu
            .open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE)
            .ok()
            .and_then(|key| key.get_value::<String, _>(VALUE_NAME).ok());

        let exe = std::env::temp_dir().join("devku-autostart.exe");
        set_enabled(&exe, true).unwrap();
        assert!(is_enabled(&exe));

        set_enabled(&exe, false).unwrap();
        assert!(!is_enabled(&exe));

        if let Some(value) = before {
            set_enabled(Path::new(""), false).ok();
            let key = hkcu
                .open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE)
                .unwrap();
            key.set_value(VALUE_NAME, &value).unwrap();
        }
    }
}
