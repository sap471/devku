use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Extract a zip into `dest`, dropping one wrapper folder when all entries
/// sit inside the same folder. The PHP, Caddy, and Bun packages all
/// use that shape, so one extraction path is enough for all three.
pub fn extract(zip_path: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;

    let file = std::fs::File::open(zip_path)
        .with_context(|| format!("gagal membuka {}", zip_path.display()))?;
    let mut archive =
        zip::ZipArchive::new(file).with_context(|| format!("bukan zip: {}", zip_path.display()))?;

    let names: Vec<String> = archive
        .file_names()
        .filter_map(|name| name.ok().map(|cow| cow.to_string()))
        .collect();
    let strip = common_root(&names);

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let raw = entry.name().map(|cow| cow.to_string()).unwrap_or_default();
        let relative = match &strip {
            Some(root) => raw
                .strip_prefix(root)
                .unwrap_or(&raw)
                .trim_start_matches('/')
                .to_string(),
            None => raw.clone(),
        };
        if relative.is_empty() {
            continue;
        }

        let target = safe_join(dest, &relative)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&target)
            .with_context(|| format!("gagal menulis {}", target.display()))?;
        std::io::copy(&mut entry, &mut out)?;
    }

    Ok(())
}

/// The wrapper folder is dropped only when every file really sits inside
/// it; a mixed zip is extracted as-is.
pub fn common_root(names: &[String]) -> Option<String> {
    let mut roots = names.iter().filter_map(|name| {
        let name = name.trim_start_matches('/');
        name.split('/').next().map(str::to_string)
    });

    let first = roots.next()?;
    if roots.any(|root| root != first) {
        return None;
    }

    // If the root is a file (not a folder), nothing is dropped.
    if names
        .iter()
        .all(|name| !name.trim_start_matches('/').contains('/'))
    {
        return None;
    }

    Some(first)
}

/// Rejects zip entries that try to escape the destination folder.
pub fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    let mut out = base.to_path_buf();
    for part in Path::new(relative).components() {
        match part {
            Component::Normal(name) => out.push(name),
            Component::CurDir => {}
            _ => bail!("entri zip tidak aman: {relative}"),
        }
    }

    if !out.starts_with(base) {
        bail!("entri zip tidak aman: {relative}");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn common_root_detects_single_wrapper() {
        let list = names(&[
            "php-8.3.36/php.exe",
            "php-8.3.36/ext/php_gd.dll",
            "php-8.3.36/php.ini-development",
        ]);
        assert_eq!(common_root(&list), Some("php-8.3.36".to_string()));
    }

    #[test]
    fn common_root_none_when_flat() {
        assert_eq!(common_root(&names(&["caddy.exe", "LICENSE"])), None);
    }

    #[test]
    fn common_root_none_when_mixed() {
        let list = names(&["bun.exe", "docs/README.md"]);
        assert_eq!(common_root(&list), None);
    }

    #[test]
    fn safe_join_rejects_traversal() {
        let base = Path::new("C:\\DevKu\\engines\\php\\8.3.36");
        assert!(safe_join(base, "../evil.exe").is_err());
        assert!(safe_join(base, "/abs/evil.exe").is_err());
        assert!(safe_join(base, "ext/php_gd.dll").is_ok());
    }
}
