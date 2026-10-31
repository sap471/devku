use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::paths::Paths;
use crate::proc::ServiceSpec;

/// Versi yang dipatok. 11.4 adalah LTS dengan masa dukung terpanjang dan
/// kompatibilitas paling lebar untuk tooling PHP.
pub const VERSION: &str = "11.4.13";

const MIRROR: &str = "https://archive.mariadb.org";

/// Skema sistem yang tidak ikut dibackup: memulihkannya ke instalasi baru
/// justru membuat konflik hak akses.
const SYSTEM_SCHEMAS: &[&str] = &["mysql", "performance_schema", "information_schema", "sys"];

pub fn archive_name(version: &str) -> String {
    format!("mariadb-{version}-winx64.zip")
}

pub fn archive_url(version: &str) -> String {
    format!(
        "{MIRROR}/mariadb-{version}/winx64-packages/{}",
        archive_name(version)
    )
}

pub fn checksums_url(version: &str) -> String {
    format!("{MIRROR}/mariadb-{version}/winx64-packages/sha256sums.txt")
}

/// Kontrak server database lokal. MariaDB satu-satunya implementasi sekarang;
/// PostgreSQL nanti cukup menambah implementasi lain tanpa menyentuh pemanggil.
pub trait DatabaseBackend: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn bin_dir(&self) -> PathBuf;
    fn data_dir(&self) -> PathBuf;
    fn is_installed(&self) -> bool;
    fn is_initialized(&self) -> bool;
    /// Menyiapkan datadir kosong. Dipanggil sekali setelah arsip diekstrak.
    fn initialize(&self, port: u16) -> Result<()>;
    /// Service normal.
    fn spec(&self, port: u16) -> Result<ServiceSpec>;
    /// Service dengan grant table dimatikan, untuk memulihkan password yang
    /// sudah tidak diketahui.
    fn recovery_spec(&self, port: u16) -> Result<ServiceSpec>;
    fn password(&self) -> String;
    fn store_password(&self, password: &str) -> Result<()>;
    fn query(&self, port: u16, password: &str, sql: &str) -> Result<String>;
    fn root_hosts(&self, port: u16, password: &str) -> Result<Vec<String>>;
    /// Mengubah password di semua akun root sekaligus. Mengubah satu baris saja
    /// tidak berpengaruh ke login TCP, karena `127.0.0.1` cocok ke akun sendiri.
    fn set_password(&self, port: u16, current: &str, target: &str) -> Result<()>;
    fn user_databases(&self, port: u16, password: &str) -> Result<Vec<String>>;
    fn dump(&self, port: u16, password: &str, dest: &Path) -> Result<()>;
    /// Membuang isi datadir.
    fn wipe(&self) -> Result<()>;
}

pub struct Mariadb {
    paths: Paths,
}

impl Mariadb {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    fn base_dir(&self) -> PathBuf {
        self.paths.mariadb_dir()
    }

    fn binary(&self, name: &str) -> PathBuf {
        self.paths.mariadb_bin().join(name)
    }

    fn sql_literal(value: &str) -> String {
        value.replace('\'', "''")
    }

    /// Akun root yang benar-benar bisa dipakai. Server berjalan dengan
    /// `skip-name-resolve`, jadi baris ber-host nama mesin diabaikan dan
    /// `ALTER USER` untuknya gagal dengan ERROR 1396.
    fn usable_root_host(host: &str) -> bool {
        host == "localhost" || host == "%" || host.parse::<std::net::IpAddr>().is_ok()
    }

