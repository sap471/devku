//! PHP extensions come from two sources: `php<name>.dll` files in the
//! `ext/` folder, and `extension=` / `zend_extension=` directives in php.ini. Every
//! function here works purely on text so it can be tested without files.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Directive {
    Extension,
    ZendExtension,
}

impl Directive {
    fn prefix(self) -> &'static str {
        match self {
            Directive::Extension => "extension=",
            Directive::ZendExtension => "zend_extension=",
        }
    }

    fn parse(trimmed: &str) -> Option<Directive> {
        if trimmed.starts_with("zend_extension=") {
            Some(Directive::ZendExtension)
        } else if trimmed.starts_with("extension=") {
            Some(Directive::Extension)
        } else {
            None
        }
    }
}

#[derive(Debug)]
struct Occurrence {
    kind: Directive,
    active: bool,
}

/// One row in the extension list. `dll` says the php_*.dll file exists in
/// the ext folder; a directive without the dll makes php-cgi fail, so the UI must
/// forbid enabling it.
#[derive(Debug, PartialEq)]
pub struct ExtensionEntry {
    pub name: String,
    pub enabled: bool,
    pub zend: bool,
    pub dll: bool,
}

/// Extension names valid for writing to php.ini: alphanumeric plus `_-.`,
/// no spaces, quotes, or line separators. Returns the lowercase form.
pub fn normalize_name(name: &str) -> Option<String> {
    let name = name.trim().trim_end_matches(".dll");
    let name = name.strip_prefix("php_").unwrap_or(name);
    let name = name.trim_matches('"');
    if name.is_empty() {
        return None;
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        return None;
    }
    Some(name.to_ascii_lowercase())
}

/// Extension list = dlls in the ext folder plus directives already
/// written in php.ini (css/dll names are trimmed to plain extension names).
pub fn scan(ini_text: &str, dlls: BTreeSet<String>) -> Vec<ExtensionEntry> {
    let mut by_name: BTreeMap<String, Vec<Occurrence>> = BTreeMap::new();

    for line in ini_text.lines() {
        let trimmed = line.trim_start();
        let (kind, value, commented) = if let Some(rest) = trimmed.strip_prefix(';') {
            let rest = rest.trim_start();
            match Directive::parse(rest) {
                Some(kind) => (kind, &rest[kind.prefix().len()..], true),
                None => continue,
            }
        } else {
            match Directive::parse(trimmed) {
                Some(kind) => (kind, &trimmed[kind.prefix().len()..], false),
                None => continue,
            }
        };

        let Some(name) = normalize_name(value) else {
            continue;
        };
        by_name.entry(name).or_default().push(Occurrence {
            kind,
            active: !commented,
        });
    }

    // Names that exist only as a dll (not yet in php.ini) are still included.
    for name in dlls.iter() {
        by_name.entry(name.clone()).or_default();
    }

    by_name
        .into_iter()
        .map(|(name, objects)| {
            // The vector can be empty: that name exists only as a dll and has not
            // been mentioned in php.ini yet.
            let zend = objects
                .last()
                .is_some_and(|object| object.kind == Directive::ZendExtension);
            ExtensionEntry {
                name: name.clone(),
                enabled: objects.iter().any(|object| object.active),
                zend,
                dll: dlls.contains(&name),
            }
        })
        .collect()
}

/// Enables/disables one extension in the php.ini text. Returns `None`
/// when nothing changed, so the caller does not rewrite the file
/// for nothing.
pub fn toggle(ini_text: &str, name: &str, enable: bool) -> Result<Option<String>> {
    let Some(target) = normalize_name(name) else {
        bail!("nama ekstensi tidak sah: {name}");
    };

    let lines: Vec<String> = ini_text.lines().map(str::to_string).collect();
    let mut found: Vec<(Directive, usize)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let (kind, value) = if let Some(rest) = trimmed.strip_prefix(';') {
            let rest = rest.trim_start();
            match Directive::parse(rest) {
                Some(kind) => (kind, &rest[kind.prefix().len()..]),
                None => continue,
            }
        } else {
            match Directive::parse(trimmed) {
                Some(kind) => (kind, &trimmed[kind.prefix().len()..]),
                None => continue,
            }
        };
        if normalize_name(value).is_some_and(|candidate| candidate == target) {
            found.push((kind, index));
        }
    }

    let uncommitted = !found.iter().any(|(_, index)| {
        lines[*index]
            .trim_start()
            .chars()
            .next()
            .is_some_and(|ch| ch != ';')
    });

    if enable {
        if !uncommitted {
            return Ok(None);
        }
        return Ok(Some(enable_lines(&lines, &target, &found)));
    }
    if uncommitted {
        return Ok(None);
    }
    Ok(Some(disable_lines(&lines, &target, &found)))
}

