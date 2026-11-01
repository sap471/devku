//! Backend Apache internal. Caddy tetap menjadi satu-satunya pintu masuk di
//! port 80/443 dan meneruskan situs ber-backend Apache ke httpd di loopback.
//! Jadi semua akses, termasuk HTTPS, tetap lewat domain tanpa port; port
//! Apache tidak pernah dibuka ke luar. PHP tetap dijalankan pool php-cgi yang
//! sama, jadi satu versi PHP tidak butuh proses tambahan per backend.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::{Config, Site};
use crate::paths::{Paths, write_atomic};
use crate::proc::ServiceSpec;

/// Modul yang wajib ada, pasangan nama modul dan berkasnya. `mpm_winnt` tidak
/// ada di daftar karena Apache Windows mengompilasinya langsung ke httpd.exe.
const MODULES: &[(&str, &str)] = &[
    ("authz_core_module", "mod_authz_core.so"),
    ("authz_host_module", "mod_authz_host.so"),
    ("authn_core_module", "mod_authn_core.so"),
    ("access_compat_module", "mod_access_compat.so"),
    ("dir_module", "mod_dir.so"),
    ("mime_module", "mod_mime.so"),
    ("log_config_module", "mod_log_config.so"),
    ("rewrite_module", "mod_rewrite.so"),
    ("proxy_module", "mod_proxy.so"),
    ("proxy_fcgi_module", "mod_proxy_fcgi.so"),
];

pub const BINARY: &str = "httpd.exe";

/// Satu instalasi Apache di `engines/apache/<versi>`.
pub struct ApacheBackend {
    paths: Paths,
    root: Option<PathBuf>,
    port: u16,
}

impl ApacheBackend {
    /// Memakai instalasi terbaru yang ada di `engines/apache`. Tidak ada folder
    /// pilihan user: pemasangan dan pemilihan versi ditangani DevKu sendiri.
    pub fn new(paths: Paths, port: u16) -> Self {
        let root = installed_roots(&paths).pop().map(|(_, root)| root);
        Self { paths, root, port }
    }

    pub fn from_config(paths: &Paths, config: &Config) -> Self {
        Self::new(paths.clone(), config.apache.port)
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Nama folder versi di `engines/apache`, bukan nama folder di dalamnya
    /// (`Apache24` untuk zip Apache Lounge).
    pub fn version(&self) -> Option<String> {
        let root = self.root.as_ref()?;
        let relative = root.strip_prefix(self.paths.apache_dir()).ok()?;
        relative
            .components()
            .next()
            .map(|part| part.as_os_str().to_string_lossy().to_string())
    }

    pub fn binary(&self) -> Option<PathBuf> {
        self.root.as_ref().map(|root| root.join("bin").join(BINARY))
    }

    pub fn config_path(&self) -> PathBuf {
        self.paths.apache_config()
    }

    /// Siap dipakai: httpd.exe ada dan modul FastCGI lengkap.
    pub fn is_installed(&self) -> bool {
        self.check().is_ok()
    }

    fn check(&self) -> Result<()> {
        let Some(root) = &self.root else {
            bail!("Apache belum dipasang; pasang lewat Pengaturan > Apache");
        };
        let binary = root.join("bin").join(BINARY);
        if !binary.exists() {
            bail!("httpd.exe tidak ada di {}", binary.display());
        }
        let missing: Vec<&str> = MODULES
            .iter()
            .map(|(_, file)| *file)
            .filter(|name| !root.join("modules").join(name).exists())
            .collect();
        if !missing.is_empty() {
            bail!("modul Apache tidak lengkap: {}", missing.join(", "));
        }
        Ok(())
    }

    /// Site Apache beserta port pool PHP-nya.
    fn served<'a>(
        &self,
        config: &'a Config,
        php_ports: &BTreeMap<String, u16>,
    ) -> Result<Vec<(&'a Site, u16)>> {
        let mut out = Vec::new();
        for site in config.apache_sites() {
            let port = php_ports.get(&site.php).with_context(|| {
                format!(
                    "site {} butuh PHP {}, tapi tidak ada port teralokasi",
                    site.id, site.php
                )
            })?;
            if !site.path.exists() {
                bail!("site {}: folder {} tidak ada", site.id, site.path.display());
            }
            out.push((site, *port));
        }
        Ok(out)
    }

