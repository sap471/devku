use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;

use crate::api;
use crate::config::Config;
use crate::paths::{Paths, write_atomic};
use crate::proc::ServiceSpec;

/// Core tidak tahu backend mana yang dipakai. Semua yang dibutuhkan untuk
/// menjalankan sebuah situs lewat interface ini.
#[async_trait]
pub trait WebServerBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn render(&self, config: &Config, php_ports: &BTreeMap<String, u16>) -> Result<String>;
    fn spec(&self, config: &Config, php_ports: &BTreeMap<String, u16>) -> Result<ServiceSpec>;
    async fn reload(&self, config: &Config, php_ports: &BTreeMap<String, u16>) -> Result<u16>;
    async fn healthy(&self) -> bool;
}

pub struct CaddyBackend {
    paths: Paths,
}

impl CaddyBackend {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    pub fn binary(&self) -> PathBuf {
        self.paths.caddy_binary()
    }

    pub fn is_installed(&self) -> bool {
        self.binary().exists()
    }

    /// Dipakai Fase 2 saat unduhan selesai: taruh binary di `engines/caddy`.
    pub fn install_binary(&self, source: &Path) -> Result<PathBuf> {
        let target = self.binary();
        std::fs::create_dir_all(self.paths.caddy_dir())?;
        std::fs::copy(source, &target)
            .with_context(|| format!("gagal menyalin caddy ke {}", target.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&target)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&target, perms)?;
        }

        Ok(target)
    }

    /// Caddy memvalidasi sebelum benar-benar dijalankan. Dipakai saat belum ada
    /// instance hidup: lebih baik satu pesan galat jelas daripada proses yang
    /// gagal start lalu diulang supervisor lima kali.
    pub async fn validate(&self, caddyfile: &str) -> Result<()> {
        if !self.is_installed() {
            bail!(
                "Caddy belum terpasang di {}",
                self.paths.caddy_binary().display()
            );
        }

        std::fs::create_dir_all(self.paths.runtime())?;
        let candidate = self.paths.runtime().join("Caddyfile.candidate");
        std::fs::write(&candidate, caddyfile)?;

        let output = tokio::process::Command::new(self.binary())
            .arg("validate")
            .arg("--config")
            .arg(&candidate)
            .arg("--adapter")
            .arg("caddyfile")
            .output()
            .await
            .with_context(|| format!("gagal menjalankan {}", self.binary().display()))?;

        let _ = std::fs::remove_file(&candidate);

        if output.status.success() {
            return Ok(());
        }
        bail!("{}", extract_caddy_error(&output.stderr, &output.stdout))
    }

    /// Render, validasi, baru tulis. Urutan ini mencegah file di disk berisi
    /// config yang ditolak, yang akan menggagalkan start berikutnya juga.
    pub async fn write_config(
        &self,
        config: &Config,
        php_ports: &BTreeMap<String, u16>,
    ) -> Result<String> {
        let caddyfile = self.render(config, php_ports)?;
        self.validate(&caddyfile).await?;
        self.write_dashboard(config)?;
        write_atomic(&self.paths.caddyfile(), &caddyfile)?;
        Ok(caddyfile)
    }

    /// Halaman yang dilayani di port 80 untuk `localhost` dan host yang tidak
    /// dikenal. Ditulis ulang setiap config berubah, jadi isinya tidak basi.
    pub fn write_dashboard(&self, config: &Config) -> Result<()> {
        let dir = self.paths.dashboard_dir();
        std::fs::create_dir_all(&dir)?;
        write_atomic(&dir.join("index.html"), &render_dashboard(config))?;
        Ok(())
    }
}

