use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
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
        Some("check") => check(),
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
        Some("up") => up(&args).await,
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
         devku-cli up [--seconds N]\n\n\
         DEVKU_HOME mengubah lokasi config/engines (default: %APPDATA%\\DevKu)."
    );
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

fn check() -> Result<()> {
    let paths = Paths::resolve();
    let config = Config::load(&paths)?;
    config.validate()?;

    let versions = config.php_versions_in_use();
    let allocation = devku_lib::ports::allocate(&versions, PHP_PORT_START)?;
    let ports: BTreeMap<String, u16> = allocation.into_iter().collect();

    let caddy = CaddyBackend::new(paths.clone());
    print!("{}", caddy.render(&config, &ports)?);
    eprintln!("\n[devku] caddy terpasang: {}", caddy.is_installed());
    Ok(())
}

fn install_php(zip: &str, version: &str) -> Result<()> {
    let paths = Paths::resolve();
    let manager = PhpManager::new(paths.clone());
    let dir = manager.extract(&PathBuf::from(zip), version)?;

    let mut config = Config::load(&paths)?;
    config.php.installed.insert(version.to_string());
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

    config.sites.retain(|site| site.id != *id);
    config.sites.push(Site {
        id: id.clone(),
        name: name.clone(),
        path,
        domain,
        php: php.clone(),
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
