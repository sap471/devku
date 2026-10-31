use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use devku_lib::caddy::{CaddyBackend, WebServerBackend};
use devku_lib::config::{Config, Site};
use devku_lib::paths::Paths;
use devku_lib::php::PhpManager;
use devku_lib::proc::ServiceState;
use devku_lib::runtime::{PHP_PORT_START, Runtime};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("status") => status(),
        Some("sites") => sites(),
        Some("check") => check().await,
        Some("install-php") => {
            let zip = args
                .get(1)
                .context("pakai: devku-cli install-php <zip> <versi>")?;
            let version = args
                .get(2)
                .context("pakai: devku-cli install-php <zip> <versi>")?;
            install_php(zip, version)
        }
        Some("install-caddy") => {
            let source = args
                .get(1)
                .context("pakai: devku-cli install-caddy <path-ke-caddy>")?;
            install_caddy(source)
        }
        Some("add-site") => add_site(&args),
        Some("php-path") => php_path(args.get(1).map(String::as_str)),
        Some("up") => up(&args).await,
        Some("runtimes") => runtimes(),
        Some("install-composer") => install_composer().await,
        Some("composer-path") => composer_path(args.get(1).map(String::as_str)),
        Some("autostart") => autostart(args.get(1).map(String::as_str)),
        Some("export-config") => export_config(
            args.get(1)
                .context("pakai: devku-cli export-config <berkas>")?,
        ),
        Some("import-config") => {
            import_config(
                args.get(1)
                    .context("pakai: devku-cli import-config <berkas>")?,
            )
            .await
        }
        Some("trust") => trust().await,
        Some(other) => {
            help();
            bail!("perintah tidak dikenal: {other}")
        }
        None => {
            help();
            Ok(())
        }
    }
}

fn help() {
    println!(
        "devku-cli — kontrol core services tanpa UI\n\n\
         USAGE:\n  \
         devku-cli status\n  \
         devku-cli sites\n  \
         devku-cli check\n  \
         devku-cli install-php <zip> <versi>\n  \
         devku-cli install-caddy <path>\n  \
         devku-cli add-site <id> <nama> <folder> <versi-php> [domain]\n  \
         devku-cli php-path [on|off]\n  \
         devku-cli up [--seconds N]\n  \
         devku-cli runtimes\n  \
         devku-cli install-composer\n  \
         devku-cli composer-path [on|off]\n  \
         devku-cli autostart [on|off]\n  \
         devku-cli export-config <berkas>\n  \
         devku-cli import-config <berkas>\n  \
         devku-cli trust\n\n\
         DEVKU_HOME mengubah lokasi config/engines (default: %APPDATA%\\DevKu)."
    );
}

fn php_path(mode: Option<&str>) -> Result<()> {
    let paths = Paths::resolve();
    let config = Config::load(&paths)?;

    match mode {
        Some("on") => {
            let installed = PhpManager::new(paths.clone()).installed();
            let version = if installed
                .iter()
                .any(|version| version == &config.php.active)
            {
                config.php.active.clone()
            } else {
                installed
                    .last()
                    .cloned()
                    .context("belum ada PHP terpasang")?
            };
            devku_lib::path_env::register(&paths, &version)?;
            println!(
                "php {} -> {}",
                version,
                devku_lib::path_env::bin_dir(&paths).display()
            );
        }
        Some("off") => {
            devku_lib::path_env::unregister(&paths)?;
            println!(
                "{} dilepas dari PATH pengguna",
                devku_lib::path_env::bin_dir(&paths).display()
            );
        }
        _ => {
            let status = devku_lib::path_env::status(&paths);
            println!("folder     {}", status.dir);
            println!(
                "di PATH    {}",
                if status.registered { "ya" } else { "tidak" }
            );
            println!(
                "shim       {}",
                status.shim_version.as_deref().unwrap_or("belum dibuat")
            );
            if status.broken {
                println!("catatan    shim menunjuk PHP yang sudah tidak ada");
            }
        }
    }

    Ok(())
}

fn status() -> Result<()> {
    let paths = Paths::resolve();
    let config = Config::load(&paths)?;
    let manager = PhpManager::new(paths.clone());
    let caddy = CaddyBackend::new(paths.clone());

    println!("home         {}", paths.root.display());
    println!("config       {}", paths.config_file().display());
    println!("tld          {}", config.tld);
    println!("backend      {:?}", config.backend);
    println!(
        "caddy        {}",
        if caddy.is_installed() {
            "terpasang"
        } else {
            "BELUM"
        }
    );
    println!("php          {:?}", manager.installed());
    println!("dipakai      {:?}", config.php_versions_in_use());
    println!("sites        {}", config.sites.len());
    println!("children     {}", config.php.children);
    Ok(())
}