#[async_trait]
impl WebServerBackend for CaddyBackend {
    fn name(&self) -> &'static str {
        "caddy"
    }

    fn render(&self, config: &Config, php_ports: &BTreeMap<String, u16>) -> Result<String> {
        let sites: Vec<_> = config.served_sites().collect();

        let mut out = String::from("{\n\tadmin localhost:2019\n\tauto_https off\n}\n");

        for site in sites {
            let port = php_ports.get(&site.php).with_context(|| {
                format!(
                    "site {} butuh PHP {}, tapi tidak ada port teralokasi",
                    site.id, site.php
                )
            })?;

            if !site.path.exists() {
                bail!("site {}: folder {} tidak ada", site.id, site.path.display());
            }

            let root = quote(&site.path);
            let log = quote(&self.paths.logs().join(format!("{}.log", site.id)));

            out.push_str(&format!(
                "\nhttp://{domain} {{\n\
                 \troot * {root}\n\
                 \tencode gzip\n\
                 \tphp_fastcgi 127.0.0.1:{port}\n\
                 \tfile_server\n\
                 \tlog {{\n\
                 \t\toutput file {log}\n\
                 \t\tformat console\n\
                 \t}}\n\
                 }}\n",
                domain = site.domain,
                root = root,
                port = port,
                log = log,
            ));
        }

        // Dasbor selalu ada, jadi port 80 tidak pernah kosong dan salah ketik
        // domain mendarat di halaman yang menjelaskan keadaannya.
        let dashboard = quote(&self.paths.dashboard_dir());
        out.push_str(&format!(
            "\nhttp://localhost, http://127.0.0.1 {{\n\
             \troot * {dashboard}\n\
             \tfile_server\n\
             }}\n\
             \nhttp:// {{\n\
             \troot * {dashboard}\n\
             \tfile_server\n\
             }}\n"
        ));

        Ok(out)
    }

    fn spec(&self, _config: &Config, _php_ports: &BTreeMap<String, u16>) -> Result<ServiceSpec> {
        if !self.is_installed() {
            bail!(
                "Caddy belum terpasang di {}; jalankan `devku-cli install-caddy <path>`",
                self.binary().display()
            );
        }

        Ok(ServiceSpec {
            name: "caddy".to_string(),
            program: self.binary(),
            args: vec![
                "run".to_string(),
                "--config".to_string(),
                self.paths.caddyfile().to_string_lossy().to_string(),
                "--adapter".to_string(),
                "caddyfile".to_string(),
            ],
            env: vec![
                (
                    "XDG_DATA_HOME".to_string(),
                    self.paths.runtime().to_string_lossy().to_string(),
                ),
                (
                    "XDG_CONFIG_HOME".to_string(),
                    self.paths.runtime().to_string_lossy().to_string(),
                ),
            ],
            cwd: Some(self.paths.runtime()),
            port: Some(api::ADMIN_PORT),
            log_dir: self.paths.logs(),
        })
    }

    async fn reload(&self, config: &Config, php_ports: &BTreeMap<String, u16>) -> Result<u16> {
        let caddyfile = self.render(config, php_ports)?;

        // Instance yang hidup sudah memvalidasi sendiri saat /load: kalau
        // ditolak, config lama tetap berjalan. Jadi tidak perlu `caddy validate`
        // terpisah di jalur ini, cukup jangan simpan file yang ditolak.
        let (status, body) = api::post_caddyfile(&caddyfile).await?;
        if status >= 400 {
            bail!("Caddy menolak config: {}", extract_load_error(&body));
        }

        write_atomic(&self.paths.caddyfile(), &caddyfile)?;
        self.write_dashboard(config)?;
        Ok(status)
    }

    async fn healthy(&self) -> bool {
        api::ping().await
    }
}

/// Caddy menulis log JSON ke stderr; ambil hanya baris level error supaya UI
/// tidak menampilkan deretan info startup.
fn extract_caddy_error(stderr: &[u8], stdout: &[u8]) -> String {
    let mut messages = Vec::new();

    for line in String::from_utf8_lossy(stderr).lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line)
            && value.get("level").and_then(|v| v.as_str()) == Some("error")
            && let Some(msg) = value.get("msg").and_then(|v| v.as_str())
        {
            messages.push(msg.to_string());
        }
    }

    if !messages.is_empty() {
        return messages.join("; ");
    }

    let out = String::from_utf8_lossy(stdout).trim().to_string();
    if !out.is_empty() {
        return out;
    }
    let err = String::from_utf8_lossy(stderr).trim().to_string();
    if err.is_empty() {
        "Caddy menolak config tanpa pesan".to_string()
    } else {
        err
    }
}

fn extract_load_error(body: &str) -> String {
    let trimmed = body.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed)
        && let Some(err) = value.get("error").and_then(|v| v.as_str())
    {
        return err.to_string();
    }
    if trimmed.is_empty() {
        "tanpa pesan".to_string()
    } else {
        trimmed.to_string()
    }
}