    /// httpd.conf lengkap: satu vhost per site ber-backend Apache.
    pub fn render(&self, config: &Config, php_ports: &BTreeMap<String, u16>) -> Result<String> {
        self.check()?;
        let root = self.root.as_ref().expect("check memastikan root ada");
        let root = slash(root);
        let runtime = slash(&self.paths.apache_runtime());
        let logs = slash(&self.paths.logs());
        let port = self.port;

        let mut out = String::new();
        out.push_str(&format!(
            "ServerRoot \"{root}\"\n\
             Listen 127.0.0.1:{port}\n\
             ServerName localhost\n\
             PidFile \"{runtime}/httpd.pid\"\n\
             DefaultRuntimeDir \"{runtime}\"\n\
             ErrorLog \"{logs}/apache-error.log\"\n\
             LogLevel warn\n\
             HostnameLookups Off\n\
             DirectoryIndex index.php index.html\n"
        ));

        // mime.types ikut distribusi standar Apache. Kalau tidak ada, lebih
        // baik tanpa pemetaan tipe daripada httpd menolak start.
        let mime = self
            .root
            .as_ref()
            .map(|root| root.join("conf/mime.types"))
            .filter(|path| path.exists());
        if let Some(mime) = mime {
            out.push_str(&format!("TypesConfig \"{}\"\n", slash(&mime)));
        }

        for (module, file) in MODULES {
            out.push_str(&format!("LoadModule {module} modules/{file}\n"));
        }

        out.push_str(
            "\n<Directory />\n\
             \tAllowOverride none\n\
             \tRequire all denied\n\
             </Directory>\n",
        );

        for (site, php_port) in self.served(config, php_ports)? {
            if site.domain.contains(['"', '\'', ' ', '\t']) {
                bail!("domain {} mengandung karakter yang tidak sah", site.domain);
            }
            let docroot = slash(&site.path);
            if docroot.contains('"') {
                bail!("path {} mengandung tanda kutip", site.path.display());
            }
            out.push_str(&format!(
                "\n<VirtualHost 127.0.0.1:{port}>\n\
                 \tServerName {domain}\n\
                 \tDocumentRoot \"{docroot}\"\n\
                 \t<Directory \"{docroot}\">\n\
                 \t\tOptions FollowSymLinks\n\
                 \t\tAllowOverride All\n\
                 \t\tRequire all granted\n\
                 \t</Directory>\n\
                 \t<FilesMatch \"\\.php$\">\n\
                 \t\tSetHandler \"proxy:fcgi://127.0.0.1:{php_port}/\"\n\
                 \t</FilesMatch>\n\
                 \tProxyFCGISetEnvIf true SCRIPT_FILENAME %{{DOCUMENT_ROOT}}%{{REQUEST_URI}}\n\
                 \tErrorLog \"{logs}/apache-{id}-error.log\"\n\
                 \tCustomLog \"{logs}/apache-{id}-access.log\" combined\n\
                 </VirtualHost>\n",
                domain = site.domain,
                id = site.id,
            ));
        }

        Ok(out)
    }

    /// Tulis config lalu kembalikan isinya, supaya pemanggil bisa membandingkan
    /// dengan config yang sedang dipakai Apache dan menghindari restart.
    pub fn write_config(
        &self,
        config: &Config,
        php_ports: &BTreeMap<String, u16>,
    ) -> Result<String> {
        let text = self.render(config, php_ports)?;
        // Apache menolak start kalau folder log belum ada, dan folder runtime
        // bisa kosong pada pemakaian pertama.
        std::fs::create_dir_all(self.paths.logs())?;
        write_atomic(&self.config_path(), &text)?;
        Ok(text)
    }

