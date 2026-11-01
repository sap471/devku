//! Lokasi folder data. Default-nya `%APPDATA%\DevKu`, tetapi pengguna boleh
//! memindahkannya ke drive lain. Penunjuknya sendiri tetap tinggal di folder
//! default supaya DevKu selalu bisa menemukan data walau data sudah pindah,
//! dan supaya folder data bisa dihapus tanpa membuat aplikasi buta arah.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::paths::write_atomic;

pub const POINTER_FILE: &str = "location.json";

/// Berkas penunjuk tidak ikut disalin: isinya justru akan menunjuk ke folder
/// yang salah setelah data berpindah.
const POINTER_SKIP: &[&str] = &[POINTER_FILE];

#[derive(Serialize, Deserialize)]
struct Pointer {
    home: String,
}

/// Asal folder data yang sedang dipakai, untuk ditampilkan di UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Default,
    Env,
    Pointer,
    /// Penunjuk ada tetapi tidak bisa dibaca. Data kembali ke folder default
    /// supaya aplikasi tetap bisa dibuka.
    Invalid,
}

impl Source {
    pub fn tag(self) -> &'static str {
        match self {
            Source::Default => "default",
            Source::Env => "environment",
            Source::Pointer => "custom",
            Source::Invalid => "invalid",
        }
    }
}

/// `DEVKU_HOME` diutamakan karena dipakai uji dan pemakaian portabel. Nilainya
/// kosong dianggap tidak diset.
pub fn env_home() -> Option<PathBuf> {
    std::env::var("DEVKU_HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn pointer_file(default_home: &Path) -> PathBuf {
    default_home.join(POINTER_FILE)
}

/// Dibaca tanpa panik: penunjuk rusak berarti kembali ke default, bukan
/// aplikasi yang gagal dibuka.
fn read_pointer(default_home: &Path) -> Result<Option<PathBuf>> {
    let file = pointer_file(default_home);
    if !file.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&file)
        .with_context(|| format!("gagal membaca {}", file.display()))?;
    let parsed: Pointer = serde_json::from_str(&raw)?;
    let home = parsed.home.trim().to_string();
    if home.is_empty() {
        bail!("penunjuk lokasi data tidak berisi folder");
    }
    Ok(Some(PathBuf::from(home)))
}

pub fn write_pointer(default_home: &Path, home: &Path) -> Result<()> {
    let pointer = Pointer {
        home: home.to_string_lossy().to_string(),
    };
    let json = serde_json::to_string_pretty(&pointer)?;
    write_atomic(&pointer_file(default_home), &json)
}

/// Folder data efektif beserta asalnya. `env` diterima sebagai parameter
/// supaya urutan prioritas bisa diuji tanpa menyentuh environment global.
pub fn resolve(default_home: &Path, env: Option<PathBuf>) -> (PathBuf, Source) {
    if let Some(custom) = env {
        return (custom, Source::Env);
    }
    match read_pointer(default_home) {
        Ok(Some(home)) => (home, Source::Pointer),
        Ok(None) => (default_home.to_path_buf(), Source::Default),
        Err(_) => (default_home.to_path_buf(), Source::Invalid),
    }
}

/// Windows tidak membedakan huruf besar/kecil pada jalur, jadi perbandingan
/// memakai bentuk yang sudah dinormalkan, bukan `Path::starts_with`.
fn normalized(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    text.trim_end_matches('\\').to_lowercase()
}

fn inside(child: &str, parent: &str) -> bool {
    child == parent || child.starts_with(&format!("{parent}\\"))
}