fn sites() -> Result<()> {
    let paths = Paths::resolve();
    let config = Config::load(&paths)?;
    if config.sites.is_empty() {
        println!("belum ada site");
        return Ok(());
    }

    println!(
        "{:<12} {:<24} {:<10} {:<8} PATH",
        "ID", "DOMAIN", "PHP", "STATUS"
    );
    for site in &config.sites {
        println!(
            "{:<12} {:<24} {:<10} {:<8} {}",
            site.id,
            site.domain,
            site.php,
            if site.enabled { "aktif" } else { "mati" },
            site.path.display()
        );
    }
    Ok(())
}

async fn check() -> Result<()> {
    let paths = Paths::resolve();
    let config = Config::load(&paths)?;
    config.validate()?;

    let versions = config.php_versions_in_use();
    let allocation = devku_lib::ports::allocate(&versions, PHP_PORT_START)?;
    let ports: BTreeMap<String, u16> = allocation.into_iter().collect();

    let caddy = CaddyBackend::new(paths.clone());
    let caddyfile = caddy.render(&config, &ports)?;
    print!("{caddyfile}");

    if caddy.is_installed() {
        caddy.validate(&caddyfile).await?;
        eprintln!("[devku] config valid");
    } else {
        eprintln!("[devku] Caddy belum terpasang; validasi dilewati");
    }
    Ok(())
}

fn install_php(zip: &str, version: &str) -> Result<()> {
    let paths = Paths::resolve();
    let manager = PhpManager::new(paths.clone());
    let dir = manager.extract(&PathBuf::from(zip), version)?;

    let mut config = Config::load(&paths)?;
    if config.php.active.is_empty() {
        config.php.active = version.to_string();
    }
    config.save(&paths)?;

    println!("PHP {version} terpasang di {}", dir.display());
    Ok(())
}

fn install_caddy(source: &str) -> Result<()> {
    let paths = Paths::resolve();
    let backend = CaddyBackend::new(paths.clone());
    let target = backend.install_binary(&PathBuf::from(source))?;
    println!("Caddy terpasang di {}", target.display());
    Ok(())
}

fn add_site(args: &[String]) -> Result<()> {
    let id = args
        .get(1)
        .context("pakai: add-site <id> <nama> <folder> <php> [domain]")?;
    let name = args.get(2).context("butuh nama")?;
    let folder = args.get(3).context("butuh folder")?;
    let php = args.get(4).context("butuh versi php")?;

    let paths = Paths::resolve();
    let mut config = Config::load(&paths)?;
    let domain = args
        .get(5)
        .cloned()
        .unwrap_or_else(|| config.domain_for(id));

    let path = PathBuf::from(folder);
    if !path.exists() {
        bail!("folder {} tidak ada", path.display());
    }
    let root = devku_lib::detect::detect(&path)
        .map(|project| project.project_root)
        .unwrap_or_else(|| path.clone());

    config.sites.retain(|site| site.id != *id);
    config.sites.push(Site {
        id: id.clone(),
        name: name.clone(),
        path,
        domain,
        php: php.clone(),
        root: Some(root),
        framework: None,
        backend: None,
        enabled: true,
    });
    config.save(&paths)?;

    let site = config.site(id).unwrap();
    println!(
        "site {} -> http://{} (PHP {})",
        site.id, site.domain, site.php
    );
    Ok(())
}

async fn up(args: &[String]) -> Result<()> {
    let paths = Paths::resolve();
    let config = Config::load(&paths)?;
    let seconds = parse_seconds(args)?;

    let hook = Arc::new(|name: &str, state: &ServiceState| {
        eprintln!("[devku] {name}: {state}");
    });

    let runtime = Runtime::start(&config, paths.clone(), Some(hook)).await?;

    for site in config.served_sites() {
        let port = runtime.port_of(&site.php).unwrap_or(0);
        println!(
            "ready  http://{}  php={}  fpm=127.0.0.1:{}",
            site.domain, site.php, port
        );
    }

    match seconds {
        Some(secs) => {
            eprintln!("[devku] berjalan {secs} detik");
            tokio::time::sleep(Duration::from_secs(secs)).await;
        }
        None => {
            eprintln!("[devku] siap. Ctrl+C untuk berhenti.");
            tokio::signal::ctrl_c().await?;
        }
    }

    runtime.shutdown().await;
    eprintln!("[devku] semua proses berhenti");
    Ok(())
}

fn parse_seconds(args: &[String]) -> Result<Option<u64>> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--seconds" {
            let value = iter.next().context("--seconds butuh angka")?;
            return Ok(Some(value.parse().context("--seconds harus angka")?));
        }
    }
    Ok(None)
}

/// Cermin panel "Runtime" di UI tanpa membuka jendela: apa yang DevKu lihat
/// sebagai runtime sistem atau bawaan, lengkap dengan jalurnya.
fn runtimes() -> Result<()> {
    let paths = Paths::resolve();
    for runtime in devku_lib::runtime_detect::detect(&paths) {
        let version = runtime.version.as_deref().unwrap_or("-");
        let path = runtime.path.as_deref().unwrap_or("");
        println!(
            "{:<10} {:<12} {:<10} {}",
            runtime.name, version, runtime.source, path
        );
    }
    Ok(())
}