    pub fn spec(&self) -> Result<ServiceSpec> {
        self.check()?;
        let binary = self.binary().expect("check memastikan binary ada");
        let root = self.root.as_ref().expect("check memastikan root ada");
        Ok(ServiceSpec {
            name: "apache".to_string(),
            program: binary,
            args: vec![
                "-f".to_string(),
                self.config_path().to_string_lossy().to_string(),
            ],
            env: Vec::new(),
            // cwd di bin supaya DLL Apache (libapr, libhttpd) ditemukan tanpa
            // mengubah PATH milik proses DevKu.
            cwd: Some(root.join("bin")),
            port: Some(self.port),
            log_dir: self.paths.logs(),
        })
    }
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// True kalau folder ini (atau satu subfolder pembungkusnya) berisi
/// `bin/httpd.exe`. Zip Apache Lounge menaruh semuanya di `Apache24/`.
pub fn has_httpd(dir: &Path) -> bool {
    discover_root(dir).is_some()
}

fn discover_root(dir: &Path) -> Option<PathBuf> {
    if dir.join("bin").join(BINARY).is_file() {
        return Some(dir.to_path_buf());
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("bin").join(BINARY).is_file() {
            return Some(path);
        }
    }
    None
}

/// Instalasi yang bisa dipakai, urut naik seperti PHP: yang terakhir selalu
/// terbaru. Nama folder dipakai sebagai versi karena itu yang ditampilkan UI.
pub fn installed_roots(paths: &Paths) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(paths.apache_dir()) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let Some(root) = discover_root(&entry.path()) else {
                continue;
            };
            let Some(name) = entry.file_name().to_str().map(|name| name.to_string()) else {
                continue;
            };
            out.push((name, root));
        }
    }
    out.sort_by_key(|(version, _)| crate::php::version_key(version));
    out
}