/// Aturan tujuan pemindahan. Folder tujuan wajib kosong supaya tidak ada
/// berkas pengguna lain yang tertimpa atau tercampur tanpa disadari.
pub fn validate_target(current: &Path, target: &Path) -> Result<()> {
    if target.as_os_str().is_empty() {
        bail!("folder tujuan belum dipilih");
    }
    if !target.is_absolute() {
        bail!("folder tujuan harus jalur absolut");
    }

    let current_norm = normalized(current);
    let target_norm = normalized(target);

    if current_norm == target_norm {
        bail!("folder data sudah berada di sana");
    }
    if inside(&target_norm, &current_norm) {
        bail!("folder tujuan tidak boleh berada di dalam folder data sekarang");
    }
    if inside(&current_norm, &target_norm) {
        bail!("folder tujuan tidak boleh menjadi induk folder data sekarang");
    }

    if target.exists() {
        if !target.is_dir() {
            bail!("{} bukan folder", target.display());
        }
        let mut entries = std::fs::read_dir(target)
            .with_context(|| format!("gagal membaca {}", target.display()))?;
        if entries.next().is_some() {
            bail!(
                "{} sudah berisi berkas; pilih folder kosong atau ketik nama folder baru",
                target.display()
            );
        }
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Totals {
    pub files: u64,
    pub bytes: u64,
}

fn entries(dir: &Path, top: bool) -> Result<Vec<std::fs::DirEntry>> {
    let mut out = Vec::new();
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("gagal membaca {}", dir.display()))?
    {
        let entry = entry?;
        if top && POINTER_SKIP.iter().any(|name| entry.file_name() == *name) {
            continue;
        }
        out.push(entry);
    }
    Ok(out)
}

fn measure(dir: &Path, top: bool, totals: &mut Totals) -> Result<()> {
    for entry in entries(dir, top)? {
        let meta = entry.metadata()?;
        if meta.is_dir() {
            measure(&entry.path(), false, totals)?;
        } else {
            totals.files += 1;
            totals.bytes += meta.len();
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn copy_dir(
    from: &Path,
    to: &Path,
    top: bool,
    done: &mut Totals,
    total: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<()> {
    std::fs::create_dir_all(to).with_context(|| format!("gagal membuat {}", to.display()))?;
    for entry in entries(from, top)? {
        let target = to.join(entry.file_name());
        let meta = entry.metadata()?;
        if meta.is_dir() {
            copy_dir(&entry.path(), &target, false, done, total, progress)?;
        } else {
            std::fs::copy(entry.path(), &target).with_context(|| {
                format!(
                    "gagal menyalin {} ke {}",
                    entry.path().display(),
                    target.display()
                )
            })?;
            done.files += 1;
            done.bytes += meta.len();
            progress(done.bytes, total);
        }
    }
    Ok(())
}

/// Menyalin seluruh isi folder data. Sumber tidak disentuh sampai pemanggil
/// berhasil menulis penunjuk, jadi kegagalan di tengah jalan tidak menghapus
/// apa pun.
pub fn copy_tree(from: &Path, to: &Path, progress: &mut dyn FnMut(u64, u64)) -> Result<Totals> {
    let mut total = Totals::default();
    measure(from, true, &mut total)?;

    let mut done = Totals::default();
    copy_dir(from, to, true, &mut done, total.bytes, progress)?;
    Ok(total)
}

/// Membuang isi folder lama setelah data pindah. Fungsinya best effort:
/// berkas yang masih dikunci proses lain tidak boleh membuat pemindahan
/// dianggap gagal, karena penunjuk sudah menunjuk ke folder baru.
pub fn prune_source(from: &Path) -> Result<()> {
    for entry in entries(from, true)? {
        let path = entry.path();
        let removed = if entry.file_type()?.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(err) = removed {
            eprintln!("gagal menghapus {}: {err}", path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("devku-location-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn pointer_roundtrip_wins_over_default() {
        let default = temp("pointer");
        let custom = default.join("pindah");
        std::fs::create_dir_all(&custom).unwrap();

        let before = resolve(&default, None);
        assert_eq!(before.1, Source::Default);

        write_pointer(&default, &custom).unwrap();
        let after = resolve(&default, None);
        assert_eq!(after.0, custom);
        assert_eq!(after.1, Source::Pointer);

        let _ = std::fs::remove_dir_all(&default);
    }

    #[test]
    fn environment_wins_over_pointer() {
        let default = temp("env");
        write_pointer(&default, &default.join("pindah")).unwrap();
        let env = default.join("env");

        let (home, source) = resolve(&default, Some(env.clone()));
        assert_eq!(home, env);
        assert_eq!(source, Source::Env);

        let _ = std::fs::remove_dir_all(&default);
    }

    #[test]
    fn broken_pointer_falls_back_to_default() {
        let default = temp("broken");
        std::fs::write(pointer_file(&default), "bukan json").unwrap();

        let (home, source) = resolve(&default, None);
        assert_eq!(home, default);
        assert_eq!(source, Source::Invalid);

        let _ = std::fs::remove_dir_all(&default);
    }

    #[test]
    fn target_must_be_absolute_and_empty() {
        let current = PathBuf::from("C:\\DevKu");
        assert!(validate_target(&current, Path::new("relatif")).is_err());
        assert!(validate_target(&current, &current).is_err());
        assert!(validate_target(&current, &current.join("anak")).is_err());
        assert!(validate_target(&current, &PathBuf::from("C:\\")).is_err());

        let dir = temp("empty");
        assert!(validate_target(&current, &dir).is_ok());
        std::fs::write(dir.join("isi.txt"), "x").unwrap();
        assert!(validate_target(&current, &dir).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copy_keeps_files_and_skips_pointer() {
        let default = temp("copy");
        let from = default.join("data");
        std::fs::create_dir_all(from.join("engines").join("php")).unwrap();
        std::fs::write(from.join("config.json"), "{}").unwrap();
        std::fs::write(from.join("engines").join("php").join("php.exe"), "bin").unwrap();
        write_pointer(&default, &from).unwrap();

        let to = default.join("baru");
        let totals = copy_tree(&from, &to, &mut |_, _| {}).unwrap();

        assert_eq!(totals.files, 2);
        assert!(to.join("config.json").exists());
        assert!(to.join("engines").join("php").join("php.exe").exists());
        assert!(!pointer_file(&to).exists());

        prune_source(&from).unwrap();
        assert!(pointer_file(&default).exists());
        assert!(!from.join("config.json").exists());
        assert!(from.exists());

        let _ = std::fs::remove_dir_all(&default);
    }
}
