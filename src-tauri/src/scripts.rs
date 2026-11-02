use std::path::Path;

use serde::Serialize;

/// One button in the site panel. `command` is sent to the terminal as-is.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CommandView {
    pub id: String,
    pub label: String,
    pub command: String,
}

/// Only commands whose marker really exists in the folder, so no
/// button is guaranteed to fail when clicked.
pub fn for_site(root: &Path, bun: bool) -> Vec<CommandView> {
    let mut out = Vec::new();

    if root.join("composer.json").exists() {
        out.push(view(
            "composer-install",
            "composer install",
            "composer install",
        ));
    }

    if root.join("artisan").exists() {
        out.push(view(
            "artisan-migrate",
            "artisan migrate",
            "php artisan migrate",
        ));
        out.push(view("artisan-serve", "artisan serve", "php artisan serve"));
    }

    if root.join("spark").exists() {
        out.push(view("spark-serve", "spark serve", "php spark serve"));
    }

    let runner = if bun { "bun run" } else { "npm run" };
    for script in package_scripts(root) {
        let command = format!("{runner} {script}");
        out.push(CommandView {
            id: format!("script-{script}"),
            label: command.clone(),
            command,
        });
    }

    out
}

fn view(id: &str, label: &str, command: &str) -> CommandView {
    CommandView {
        id: id.to_string(),
        label: label.to_string(),
        command: command.to_string(),
    }
}

fn package_scripts(root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join("package.json")) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = json.get("scripts").and_then(|value| value.as_object()) else {
        return Vec::new();
    };

    scripts
        .keys()
        .filter(|key| {
            !key.is_empty()
                && key
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '-' | '_' | '.'))
        })
        .take(8)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("devku-scripts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn laravel_project_offers_composer_and_artisan() {
        let root = sandbox("laravel");
        std::fs::write(root.join("composer.json"), "{}").unwrap();
        std::fs::write(root.join("artisan"), "").unwrap();

        let commands = for_site(&root, true);
        let names: Vec<&str> = commands.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(
            names,
            vec!["composer install", "artisan migrate", "artisan serve"]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn package_scripts_use_bun_when_available() {
        let root = sandbox("pkg");
        std::fs::write(
            root.join("package.json"),
            r#"{"scripts":{"dev":"vite","build":"vite build"}}"#,
        )
        .unwrap();

        let commands = for_site(&root, true);
        assert_eq!(commands[0].command, "bun run build");
        assert_eq!(commands[1].command, "bun run dev");

        let commands = for_site(&root, false);
        assert_eq!(commands[0].command, "npm run build");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn odd_script_names_are_skipped() {
        let root = sandbox("odd");
        std::fs::write(
            root.join("package.json"),
            r#"{"scripts":{"dev && rm -rf /":"x","ok:dev":"y"}}"#,
        )
        .unwrap();

        let commands = for_site(&root, true);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command, "bun run ok:dev");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_folder_offers_nothing() {
        let root = sandbox("empty");
        assert!(for_site(&root, true).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
