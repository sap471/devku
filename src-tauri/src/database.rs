use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::paths::Paths;
use crate::proc::ServiceSpec;

/// The pinned version. 11.4 is the LTS with the longest support window and
/// the widest compatibility for PHP tooling.
pub const VERSION: &str = "11.4.13";

const MIRROR: &str = "https://archive.mariadb.org";

/// System schemas left out of backups: restoring them into a new install
/// would only create privilege conflicts.
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

/// Engine names as they appear in `config.json`. `Mysql` shares the MariaDB
/// driver but is a separate choice: the two cannot run together.
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseEngine {
    #[default]
    Mariadb,
    Mysql,
    Postgres,
}

impl DatabaseEngine {
    /// The folder name under `engines/database` and `databases`.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Mariadb => "mariadb",
            Self::Mysql => "mysql",
            Self::Postgres => "postgres",
        }
    }

    /// The port `.env` files and client tools expect for this engine. 3306 is
    /// what Laravel expects from the MySQL family; PostgreSQL's compiled-in
    /// default is 5432.
    pub fn default_port(&self) -> u16 {
        match self {
            Self::Mariadb | Self::Mysql => 3306,
            Self::Postgres => 5432,
        }
    }
}

/// Everything needed to reach a running server. The user is part of it because
/// PostgreSQL roles are not `user@host` accounts.
pub struct Conn {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub secret: String,
}

/// Local database server contract, shared by every engine. Engine-specific
/// behaviour (MariaDB's root accounts, PostgreSQL's roles) lives on the
/// implementation instead of here, so a second engine does not have to fake
/// a model it does not have.
pub trait DatabaseBackend: Send + Sync {
    fn engine(&self) -> DatabaseEngine;
    fn label(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn bin_dir(&self) -> PathBuf;
    fn data_dir(&self) -> PathBuf;
    fn is_installed(&self) -> bool;
    fn is_initialized(&self) -> bool;
    /// Prepares an empty datadir. Called once after the archive is extracted.
    fn initialize(&self, port: u16) -> Result<()>;
    /// False when the engine is a plain file and spawns nothing.
    fn has_server(&self) -> bool;
    /// The process to supervise, or None for a serverless engine.
    fn spec(&self, port: u16) -> Result<Option<ServiceSpec>>;
    fn query(&self, conn: &Conn, sql: &str) -> Result<String>;
    fn user_databases(&self, conn: &Conn) -> Result<Vec<String>>;
    fn system_schemas(&self) -> &'static [&'static str];
    fn dump(&self, conn: &Conn, dest: &Path) -> Result<()>;
    fn restore(&self, conn: &Conn, src: &Path) -> Result<()>;
    /// Empties the datadir.
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

    /// The root account that actually works. The server runs with
    /// `skip-name-resolve`, so rows hosted by the machine name are ignored and
    /// `ALTER USER` on them fails with ERROR 1396.
    fn usable_root_host(host: &str) -> bool {
        host == "localhost" || host == "%" || host.parse::<std::net::IpAddr>().is_ok()
    }

