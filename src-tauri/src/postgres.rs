use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::database::{Conn, DatabaseBackend, DatabaseEngine};
use crate::paths::Paths;
use crate::proc::ServiceSpec;

/// Generates a random password for the postgres superuser. Uses `SystemTime`
/// and the current PID as seed material, hashed with SHA-256 and encoded as
/// a 32-hex string. This avoids adding a `rand` dependency while still
/// producing a password that is unique per machine and per install.
fn generate_password() -> String {
    use sha2::Digest;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    let mut h = sha2::Sha256::new();
    h.update(now.to_le_bytes());
    h.update(pid.to_le_bytes());
    h.finalize()
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The pinned version. EDB publishes Windows x64 binaries for every supported
/// release; 18 is the current stable line at plan time.
pub const POSTGRES_VERSION: &str = "18.6";

/// EDB publishes no checksum for its binaries zip, so DevKu verifies the
/// archive with the recorded-digest path (`download.rs`), the same way Bun is
/// handled. The URL is the plain layout that works for modern versions.
pub fn archive_url(version: &str) -> String {
    format!(
        "https://get.enterprisedb.com/postgresql/postgresql-{version}-1-windows-x64-binaries.zip"
    )
}

/// Databases every PostgreSQL server ships. They must never be backed up.
const SYSTEM_SCHEMAS: &[&str] = &["template0", "template1", "postgres"];

pub struct Postgres {
    paths: Paths,
}

impl Postgres {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    fn binary(&self, name: &str) -> PathBuf {
        self.paths.database_bin(DatabaseEngine::Postgres).join(name)
    }

    fn data_dir(&self) -> PathBuf {
        self.paths.database_data(DatabaseEngine::Postgres)
    }

    /// The config file `initdb` creates is deliberately not copied from a
    /// template; DevKu writes its own so the port and listen address are
    /// exactly what the manager expects.
    fn config_file(&self) -> PathBuf {
        self.paths
            .database_config(DatabaseEngine::Postgres, "postgresql.conf")
    }

    fn hba_file(&self) -> PathBuf {
        self.paths
            .database_config(DatabaseEngine::Postgres, "pg_hba.conf")
    }

    /// Every element is exactly one argv token. `run_hidden` passes each
    /// element as a single quoted argument, so short flags like `-A
    /// scram-sha-256` written as one string arrive with a leading space in
    /// the option value; the `--long=value` form is the safe spelling here,
    /// the same way the MariaDB backend passes `--host=`.
    fn initdb_args(data: &Path, pwfile: &Path) -> Vec<String> {
        vec![
            format!("--pgdata={}", data.display()),
            "--username=postgres".to_string(),
            "--auth=scram-sha-256".to_string(),
            "--encoding=UTF8".to_string(),
            format!("--pwfile={}", pwfile.display()),
        ]
    }
}

impl DatabaseBackend for Postgres {
    fn engine(&self) -> DatabaseEngine {
        DatabaseEngine::Postgres
    }

    fn label(&self) -> &'static str {
        "PostgreSQL"
    }

    fn version(&self) -> &'static str {
        POSTGRES_VERSION
    }

    fn bin_dir(&self) -> PathBuf {
        self.paths.database_bin(DatabaseEngine::Postgres)
    }

    fn data_dir(&self) -> PathBuf {
        self.data_dir()
    }

    fn is_installed(&self) -> bool {
        self.binary("postgres.exe").exists() && self.binary("initdb.exe").exists()
    }

    fn is_initialized(&self) -> bool {
        self.data_dir().join("PG_VERSION").is_file()
    }

    fn initialize(&self, port: u16) -> Result<()> {
        if self.is_initialized() {
            return Ok(());
        }
        if !self.is_installed() {
            bail!("PostgreSQL belum terpasang");
        }

        let data = self.data_dir();
        std::fs::create_dir_all(&data)
            .with_context(|| format!("gagal membuat {}", data.display()))?;

        // `initdb` reads the superuser password from the file, so a password
        // must exist before the process starts. If the file is absent or empty
        // (e.g. the first install), generate a random one now. The same value
        // is read back by `Conn` via `database_secret`, so the generated
        // password survives restarts without further intervention.
        let password_file = self.paths.database_secret(DatabaseEngine::Postgres);
        if !password_file.exists()
            || std::fs::read(&password_file)
                .map(|b| b.is_empty())
                .unwrap_or(true)
        {
            crate::paths::write_atomic(&password_file, &generate_password())?;
        }

        // The superuser authenticates with a password, never with `trust`,
        // mirroring MariaDB's password model. The password is read from the
        // password file so it is not part of any process list.
        let output = crate::subprocess::run_hidden(
            &self.binary("initdb.exe"),
            &Self::initdb_args(&data, &password_file),
            Some(&self.bin_dir()),
        )?;

        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            bail!("gagal menyiapkan datadir: {}", detail.trim());
        }

        self.write_config(port)?;
        Ok(())
    }

    fn has_server(&self) -> bool {
        true
    }

    fn spec(&self, port: u16) -> Result<Option<ServiceSpec>> {
        if !self.is_installed() {
            bail!("PostgreSQL belum terpasang");
        }
        if !self.is_initialized() {
            bail!("database belum disiapkan");
        }

        Ok(Some(ServiceSpec {
            name: "postgres".to_string(),
            program: self.binary("postgres.exe"),
            args: vec![
                format!("-D {}", self.data_dir().display()),
                format!("-p {port}"),
            ],
            env: Vec::new(),
            cwd: Some(self.bin_dir()),
            port: Some(port),
            log_dir: self.paths.logs(),
        }))
    }

    fn query(&self, conn: &Conn, sql: &str) -> Result<String> {
        let output = crate::subprocess::run_hidden_env(
            &self.binary("psql.exe"),
            &[
                format!("--host={}", conn.host),
                format!("--port={}", conn.port),
                format!("--username={}", conn.user),
                "--dbname=postgres".to_string(),
                "--tuples-only".to_string(),
                "--no-align".to_string(),
                format!("--command={sql}"),
            ],
            Some(&self.bin_dir()),
            &[("PGPASSWORD".to_string(), conn.secret.clone())],
        )?;

        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            bail!("{}", detail.trim());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn user_databases(&self, conn: &Conn) -> Result<Vec<String>> {
        let sql = "SELECT datname FROM pg_database WHERE datistemplate = false AND datname <> 'postgres' ORDER BY datname";
        let text = self.query(conn, sql)?;
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

        let output = crate::subprocess::run_hidden_env(
            &self.binary("pg_dump.exe"),
            &[
                format!("--host={}", conn.host),
                format!("--port={}", conn.port),
                format!("--username={}", conn.user),
                "--format=plain".to_string(),
                format!("--file={}", dest.display()),
                "postgres".to_string(),
            ],
            Some(&self.bin_dir()),
            &[("PGPASSWORD".to_string(), conn.secret.clone())],
        )?;

        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            bail!("backup gagal: {}", detail.trim());
        }
        Ok(())
    }

    fn restore(&self, conn: &Conn, src: &Path) -> Result<()> {
        if !src.exists() {
            bail!("berkas backup tidak ditemukan: {}", src.display());
        }

        let output = crate::subprocess::run_hidden_env(
            &self.binary("psql.exe"),
            &[
                format!("--host={}", conn.host),
                format!("--port={}", conn.port),
                format!("--username={}", conn.user),
                "--dbname=postgres".to_string(),
                format!("--file={}", src.display()),
            ],
            Some(&self.bin_dir()),
            &[("PGPASSWORD".to_string(), conn.secret.clone())],
        )?;

        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            bail!("restore gagal: {}", detail.trim());
        }
        Ok(())
    }

    fn wipe(&self) -> Result<()> {
        let dir = self.data_dir();
        if !dir.exists() {
            return Ok(());
        }
        std::fs::remove_dir_all(&dir).with_context(|| format!("gagal menghapus {}", dir.display()))
    }
}