fn render_dashboard(config: &Config) -> String {
    let mut items = String::new();
    for site in config.served_sites() {
        items.push_str(&format!(
            r#"<li><a href="http://{domain}/">{name}</a><span>{domain} · php {php} · {fw}</span></li>"#,
            domain = html_escape(&site.domain),
            name = html_escape(&site.name),
            php = html_escape(&site.php),
            fw = html_escape(site.framework.as_deref().unwrap_or("php")),
        ));
    }

    let content = if items.is_empty() {
        "<p class=\"empty\">Belum ada situs. Tambahkan lewat aplikasi DevKu, lalu halaman ini memuat daftarnya.</p>".to_string()
    } else {
        format!("<ul class=\"sites\">{items}</ul>")
    };

    let style = r#"
    :root { color-scheme: light dark; }
    body { margin: 0; padding: 32px 24px; font: 14px/1.55 ui-sans-serif, system-ui, "Segoe UI", sans-serif;
           background: #fafaf9; color: #18181b; }
    header { display: flex; align-items: baseline; gap: 10px; margin-bottom: 24px; }
    header strong { font-size: 15px; letter-spacing: -0.01em; }
    header span { font-size: 12px; color: #52525b; }
    h1 { margin: 0 0 10px; font-size: 12px; font-weight: 650; letter-spacing: 0.07em;
         text-transform: uppercase; color: #52525b; }
    .sites { list-style: none; margin: 0; padding: 0; border-top: 1px solid #e4e4e7; }
    .sites li { display: flex; flex-wrap: wrap; gap: 4px 12px; align-items: baseline;
                padding: 9px 0; border-bottom: 1px solid #e4e4e7; }
    .sites a { color: #b45309; font-weight: 600; text-decoration: none; }
    .sites a:hover, .sites a:focus-visible { text-decoration: underline; }
    .sites span { font-family: ui-monospace, "Cascadia Code", Consolas, monospace;
                  font-size: 12px; color: #52525b; }
    .empty { color: #52525b; max-width: 48ch; }
    footer { margin-top: 28px; font-size: 12px; color: #52525b; }
    @media (prefers-color-scheme: dark) {
      body { background: #0c0d0f; color: #e7e7ea; }
      header span, h1, .sites span, .empty, footer { color: #a1a1aa; }
      .sites, .sites li { border-color: #26292e; }
      .sites a { color: #f59e0b; }
    }
    "#;

    format!(
        "<!doctype html>\n<html lang=\"id\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta http-equiv=\"refresh\" content=\"15\">\n<title>DevKu</title>\n\
         <style>{style}</style>\n</head>\n<body>\n\
         <header><strong>DevKu</strong><span>Dasbor</span></header>\n\
         <main>\n<h1>Situs</h1>\n{content}\n</main>\n\
         <footer>Halaman ini dibuat otomatis dan dimuat ulang tiap 15 detik.</footer>\n\
         </body>\n</html>\n"
    )
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Caddyfile memakai spasi sebagai pemisah, jadi path dengan spasi harus dikutip.
fn quote(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    format!("\"{text}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Backend, PhpSettings, Site};
    use std::collections::BTreeSet;

    fn sample_config() -> Config {
        Config {
            version: 1,
            backend: Backend::Caddy,
            tld: "localhost".to_string(),
            autostart: true,
            php: PhpSettings {
                installed: BTreeSet::new(),
                active: "8.3.36".to_string(),
                children: 4,
            },
            sites: vec![
                Site {
                    id: "app-a".to_string(),
                    name: "App A".to_string(),
                    path: std::env::temp_dir(),
                    domain: "app-a.localhost".to_string(),
                    php: "8.3.36".to_string(),
                    root: None,
                    framework: Some("laravel".to_string()),
                    backend: None,
                    enabled: true,
                },
                Site {
                    id: "app-b".to_string(),
                    name: "App B".to_string(),
                    path: std::env::temp_dir(),
                    domain: "app-b.localhost".to_string(),
                    php: "7.4.33".to_string(),
                    root: None,
                    framework: Some("codeigniter".to_string()),
                    backend: None,
                    enabled: true,
                },
            ],
        }
    }

    fn ports() -> BTreeMap<String, u16> {
        BTreeMap::from([
            ("7.4.33".to_string(), 9002u16),
            ("8.3.36".to_string(), 9001u16),
        ])
    }

    #[test]
    fn renders_one_block_per_site_with_its_own_fastcgi_port() {
        let paths = Paths {
            root: std::env::temp_dir(),
        };
        let caddy = CaddyBackend::new(paths);
        let text = caddy.render(&sample_config(), &ports()).unwrap();

        assert!(text.contains("http://app-a.localhost"));
        assert!(text.contains("php_fastcgi 127.0.0.1:9001"));
        assert!(text.contains("http://app-b.localhost"));
        assert!(text.contains("php_fastcgi 127.0.0.1:9002"));
        assert_eq!(text.matches("php_fastcgi").count(), 2);
    }

    #[test]
    fn disabled_sites_are_not_served() {
        let paths = Paths {
            root: std::env::temp_dir(),
        };
        let caddy = CaddyBackend::new(paths);
        let mut config = sample_config();
        config.sites[0].enabled = false;

        let text = caddy.render(&config, &ports()).unwrap();
        assert!(!text.contains("app-a.localhost"));
        assert!(text.contains("app-b.localhost"));
    }

    #[test]
    fn missing_php_port_is_an_error() {
        let paths = Paths {
            root: std::env::temp_dir(),
        };
        let caddy = CaddyBackend::new(paths);
        let mut map = ports();
        map.remove("7.4.33");
        assert!(caddy.render(&sample_config(), &map).is_err());
    }

    #[test]
    fn empty_config_still_valid() {
        let paths = Paths {
            root: std::env::temp_dir(),
        };
        let caddy = CaddyBackend::new(paths);
        let config = Config::default();
        let text = caddy.render(&config, &BTreeMap::new()).unwrap();
        assert!(text.contains("admin localhost:2019"));
        assert!(!text.contains("php_fastcgi"));
    }

    #[test]
    fn dashboard_lists_sites_and_escapes_names() {
        let paths = Paths {
            root: std::env::temp_dir().join(format!("devku-dash-{}", std::process::id())),
        };
        let caddy = CaddyBackend::new(paths);
        let mut config = sample_config();
        config.sites[0].name = "Toko <b>".to_string();

        caddy.write_dashboard(&config).unwrap();
        let html = std::fs::read_to_string(caddy.paths.dashboard_dir().join("index.html")).unwrap();

        assert!(html.contains("Toko &lt;b&gt;"));
        assert!(html.contains("app-a.localhost"));
        assert!(html.contains("php 8.3.36"));

        let _ = std::fs::remove_dir_all(&caddy.paths.root);
    }

    #[test]
    fn dashboard_without_sites_explains_next_step() {
        let paths = Paths {
            root: std::env::temp_dir().join(format!("devku-dash-empty-{}", std::process::id())),
        };
        let caddy = CaddyBackend::new(paths);
        let config = Config {
            sites: Vec::new(),
            ..Default::default()
        };

        caddy.write_dashboard(&config).unwrap();
        let html = std::fs::read_to_string(caddy.paths.dashboard_dir().join("index.html")).unwrap();

        assert!(html.contains("Belum ada situs"));

        let _ = std::fs::remove_dir_all(&caddy.paths.root);
    }

    #[test]
    fn render_always_serves_dashboard_on_port_80() {
        let paths = Paths {
            root: std::env::temp_dir(),
        };
        let caddy = CaddyBackend::new(paths);
        let config = Config {
            sites: Vec::new(),
            ..Default::default()
        };

        let text = caddy.render(&config, &BTreeMap::new()).unwrap();
        assert!(text.contains("http://localhost, http://127.0.0.1"));
        assert!(text.contains("\nhttp:// {"));
    }

    #[test]
    fn picks_only_error_lines_from_caddy_log() {
        let stderr = br#"{"level":"info","msg":"using config from file"}
{"level":"error","msg":"unrecognized directive: nope"}
{"level":"info","msg":"done"}"#;

        let message = extract_caddy_error(stderr, b"");
        assert_eq!(message, "unrecognized directive: nope");
    }

    #[test]
    fn falls_back_to_plain_output() {
        let message = extract_caddy_error(b"", b"Valid configuration\n");
        assert_eq!(message, "Valid configuration");
    }

    #[test]
    fn parses_load_error_body() {
        let body = r#"{"error":"Caddyfile:8: unrecognized directive: nope"}"#;
        assert_eq!(
            extract_load_error(body),
            "Caddyfile:8: unrecognized directive: nope"
        );
        assert_eq!(extract_load_error(""), "tanpa pesan");
    }

    /// Butuh binary Caddy nyata. Jalankan dengan:
    /// `$env:DEVKU_CADDY_BIN="...caddy.exe"; cargo test --lib -- --ignored`
    #[tokio::test]
    #[ignore = "butuh binary Caddy lewat DEVKU_CADDY_BIN"]
    async fn validate_accepts_good_and_rejects_bad() {
        let bin = std::env::var("DEVKU_CADDY_BIN").expect("set DEVKU_CADDY_BIN");
        let root = std::env::temp_dir().join(format!("devku-validate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let paths = Paths { root: root.clone() };
        paths.ensure().unwrap();
        let backend = CaddyBackend::new(paths);
        std::fs::create_dir_all(backend.binary().parent().unwrap()).unwrap();
        std::fs::copy(&bin, backend.binary()).unwrap();

        let good = "{\n\tadmin localhost:2019\n\tauto_https off\n}\n\n\
                    http://x.localhost:8099 {\n\trespond \"ok\"\n}\n";
        backend.validate(good).await.unwrap();

        let bad = format!("{good}\nhttp://y.localhost:8099 {{\n\tnope_directive z\n}}\n");
        let message = backend.validate(&bad).await.unwrap_err().to_string();
        assert!(
            message.contains("unrecognized directive"),
            "pesan: {message}"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