/// Enable: uncomment the last existing directive, or append
/// a new line at the end of the file, keeping one special marker intact.
fn enable_lines(lines: &[String], target: &str, found: &[(Directive, usize)]) -> String {
    let mut next: Vec<String> = lines.to_vec();

    if let Some(&(kind, index)) = found.last() {
        next[index] = format!("{}{target}", kind.prefix());
    } else {
        let needs_blank = next.last().is_some_and(|line| !line.trim().is_empty());
        if needs_blank {
            next.push(String::new());
        }
        next.push("; Dikelola DevKu.".to_string());
        next.push(format!("extension={target}"));
    }

    let mut text = next.join("\n");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// Disable: comment out every active directive. The rest is untouched.
fn disable_lines(lines: &[String], target: &str, found: &[(Directive, usize)]) -> String {
    let mut next: Vec<String> = lines.to_vec();
    for &(kind, index) in found {
        let line = &next[index];
        if line.trim_start().starts_with(';') {
            continue;
        }
        next[index] = format!(";{}{target}", kind.prefix());
    }
    next.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dlls(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn scan_unites_ini_and_dll() {
        let ini = ";extension=curl\nextension=mbstring\n;zend_extension=opcache\n";
        let entries = scan(ini, dlls(&["curl", "gd", "opcache"]));

        let curl = entries.iter().find(|entry| entry.name == "curl").unwrap();
        assert!(!curl.enabled && curl.dll);

        let mbstring = entries
            .iter()
            .find(|entry| entry.name == "mbstring")
            .unwrap();
        assert!(mbstring.enabled && !mbstring.zend);

        let opcache = entries
            .iter()
            .find(|entry| entry.name == "opcache")
            .unwrap();
        assert!(opcache.zend && !opcache.enabled);

        let gd = entries.iter().find(|entry| entry.name == "gd").unwrap();
        assert!(gd.dll && !gd.enabled && !gd.zend);
    }

    #[test]
    fn scan_handles_dll_suffix_and_case() {
        let entries = scan("extension=Pdo_Mysql.dll\n", dlls(&["pdo_mysql"]));
        let pdo = entries
            .iter()
            .find(|entry| entry.name == "pdo_mysql")
            .unwrap();
        assert!(pdo.enabled && pdo.dll);
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn toggle_uncomments_and_can_recomment() {
        let text = ";extension=snmp\nextension=mbstring\n";
        let on = toggle(text, "snmp", true).unwrap().unwrap();
        assert!(on.contains("extension=snmp"));

        let off = toggle(&on, "snmp", false).unwrap().unwrap();
        assert!(off.contains(";extension=snmp"));
        assert!(
            !off.lines()
                .any(|line| line.trim_start().starts_with("extension=snmp"))
        );
    }

    #[test]
    fn toggle_appends_unknown_directive() {
        let text = "max_execution_time = 30\n";
        let next = toggle(text, "gd", true).unwrap().unwrap();
        assert!(next.contains("; Dikelola DevKu."));
        assert!(next.contains("extension=gd"));

        // Safe to run twice: it does not add a second line.
        assert_eq!(toggle(&next, "gd", true).unwrap(), None);
    }

    #[test]
    fn toggle_off_unknown_directives_is_neutral() {
        let text = ";extension=zip\n";
        assert_eq!(toggle(text, "zip", false).unwrap(), None);
    }

    #[test]
    fn toggle_rejects_bad_names() {
        let err = toggle("x", "curl.dll; upload_tmp_dir=/x", true);
        assert!(err.is_err());
    }

    #[test]
    fn enabled_state_read_from_active_line() {
        assert_eq!(toggle("extension=gd\n", "gd", true).unwrap(), None);
        assert_eq!(toggle(";extension=gd\n", "gd", false).unwrap(), None);
    }
}