impl Postgres {
    /// `initdb` generates a config and host-based auth file. DevKu writes its
    /// own so the server binds only loopback and only a port we control.
    fn write_config(&self, port: u16) -> Result<()> {
        let config = format!("port = {port}\nlisten_addresses = '127.0.0.1'\n");
        crate::paths::write_atomic(&self.config_file(), &config)?;

        // Loopback only, password auth. The user's `root.pass` is the secret
        // the superuser was initialized with.
        let hba = "local all all scram-sha-256\nhost all all 127.0.0.1/32 scram-sha-256\n";
        crate::paths::write_atomic(&self.hba_file(), hba)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_archive_url_pins_edb_pattern() {
        assert_eq!(
            archive_url("18.6"),
            "https://get.enterprisedb.com/postgresql/postgresql-18.6-1-windows-x64-binaries.zip"
        );
    }

    #[test]
    fn postgres_reports_engine_and_system_schemas() {
        let paths = Paths {
            root: std::env::temp_dir().join("devku-plan-t11"),
        };
        let backend = Postgres::new(paths);
        assert_eq!(backend.engine(), DatabaseEngine::Postgres);
        assert!(backend.has_server());
        assert!(backend.system_schemas().contains(&"template1"));
    }

    #[test]
    fn postgres_not_installed_on_empty_home() {
        let paths = Paths {
            root: std::env::temp_dir().join("devku-plan-t11b"),
        };
        let backend = Postgres::new(paths);
        assert!(!backend.is_installed());
        assert!(!backend.is_initialized());
    }

    #[test]
    fn postgres_bin_dir_is_engine_scoped() {
        let paths = Paths {
            root: std::env::temp_dir().join("devku-plan-t11c"),
        };
        let backend = Postgres::new(paths);
        assert!(backend.bin_dir().ends_with("postgres/bin"));
    }

    /// Runs the real installed `initdb` with exactly the flags production code
    /// sends, so a flag the binary rejects fails here and not on the user's
    /// first click of Pasang. Skipped when PostgreSQL is not installed, since
    /// routine tests never download engine binaries.
    #[test]
    fn initdb_accepts_devku_flags() {
        let installed = Paths {
            root: std::path::PathBuf::from(std::env::var("APPDATA").unwrap()).join("DevKu"),
        };
        let initdb = installed
            .database_bin(DatabaseEngine::Postgres)
            .join("initdb.exe");
        if !initdb.exists() {
            return;
        }

        let root = std::env::temp_dir().join(format!("devku-initdb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let pwfile = root.join("pw.txt");
        std::fs::write(&pwfile, "devku-test").unwrap();

        let bin_dir = initdb.parent().map(|p| p.to_path_buf());
        let output = crate::subprocess::run_hidden(
            &initdb,
            &Postgres::initdb_args(&data, &pwfile),
            Some(bin_dir.as_deref().unwrap_or(&data)),
        )
        .unwrap();

        assert!(
            output.status.success(),
            "initdb menolak argumen DevKu: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(data.join("PG_VERSION").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }
}