/// Versi terpasang, urut naik.
pub fn installed(paths: &Paths) -> Vec<String> {
    installed_roots(paths)
        .into_iter()
        .map(|(version, _)| version)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ApacheSettings, Backend, PhpSettings, Site};
    use std::path::PathBuf;

    fn apache_root(dir: &Path) -> PathBuf {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::create_dir_all(dir.join("modules")).unwrap();
        std::fs::create_dir_all(dir.join("conf")).unwrap();
        std::fs::write(dir.join("bin/httpd.exe"), b"x").unwrap();
        std::fs::write(dir.join("conf/mime.types"), b"").unwrap();
        for (_, file) in MODULES {
            std::fs::write(dir.join("modules").join(file), b"x").unwrap();
        }
        dir.to_path_buf()
    }

    fn temp_paths(tag: &str) -> Paths {
        let root = std::env::temp_dir().join(format!("devku-apache-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Paths { root }
    }

    fn site(id: &str, backend: Backend) -> Site {
        Site {
            id: id.to_string(),
            name: id.to_string(),
            path: std::env::temp_dir(),
            domain: format!("{id}.localhost"),
            php: "8.3.36".to_string(),
            root: None,
            framework: None,
            backend: Some(backend),
            enabled: true,
        }
    }

    fn sample_config() -> Config {
        Config {
            apache: ApacheSettings { port: 8080 },
            php: PhpSettings {
                active: String::new(),
                children: 4,
            },
            sites: vec![
                site("apache-app", Backend::Apache),
                site("caddy-app", Backend::Caddy),
            ],
            ..Config::default()
        }
    }

    fn ports() -> BTreeMap<String, u16> {
        BTreeMap::from([("8.3.36".to_string(), 9002)])
    }

    fn backend(root: &Path) -> ApacheBackend {
        ApacheBackend {
            paths: temp_paths("runtime"),
            root: Some(root.to_path_buf()),
            port: 8080,
        }
    }

    #[test]
    fn render_has_one_vhost_per_apache_site() {
        let root = apache_root(
            &std::env::temp_dir().join(format!("devku-apache-vhost-{}", std::process::id())),
        );
        let backend = backend(&root);
        let text = backend.render(&sample_config(), &ports()).unwrap();

        assert!(text.contains("Listen 127.0.0.1:8080"));
        assert!(text.contains("<VirtualHost 127.0.0.1:8080>"));
        assert_eq!(text.matches("<VirtualHost").count(), 1);
        assert!(text.contains("ServerName apache-app.localhost"));
        assert!(!text.contains("caddy-app.localhost"));
        assert!(text.contains("SetHandler \"proxy:fcgi://127.0.0.1:9002/\""));
        assert!(text.contains("ProxyFCGISetEnvIf true SCRIPT_FILENAME"));
        assert!(text.contains("LoadModule proxy_fcgi_module modules/mod_proxy_fcgi.so"));
    }

    #[test]
    fn render_fails_without_installation() {
        let backend = ApacheBackend {
            paths: temp_paths("empty"),
            root: None,
            port: 8080,
        };
        assert!(!backend.is_installed());
        assert!(backend.render(&Config::default(), &ports()).is_err());
    }

    #[test]
    fn render_needs_a_php_port_for_each_site() {
        let root = apache_root(
            &std::env::temp_dir().join(format!("devku-apache-ports-{}", std::process::id())),
        );
        let error = backend(&root)
            .render(&sample_config(), &BTreeMap::new())
            .unwrap_err()
            .to_string();
        assert!(error.contains("tidak ada port teralokasi"));
    }

    #[test]
    fn installed_uses_newest_folder() {
        let paths = temp_paths("installed");
        let base = paths.apache_dir();
        apache_root(&base.join("2.4.62-240904"));
        // Zip Apache Lounge membungkus isinya di Apache24/.
        apache_root(&base.join("2.4.69-261002").join("Apache24"));

        assert_eq!(installed(&paths), vec!["2.4.62-240904", "2.4.69-261002"]);
        let backend = ApacheBackend::new(paths, 8080);
        assert!(backend.is_installed());
        assert_eq!(backend.version().as_deref(), Some("2.4.69-261002"));
        assert!(backend.root().unwrap().ends_with("Apache24"));
    }

    /// Membuktikan config hasil render benar-benar diterima httpd dan PHP
    /// dieksekusi lewat pool yang sudah berjalan. Tidak dijalankan default
    /// karena butuh Apache lokal dan pool php-cgi.
    ///
    /// `DEVKU_APACHE_ROOT=... DEVKU_PHP_PORT=9011 cargo test e2e_rendered -- --ignored`
    #[test]
    #[ignore = "butuh Apache lokal dan pool php-cgi yang sudah berjalan"]
    fn e2e_rendered_config_serves_php() {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        let Ok(root) = std::env::var("DEVKU_APACHE_ROOT") else {
            panic!("set DEVKU_APACHE_ROOT ke folder instalasi Apache");
        };
        let php_port: u16 = std::env::var("DEVKU_PHP_PORT")
            .expect("set DEVKU_PHP_PORT ke port pool php-cgi")
            .parse()
            .expect("DEVKU_PHP_PORT harus angka");

        let paths = temp_paths("e2e");
        let docroot = paths.root.join("htdocs");
        std::fs::create_dir_all(&docroot).unwrap();
        std::fs::write(
            docroot.join("index.php"),
            "<?php echo 'e2e-ok ', PHP_VERSION;",
        )
        .unwrap();
        // Permalink ala WordPress: path yang bukan berkas diarahkan ke
        // index.php. SCRIPT_FILENAME harus tetap menunjuk berkas hasil rewrite.
        std::fs::write(
            docroot.join(".htaccess"),
            "RewriteEngine On\nRewriteCond %{REQUEST_FILENAME} !-f\nRewriteRule ^ index.php [L]\n",
        )
        .unwrap();

        let mut config = sample_config();
        config.sites[0].path = docroot;
        let backend = ApacheBackend {
            paths: paths.clone(),
            root: Some(PathBuf::from(&root)),
            port: 8080,
        };
        backend
            .write_config(&config, &BTreeMap::from([("8.3.36".to_string(), php_port)]))
            .unwrap();

        let spec = backend.spec().unwrap();
        let mut httpd = Command::new(&spec.program)
            .args(&spec.args)
            .current_dir(spec.cwd.unwrap())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(10);
        while TcpStream::connect(("127.0.0.1", 8080)).is_err() {
            assert!(Instant::now() < deadline, "Apache tidak siap di port 8080");
            std::thread::sleep(Duration::from_millis(100));
        }

        let get = |path: &str| -> String {
            let mut stream = TcpStream::connect(("127.0.0.1", 8080)).unwrap();
            write!(
                stream,
                "GET {path} HTTP/1.1\r\nHost: apache-app.localhost:8080\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        };

        let direct = get("/index.php");
        let rewritten = get("/halo");

        let _ = httpd.kill();
        let _ = httpd.wait();

        assert!(direct.contains("200 OK"), "respons: {direct}");
        assert!(direct.contains("e2e-ok"), "respons: {direct}");
        assert!(rewritten.contains("200 OK"), "respons: {rewritten}");
        assert!(rewritten.contains("e2e-ok"), "respons: {rewritten}");
    }
}