    /// Pesan galat klien memuat peringatan passwordless yang bukan kesalahan.
    fn clean_error(stderr: &str) -> String {
        stderr
            .lines()
            .filter(|line| !line.trim_start().starts_with("WARNING:"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    /// Berkas konfigurasi milik DevKu. `my.ini` yang ditinggalkan penginisialisasi
    /// memuat jalur absolut ke folder saat itu, jadi tidak dipakai.
    fn write_config(&self, port: u16) -> Result<PathBuf> {
        let file = self.paths.mariadb_config();
        let base = self.base_dir();
        let text = format!(
            "[mysqld]\n\
             basedir={base}\n\
             datadir={data}\n\
             port={port}\n\
             bind-address=127.0.0.1\n\
             skip-name-resolve\n\
             plugin-dir={base}\\lib\\plugin\n\
             character-sets-dir={base}\\share\\charsets\n",
            base = base.display(),
            data = self.paths.mariadb_data().display(),
        );
        crate::paths::write_atomic(&file, &text)?;
        Ok(file)
    }

    fn spec_with(&self, port: u16, grant_tables: bool) -> Result<ServiceSpec> {
        if !self.is_installed() {
            bail!("MariaDB belum terpasang");
        }
        if !self.is_initialized() {
            bail!("database belum disiapkan");
        }

        let config = self.write_config(port)?;
        let mut args = vec![
            format!("--defaults-file={}", config.display()),
            "--console".to_string(),
        ];
        if !grant_tables {
            args.push("--skip-grant-tables".to_string());
        }

        Ok(ServiceSpec {
            name: "mariadb".to_string(),
            program: self.binary("mysqld.exe"),
            args,
            env: Vec::new(),
            cwd: Some(self.paths.mariadb_bin()),
            port: Some(port),
            log_dir: self.paths.logs(),
        })
    }
}

impl DatabaseBackend for Mariadb {
    fn id(&self) -> &'static str {
        "mariadb"
    }

    fn label(&self) -> &'static str {
        "MariaDB"
    }

    fn version(&self) -> &'static str {
        VERSION
    }

    fn bin_dir(&self) -> PathBuf {
        self.paths.mariadb_bin()
    }

    fn data_dir(&self) -> PathBuf {
        self.paths.mariadb_data()
    }

    fn is_installed(&self) -> bool {
        // server.dll wajib ada di samping stub mysqld.exe.
        self.binary("mysqld.exe").exists() && self.binary("server.dll").exists()
    }

    fn is_initialized(&self) -> bool {
        self.paths.mariadb_data().join("mysql").is_dir()
    }

    fn initialize(&self, port: u16) -> Result<()> {
        if self.is_initialized() {
            return Ok(());
        }
        if !self.is_installed() {
            bail!("MariaDB belum terpasang");
        }

        let data = self.paths.mariadb_data();
        std::fs::create_dir_all(&data)
            .with_context(|| format!("gagal membuat {}", data.display()))?;

        let output = crate::subprocess::run_hidden(
            &self.binary("mariadb-install-db.exe"),
            &[format!("--datadir={}", data.display())],
            Some(&self.paths.mariadb_bin()),
        )?;

        if !self.is_initialized() {
            let detail = String::from_utf8_lossy(&output.stderr);
            bail!("gagal menyiapkan datadir: {}", detail.trim());
        }

        // Sisa my.ini dari penginisialisasi tidak dipakai; DevKu menulis sendiri.
        let _ = std::fs::remove_file(data.join("my.ini"));
        self.write_config(port)?;
        Ok(())
    }

    fn spec(&self, port: u16) -> Result<ServiceSpec> {
        self.spec_with(port, true)
    }

    fn recovery_spec(&self, port: u16) -> Result<ServiceSpec> {
        self.spec_with(port, false)
    }

    fn password(&self) -> String {
        std::fs::read_to_string(self.paths.mariadb_password())
            .map(|text| text.trim_end_matches(['\r', '\n']).to_string())
            .unwrap_or_default()
    }

    fn store_password(&self, password: &str) -> Result<()> {
        crate::paths::write_atomic(&self.paths.mariadb_password(), password)
    }