    /// The client error message carries a passwordless warning that is not a failure.
    fn clean_error(stderr: &str) -> String {
        stderr
            .lines()
            .filter(|line| !line.trim_start().starts_with("WARNING:"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    /// Bind addresses for `my.ini`. IPv6 loopback is included only when the
    /// IPv6 stack is usable: registering it on a machine without IPv6 makes
    /// mysqld fail to bind and the database never starts.
    fn bind_addresses() -> &'static str {
        if std::net::TcpListener::bind(("::1", 0)).is_ok() {
            "127.0.0.1,::1"
        } else {
            "127.0.0.1"
        }
    }

    /// The config file owned by DevKu. The `my.ini` left by the initializer
    /// holds absolute paths to that moment's folder, so it is not used.
    fn write_config(&self, port: u16) -> Result<PathBuf> {
        let file = self.paths.mariadb_config();
        let base = self.base_dir();
        // Two loopback addresses, not just IPv4: on Windows `localhost`
        // is answered by AAAA first, so clients try ::1 first. Without a
        // listener on ::1 that attempt hangs for about 2 seconds before
        // falling back to 127.0.0.1, which makes WordPress with DB_HOST=localhost slow.
        // Note: an account granted only for 127.0.0.1 does not automatically
        // match from ::1; grant ::1 or '%' like the DevKu root account.
        let text = format!(
            "[mysqld]\n\
             basedir={base}\n\
             datadir={data}\n\
             port={port}\n\
             bind-address={bind}\n\
             skip-name-resolve\n\
             plugin-dir={base}\\lib\\plugin\n\
             character-sets-dir={base}\\share\\charsets\n",
            base = base.display(),
            data = self.paths.mariadb_data().display(),
            bind = Self::bind_addresses(),
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
    fn engine(&self) -> DatabaseEngine {
        DatabaseEngine::Mariadb
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
        // server.dll must sit next to the mysqld.exe stub.
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

        // The initializer's leftover my.ini is unused; DevKu writes its own.
        let _ = std::fs::remove_file(data.join("my.ini"));
        self.write_config(port)?;
        Ok(())
    }

    fn has_server(&self) -> bool {
        true
    }

    fn spec(&self, port: u16) -> Result<Option<ServiceSpec>> {
        Ok(Some(self.spec_with(port, true)?))
    }

    fn query(&self, conn: &Conn, sql: &str) -> Result<String> {
        self.run_query(&conn.host, conn.port, &conn.user, &conn.secret, sql)
    }

    fn user_databases(&self, conn: &Conn) -> Result<Vec<String>> {
        let filter = SYSTEM_SCHEMAS
            .iter()
            .map(|name| format!("'{name}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT schema_name FROM information_schema.schemata WHERE schema_name NOT IN ({filter}) ORDER BY schema_name"
        );
        let text = self.query(conn, &sql)?;
        Ok(text
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect())
    }

    fn system_schemas(&self) -> &'static [&'static str] {
        SYSTEM_SCHEMAS
    }

    fn dump(&self, conn: &Conn, dest: &Path) -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let databases = self.user_databases(conn)?;
        if databases.is_empty() {
            std::fs::write(
                dest,
                "-- DevKu: belum ada database buatan user saat backup dibuat.\n",
            )?;
            return Ok(());
        }

        let mut args = vec![
            format!("--host={}", conn.host),
            format!("--port={}", conn.port),
            format!("--user={}", conn.user),
            format!("--password={}", conn.secret),
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

    fn restore(&self, conn: &Conn, src: &Path) -> Result<()> {
        if !src.exists() {
            bail!("berkas backup tidak ditemukan: {}", src.display());
        }

        let file =
            std::fs::File::open(src).with_context(|| format!("gagal membuka {}", src.display()))?;
        let output = crate::subprocess::run_hidden_with_stdin(
            &self.binary("mariadb.exe"),
            &[
                format!("--host={}", conn.host),
                format!("--port={}", conn.port),
                format!("--user={}", conn.user),
                format!("--password={}", conn.secret),
                // The client defaults to latin1, which would corrupt the
                // multibyte data the dump was written with.
                "--default-character-set=utf8mb4".to_string(),
            ],
            Some(&self.paths.mariadb_bin()),
            file,
        )?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            bail!("restore gagal: {}", Self::clean_error(&detail));
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

/// The MySQL account model: root rows in `mysql.user`, one per host. These
/// stay off the shared trait because PostgreSQL has roles, not `user@host`.
impl Mariadb {
    /// Service with the grant table disabled, to recover a password that is
    /// no longer known.
    pub fn recovery_spec(&self, port: u16) -> Result<ServiceSpec> {
        self.spec_with(port, false)
    }

    pub fn password(&self) -> String {
        std::fs::read_to_string(self.paths.mariadb_password())
            .map(|text| text.trim_end_matches(['\r', '\n']).to_string())
            .unwrap_or_default()
    }

    pub fn store_password(&self, password: &str) -> Result<()> {
        crate::paths::write_atomic(&self.paths.mariadb_password(), password)
    }

    fn run_query(
        &self,
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        sql: &str,
    ) -> Result<String> {
        let output = crate::subprocess::run_hidden(
            &self.binary("mariadb.exe"),
            &[
                format!("--host={host}"),
                format!("--port={port}"),
                format!("--user={user}"),
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

    pub fn root_hosts(&self, port: u16, password: &str) -> Result<Vec<String>> {
        let text = self.run_query(
            "127.0.0.1",
            port,
            "root",
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

    /// Changes the password on every root account at once. Changing a single row
    /// does not affect TCP login, because `127.0.0.1` matches its own account row.
    pub fn set_password(&self, port: u16, current: &str, target: &str) -> Result<()> {
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

        self.run_query("127.0.0.1", port, "root", current, &sql)?;
        Ok(())
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
    fn config_binds_both_loopback_addresses() {
        let root = std::env::temp_dir().join(format!("devku-myini-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let backend = Mariadb::new(Paths { root: root.clone() });
        let file = backend.write_config(3306).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();

        // `localhost` tries ::1 first; the IPv6 listener prevents the roughly
        // 2 second delay before the client falls back to 127.0.0.1.
        assert!(text.contains(&format!("bind-address={}", Mariadb::bind_addresses())));
        assert!(text.contains("port=3306"));
        assert!(text.contains("skip-name-resolve"));

        let _ = std::fs::remove_dir_all(&root);
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

    #[test]
    fn mariadb_reports_engine_and_has_a_server() {
        let paths = Paths {
            root: std::env::temp_dir().join("devku-plan-t1"),
        };
        let backend = Mariadb::new(paths);
        assert_eq!(backend.engine(), DatabaseEngine::Mariadb);
        assert!(backend.has_server());
        assert!(backend.system_schemas().contains(&"mysql"));
    }
}
