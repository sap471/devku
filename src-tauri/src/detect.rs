use std::path::{Path, PathBuf};

/// Result of scanning a project folder. `framework` is deliberately a short string
/// so it can be stored in config and shown as-is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    pub framework: &'static str,
    pub document_root: PathBuf,
    /// Folder the user picked, used by the UI to show what was read.
    pub project_root: PathBuf,
}

/// Find framework markers from the folder contents. Order matters: Laravel and CI4
/// win over generic detection because `public/index.php` is there too.
pub fn detect(path: &Path) -> Option<Detection> {
    if !path.is_dir() {
        return None;
    }

    let project_root = path.to_path_buf();

    if path.join("artisan").is_file() {
        return Some(Detection {
            framework: "laravel",
            document_root: public_or(path),
            project_root,
        });
    }

    if path.join("spark").is_file() {
        return Some(Detection {
            framework: "codeigniter4",
            document_root: public_or(path),
            project_root,
        });
    }

    if path.join("system").is_dir() && path.join("index.php").is_file() {
        return Some(Detection {
            framework: "codeigniter3",
            document_root: path.to_path_buf(),
            project_root,
        });
    }

    if path.join("public").join("index.php").is_file() {
        return Some(Detection {
            framework: "php",
            document_root: path.join("public"),
            project_root,
        });
    }

    if path.join("index.php").is_file() {
        return Some(Detection {
            framework: "php",
            document_root: path.to_path_buf(),
            project_root,
        });
    }

    None
}

fn public_or(path: &Path) -> PathBuf {
    let public = path.join("public");
    if public.is_dir() {
        public
    } else {
        path.to_path_buf()
    }
}

/// Label shown to the user. Do not invent a fancier name.
pub fn label(framework: &str) -> &str {
    match framework {
        "laravel" => "Laravel",
        "codeigniter3" => "CodeIgniter 3",
        "codeigniter4" => "CodeIgniter 4",
        "php" => "PHP",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("devku-detect-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn detects_laravel_and_uses_public() {
        let dir = temp_project("laravel");
        write(&dir.join("artisan"), "");
        write(&dir.join("public/index.php"), "<?php");

        let found = detect(&dir).unwrap();
        assert_eq!(found.framework, "laravel");
        assert_eq!(found.document_root, dir.join("public"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_codeigniter4() {
        let dir = temp_project("ci4");
        write(&dir.join("spark"), "");
        write(&dir.join("public/index.php"), "<?php");

        let found = detect(&dir).unwrap();
        assert_eq!(found.framework, "codeigniter4");
        assert_eq!(found.document_root, dir.join("public"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_codeigniter3_from_system_dir() {
        let dir = temp_project("ci3");
        std::fs::create_dir_all(dir.join("system")).unwrap();
        write(&dir.join("index.php"), "<?php");

        let found = detect(&dir).unwrap();
        assert_eq!(found.framework, "codeigniter3");
        assert_eq!(found.document_root, dir);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_plain_php_in_public() {
        let dir = temp_project("php-public");
        write(&dir.join("public/index.php"), "<?php");

        let found = detect(&dir).unwrap();
        assert_eq!(found.framework, "php");
        assert_eq!(found.document_root, dir.join("public"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_folder_without_php() {
        let dir = temp_project("empty");
        write(&dir.join("README.md"), "not php");

        assert!(detect(&dir).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