    fn query(&self, port: u16, password: &str, sql: &str) -> Result<String> {
        let output = crate::subprocess::run_hidden(
            &self.binary("mariadb.exe"),
            &[
                "--host=127.0.0.1".to_string(),
                format!("--port={port}"),
                "--user=root".to_string(),
                format!("--password={password}"),
                "--connect-timeout=5".to_string(),
                "--batch".to_string(),
                "--skip-column-names".to_string(),
                format!("--execute={sql}"),
            ],
            None,
        )?;

        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            bail!("{}", Self::clean_error(&detail));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn root_hosts(&self, port: u16, password: &str) -> Result<Vec<String>> {
        let text = self.query(
            port,
            password,
            "SELECT host FROM mysql.user WHERE user='root'",
        )?;
        let hosts: Vec<String> = text
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .filter(|line| Self::usable_root_host(line))
            .collect();
        if hosts.is_empty() {
            bail!("tidak ada akun root yang bisa dipakai");
        }
        Ok(hosts)
    }

    fn set_password(&self, port: u16, current: &str, target: &str) -> Result<()> {
        let hosts = self.root_hosts(port, current)?;
        let value = Self::sql_literal(target);

        let mut sql = String::from("FLUSH PRIVILEGES; ");
        for host in hosts {
            sql.push_str(&format!(
                "ALTER USER 'root'@'{}' IDENTIFIED BY '{}'; ",
                Self::sql_literal(&host),
                value
            ));
        }

        self.query(port, current, &sql)?;
        Ok(())
    }

    fn user_databases(&self, port: u16, password: &str) -> Result<Vec<String>> {
        let filter = SYSTEM_SCHEMAS
            .iter()
            .map(|name| format!("'{name}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT schema_name FROM information_schema.schemata WHERE schema_name NOT IN ({filter}) ORDER BY schema_name"
        );
        let text = self.query(port, password, &sql)?;
        Ok(text
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect())
    }

    fn dump(&self, port: u16, password: &str, dest: &Path) -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let databases = self.user_databases(port, password)?;
        if databases.is_empty() {
            std::fs::write(
                dest,
                "-- DevKu: belum ada database buatan user saat backup dibuat.\n",
            )?;
            return Ok(());
        }

        let mut args = vec![
            "--host=127.0.0.1".to_string(),
            format!("--port={port}"),
            "--user=root".to_string(),
            format!("--password={password}"),
            "--default-character-set=utf8mb4".to_string(),
            "--single-transaction".to_string(),
            "--routines".to_string(),
            "--events".to_string(),
            "--databases".to_string(),
        ];
        args.extend(databases);
        args.push(format!("--result-file={}", dest.display()));

        let output = crate::subprocess::run_hidden(&self.binary("mariadb-dump.exe"), &args, None)?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            bail!("backup gagal: {}", Self::clean_error(&detail));
        }
        Ok(())
    }

    fn wipe(&self) -> Result<()> {
        let dir = self.paths.mariadb_data();
        if !dir.exists() {
            return Ok(());
        }

        let mut last: Option<std::io::Error> = None;
        for attempt in 0..5 {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    last = Some(err);
                    if attempt < 4 {
                        std::thread::sleep(std::time::Duration::from_millis(300));
                    }
                }
            }
        }

        let err = last.expect("setiap percobaan mengisi galat terakhir");
        Err(anyhow::Error::from(err)).with_context(|| format!("gagal menghapus {}", dir.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_follow_the_archive_layout() {
        assert_eq!(
            archive_url("11.4.13"),
            "https://archive.mariadb.org/mariadb-11.4.13/winx64-packages/mariadb-11.4.13-winx64.zip"
        );
        assert_eq!(
            checksums_url("11.4.13"),
            "https://archive.mariadb.org/mariadb-11.4.13/winx64-packages/sha256sums.txt"
        );
    }

    #[test]
    fn sql_literal_doubles_quotes() {
        assert_eq!(Mariadb::sql_literal("rahasia"), "rahasia");
        assert_eq!(Mariadb::sql_literal("o'brien"), "o''brien");
    }

    #[test]
    fn hostname_accounts_are_skipped_under_skip_name_resolve() {
        assert!(Mariadb::usable_root_host("localhost"));
        assert!(Mariadb::usable_root_host("127.0.0.1"));
        assert!(Mariadb::usable_root_host("::1"));
        assert!(Mariadb::usable_root_host("%"));
        assert!(!Mariadb::usable_root_host("desktop-rqlkubv"));
    }

    #[test]
    fn passwordless_warning_is_not_reported_as_error() {
        let raw = "WARNING: option --ssl-verify-server-cert is disabled\nERROR 1045: Access denied";
        let cleaned = Mariadb::clean_error(raw);
        assert_eq!(cleaned, "ERROR 1045: Access denied");
    }

    #[test]
    fn system_schemas_are_excluded_from_backups() {
        for name in ["mysql", "performance_schema", "information_schema", "sys"] {
            assert!(SYSTEM_SCHEMAS.contains(&name));
        }
    }
}
