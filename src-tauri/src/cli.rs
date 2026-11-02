use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::caddy::{CaddyBackend, WebServerBackend};
use crate::config::{Config, Site};
use crate::paths::Paths;
use crate::php::PhpManager;
use crate::proc::ServiceState;
use crate::runtime::{PHP_PORT_START, Runtime};

/// Hasil dispatch yang tidak bisa dinyatakan dengan `Result`: perintah tak
/// dikenal bukan kegagalan, tetapi punya exit code sendiri.
enum Status {
    Ok,
    UnknownCommand,
}

pub fn run(args: &[String]) -> ExitCode {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build();

    let Ok(runtime) = runtime else {
        eprintln!("DevKu: gagal menyiapkan runtime");
        return ExitCode::FAILURE;
    };

    match runtime.block_on(dispatch(args)) {
        Ok(Status::Ok) => ExitCode::SUCCESS,
        Ok(Status::UnknownCommand) => ExitCode::from(2),
        Err(err) => {
            eprintln!("DevKu: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn dispatch(args: &[String]) -> Result<Status> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(Status::Ok);
    };

    match command {
        "status" => status()?,
        "sites" => sites()?,
        "check" => check().await?,
        "install-php" => {
            let zip = args
                .get(1)
                .context("pakai: devku install-php <zip> <versi>")?;
            let version = args
                .get(2)
                .context("pakai: devku install-php <zip> <versi>")?;
            install_php(zip, version)?;
        }
        "install-caddy" => {
            let source = args
                .get(1)
                .context("pakai: devku install-caddy <path-ke-caddy>")?;
            install_caddy(source)?;
        }
        "add-site" => add_site(args)?,
        "php-path" => php_path(args.get(1).map(String::as_str))?,
        "up" => up(args).await?,
        "runtimes" => runtimes()?,
        "install-composer" => install_composer().await?,
        "composer-path" => composer_path(args.get(1).map(String::as_str))?,
        "autostart" => autostart(args.get(1).map(String::as_str))?,
        "export-config" => {
            export_config(args.get(1).context("pakai: devku export-config <berkas>")?)?
        }
        "import-config" => {
            import_config(args.get(1).context("pakai: devku import-config <berkas>")?).await?;
        }
        "trust" => trust().await?,
        other => {
            eprintln!("perintah tidak dikenal: {other}");
            eprint_help();
            return Ok(Status::UnknownCommand);
        }
    }

    Ok(Status::Ok)
}

pub fn print_help() {
    println!("{}", help_text());
}

fn eprint_help() {
    eprintln!("{}", help_text());
}

fn help_text() -> &'static str {
    "devku: development environment PHP untuk Windows\n\n\
     USAGE:\n  \
     devku                       buka aplikasi\n  \
     devku --hidden              buka di latar belakang saat login\n  \
     devku status\n  \
     devku sites\n  \
     devku check\n  \
     devku install-php <zip> <versi>\n  \
     devku install-caddy <path>\n  \
     devku add-site <id> <nama> <folder> <versi-php> [domain]\n  \
     devku php-path [on|off]\n  \
     devku up [--seconds N]\n  \
     devku runtimes\n  \
     devku install-composer\n  \
     devku composer-path [on|off]\n  \
     devku autostart [on|off]\n  \
     devku export-config <berkas>\n  \
     devku import-config <berkas>\n  \
     devku trust\n\n\
     Exit code: 0 sukses, 1 gagal, 2 perintah tidak dikenal.\n\
     DEVKU_HOME mengubah lokasi config/engines dan mengunci pemindahan\n\
     dari UI (default: %APPDATA%\\DevKu)."
}

fn php_path(mode: Option<&str>) -> Result<()> {
    let paths = Paths::resolve();

    match mode {
        Some("on") => {
            let config = Config::load(&paths)?;
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
            crate::path_env::register(&paths, &version)?;
            println!(
                "php {} -> {}",
                version,
                crate::path_env::bin_dir(&paths).display()
            );
        }
        Some("off") => {
            crate::path_env::unregister(&paths)?;
            println!(
                "{} dilepas dari PATH pengguna",
                crate::path_env::bin_dir(&paths).display()
            );
        }
        _ => {
            let status = crate::path_env::status(&paths);
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
    println!("workspace    {}", config.workspace_dir().display());
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
    let allocation = crate::ports::allocate(&versions, PHP_PORT_START)?;
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
    let root = crate::detect::detect(&path)
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
            "ready  {}  php={}  fpm=127.0.0.1:{}",
            config.site_url(site),
            site.php,
            port
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
    for runtime in crate::runtime_detect::detect(&paths) {
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
    let installed = crate::composer::install(&paths, &cancel, |stage, _, _| {
        if stage != last {
            eprintln!("[devku] {stage}");
            last = stage;
        }
    })
    .await?;

    println!("Composer {} terpasang", installed.version);
    println!("phar         {}", crate::composer::phar(&paths).display());
    println!(
        "php          {} ({})",
        installed.php.path.display(),
        installed.php.source
    );
    println!("shim         {}", crate::composer::shim(&paths).display());
    Ok(())
}

/// Kelola folder paket global Composer di PATH pengguna. Tanpa argumen,
/// tampilkan lokasi dan tool yang terlihat di dalamnya.
fn composer_path(mode: Option<&str>) -> Result<()> {
    let dir = crate::composer::global_bin_dir();

    match mode {
        Some("on") => {
            if !dir.is_dir() {
                bail!(
                    "folder {} belum ada; jalankan `composer global require laravel/installer` dulu",
                    dir.display()
                );
            }
            crate::path_env::add_dir(&dir)?;
            println!("{} ditambahkan ke PATH pengguna", dir.display());
        }
        Some("off") => {
            crate::path_env::remove_dir(&dir)?;
            println!("{} dilepas dari PATH pengguna", dir.display());
        }
        _ => {
            let status = crate::composer::global_status();
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
            crate::autostart::set_enabled(&exe, true)?;
            println!(
                "DevKu akan dibuka saat login: {}",
                crate::autostart::command_line(&exe)
            );
        }
        Some("off") => {
            crate::autostart::set_enabled(&exe, false)?;
            println!("DevKu tidak lagi dibuka saat login");
        }
        _ => {
            println!("target     {}", crate::autostart::command_line(&exe));
            println!(
                "di startup {}",
                if crate::autostart::is_enabled(&exe) {
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
    let mut session = crate::session::Session::new(Paths::resolve(), None)?;
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