/// Pasang Composer langsung dari CLI. PHP diambil dari PATH lebih dulu, sama
/// seperti tombol di UI.
async fn install_composer() -> Result<()> {
    let paths = Paths::resolve();
    let cancel = AtomicBool::new(false);

    eprintln!("[devku] menyiapkan Composer...");
    let mut last = "";
    let installed = devku_lib::composer::install(&paths, &cancel, |stage, _, _| {
        if stage != last {
            eprintln!("[devku] {stage}");
            last = stage;
        }
    })
    .await?;

    println!("Composer {} terpasang", installed.version);
    println!(
        "phar         {}",
        devku_lib::composer::phar(&paths).display()
    );
    println!(
        "php          {} ({})",
        installed.php.path.display(),
        installed.php.source
    );
    println!(
        "shim         {}",
        devku_lib::composer::shim(&paths).display()
    );
    Ok(())
}

/// Kelola folder paket global Composer di PATH pengguna. Tanpa argumen,
/// tampilkan lokasi dan tool yang terlihat di dalamnya.
fn composer_path(mode: Option<&str>) -> Result<()> {
    let dir = devku_lib::composer::global_bin_dir();

    match mode {
        Some("on") => {
            if !dir.is_dir() {
                bail!(
                    "folder {} belum ada; jalankan `composer global require laravel/installer` dulu",
                    dir.display()
                );
            }
            devku_lib::path_env::add_dir(&dir)?;
            println!("{} ditambahkan ke PATH pengguna", dir.display());
        }
        Some("off") => {
            devku_lib::path_env::remove_dir(&dir)?;
            println!("{} dilepas dari PATH pengguna", dir.display());
        }
        _ => {
            let status = devku_lib::composer::global_status();
            println!("folder     {}", status.dir);
            println!(
                "di PATH    {}",
                if status.registered { "ya" } else { "tidak" }
            );
            println!(
                "tools      {}",
                if status.tools.is_empty() {
                    "belum ada".to_string()
                } else {
                    status.tools.join(", ")
                }
            );
        }
    }

    Ok(())
}

/// Daftarkan atau lepas DevKu dari startup Windows. Nilai Run menunjuk
/// `devku.exe` di folder yang sama dengan CLI ini.
fn autostart(mode: Option<&str>) -> Result<()> {
    let exe = std::env::current_exe()?.with_file_name("devku.exe");

    match mode {
        Some("on") => {
            devku_lib::autostart::set_enabled(&exe, true)?;
            println!(
                "DevKu akan dibuka saat login: {}",
                devku_lib::autostart::command_line(&exe)
            );
        }
        Some("off") => {
            devku_lib::autostart::set_enabled(&exe, false)?;
            println!("DevKu tidak lagi dibuka saat login");
        }
        _ => {
            println!("target     {}", devku_lib::autostart::command_line(&exe));
            println!(
                "di startup {}",
                if devku_lib::autostart::is_enabled(&exe) {
                    "ya"
                } else {
                    "tidak"
                }
            );
        }
    }

    Ok(())
}

/// Tulis config ke berkas supaya bisa dibagikan ke mesin lain.
fn export_config(target: &str) -> Result<()> {
    let paths = Paths::resolve();
    let config = Config::load(&paths)?;
    std::fs::write(target, serde_json::to_string_pretty(&config)?)
        .with_context(|| format!("gagal menulis {target}"))?;
    println!("config ditulis ke {target}");
    Ok(())
}

/// Impor config dari berkas. Folder project yang tidak ada di mesin ini
/// dilewati, dan situs dengan PHP belum terpasang dibuat nonaktif.
async fn import_config(source: &str) -> Result<()> {
    let text =
        std::fs::read_to_string(source).with_context(|| format!("gagal membaca {source}"))?;
    let mut session = devku_lib::session::Session::new(Paths::resolve(), None)?;
    let report = session.import_config(&text).await?;

    println!("{} situs diimpor", report.imported);
    if !report.skipped.is_empty() {
        println!("dilewati, folder tidak ada: {}", report.skipped.join(", "));
    }
    if !report.disabled.is_empty() {
        println!(
            "dinonaktifkan, PHP belum terpasang: {}",
            report.disabled.join(", ")
        );
    }
    if let Some(version) = report.active_php_cleared {
        println!("php aktif dikosongkan: {version} belum terpasang");
    }

    Ok(())
}

/// Pasang CA lokal Caddy ke trust store, langsung dari CLI. Caddy harus
/// berjalan (admin API hidup) seperti tombol UI menjalankannya.
async fn trust() -> Result<()> {
    let paths = Paths::resolve();
    let backend = CaddyBackend::new(paths.clone());
    let status = backend.trust().await?;
    println!("certificate: {status}");
    Ok(())
}
