//! Comptes synchronisés et extensions installées, lus dans le profil.
//!
//! Les bases analysées se trouvent dans le dossier de profil du navigateur,
//! aux côtés de fichiers de configuration (`Preferences`, `extensions.json`,
//! dossier `Extensions/`, `prefs.js`…). Ce module remonte depuis le chemin
//! d'une base vers le profil, puis en extrait le compte de synchronisation et
//! la liste des extensions, en lecture seule.

use crate::model::{Artifact, Cell, Row};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const SYNC_COLUMNS: &[&str] = &["Navigateur", "Service", "Compte", "Détail", "Source"];
pub const EXT_COLUMNS: &[&str] =
    &["Navigateur", "Extension", "Version", "Permissions", "Identifiant"];

fn is_chromium(browser: &str) -> bool {
    matches!(
        browser,
        "Chrome" | "Edge" | "Brave" | "Opera" | "Vivaldi" | "Chromium" | "IE/Edge Legacy"
    )
}

/// Dossiers de profil distincts (un profil peut porter plusieurs bases).
fn profile_dirs(arts: &[Artifact]) -> Vec<(PathBuf, String)> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for art in arts {
        if let Some(dir) = Path::new(&art.path).parent() {
            let key = format!("{}|{}", dir.display(), art.browser);
            if seen.insert(key) {
                out.push((dir.to_path_buf(), art.browser.clone()));
            }
        }
    }
    out
}

fn read_json(path: &Path) -> Option<Value> {
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

// --- Comptes synchronisés --------------------------------------------------

pub fn build_sync_accounts(arts: &[Artifact]) -> Vec<Row> {
    let mut out = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for (dir, browser) in profile_dirs(arts) {
        if is_chromium(&browser) {
            for name in ["Preferences", "Secure Preferences"] {
                let p = dir.join(name);
                if let Some(v) = read_json(&p) {
                    chromium_accounts(&v, &browser, &p.to_string_lossy(), &mut out, &mut seen);
                }
            }
        } else if browser == "Firefox" {
            firefox_accounts(&dir, &mut out, &mut seen);
        }
    }
    out
}

fn chromium_accounts(
    v: &Value,
    browser: &str,
    src: &str,
    out: &mut Vec<Row>,
    seen: &mut HashSet<(String, String)>,
) {
    if let Some(arr) = v.get("account_info").and_then(|a| a.as_array()) {
        for acc in arr {
            let email = acc.get("email").and_then(|e| e.as_str()).unwrap_or("");
            if email.is_empty() {
                continue;
            }
            let name = acc
                .get("full_name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            push_sync(out, seen, browser, "Synchronisation", email, &name, src);
        }
    }
}

fn firefox_accounts(
    dir: &Path,
    out: &mut Vec<Row>,
    seen: &mut HashSet<(String, String)>,
) {
    // signedInUser.json
    let sju = dir.join("signedInUser.json");
    if let Some(v) = read_json(&sju) {
        let email = v
            .get("accountData")
            .and_then(|a| a.get("email"))
            .and_then(|e| e.as_str())
            .or_else(|| v.get("email").and_then(|e| e.as_str()))
            .unwrap_or("");
        if !email.is_empty() {
            push_sync(out, seen, "Firefox", "Firefox Sync", email, "", &sju.to_string_lossy());
        }
    }
    // prefs.js : user_pref("services.sync.username", "email");
    let prefs_path = dir.join("prefs.js");
    if let Ok(prefs) = std::fs::read_to_string(&prefs_path) {
        for line in prefs.lines() {
            if let Some(email) = pref_value(line, "services.sync.username") {
                push_sync(out, seen, "Firefox", "Firefox Sync", &email, "", &prefs_path.to_string_lossy());
            }
        }
    }
}

/// Extrait la valeur d'un `user_pref("<clé>", "<valeur>");`.
fn pref_value(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let rest = line.split_once(&needle)?.1;
    let start = rest.find('"')? + 1;
    let end = rest[start..].find('"')? + start;
    Some(rest[start..end].to_string())
}

#[allow(clippy::too_many_arguments)]
fn push_sync(
    out: &mut Vec<Row>,
    seen: &mut HashSet<(String, String)>,
    browser: &str,
    service: &str,
    account: &str,
    detail: &str,
    src: &str,
) {
    if !seen.insert((browser.to_string(), account.to_string())) {
        return;
    }
    let mut r = Row::new();
    r.set("Navigateur", Cell::text(browser));
    r.set("Service", Cell::text(service));
    r.set("Compte", Cell::text(account));
    r.set("Détail", Cell::text(detail));
    r.set("Source", Cell::text(src));
    r.set("_chemin", Cell::text(src));
    out.push(r);
}

// --- Extensions ------------------------------------------------------------

pub fn build_extensions(arts: &[Artifact]) -> Vec<Row> {
    let mut out = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for (dir, browser) in profile_dirs(arts) {
        if is_chromium(&browser) {
            chromium_extensions(&dir, &browser, &mut out, &mut seen);
        } else if browser == "Firefox" {
            firefox_extensions(&dir, &mut out, &mut seen);
        }
    }
    out
}

fn chromium_extensions(
    dir: &Path,
    browser: &str,
    out: &mut Vec<Row>,
    seen: &mut HashSet<(String, String)>,
) {
    let ext_dir = dir.join("Extensions");
    let ids = match std::fs::read_dir(&ext_dir) {
        Ok(d) => d,
        Err(_) => return,
    };
    for id_entry in ids.flatten() {
        let id = id_entry.file_name().to_string_lossy().to_string();
        let vers = match std::fs::read_dir(id_entry.path()) {
            Ok(d) => d,
            Err(_) => continue,
        };
        for ver_entry in vers.flatten() {
            let manifest = ver_entry.path().join("manifest.json");
            if let Some(v) = read_json(&manifest) {
                let raw_name = v.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let name = if raw_name.is_empty() || raw_name.starts_with("__MSG_") {
                    format!("(id {id})")
                } else {
                    raw_name.to_string()
                };
                let version = v.get("version").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let perms = collect_perms(&v);
                push_ext(out, seen, browser, &name, &version, &perms, &id, &manifest.to_string_lossy());
            }
        }
    }
}

fn collect_perms(v: &Value) -> String {
    let mut perms: Vec<String> = Vec::new();
    for key in ["permissions", "optional_permissions", "host_permissions"] {
        if let Some(arr) = v.get(key).and_then(|a| a.as_array()) {
            for p in arr {
                if let Some(s) = p.as_str() {
                    perms.push(s.to_string());
                }
            }
        }
    }
    perms.join(", ")
}

fn firefox_extensions(
    dir: &Path,
    out: &mut Vec<Row>,
    seen: &mut HashSet<(String, String)>,
) {
    let ext_json = dir.join("extensions.json");
    let v = match read_json(&ext_json) {
        Some(v) => v,
        None => return,
    };
    let ext_json_s = ext_json.to_string_lossy().to_string();
    let addons = match v.get("addons").and_then(|a| a.as_array()) {
        Some(a) => a,
        None => return,
    };
    for a in addons {
        // Ignore les composants internes du navigateur.
        if a.get("location").and_then(|l| l.as_str()) == Some("app-builtin") {
            continue;
        }
        let name = a
            .get("defaultLocale")
            .and_then(|d| d.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let version = a.get("version").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let id = a.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let perms = a
            .get("userPermissions")
            .and_then(|p| p.get("permissions"))
            .and_then(|p| p.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        push_ext(out, seen, "Firefox", &name, &version, &perms, &id, &ext_json_s);
    }
}

#[allow(clippy::too_many_arguments)]
fn push_ext(
    out: &mut Vec<Row>,
    seen: &mut HashSet<(String, String)>,
    browser: &str,
    name: &str,
    version: &str,
    perms: &str,
    id: &str,
    chemin: &str,
) {
    if !seen.insert((browser.to_string(), id.to_string())) {
        return;
    }
    let mut r = Row::new();
    r.set("Navigateur", Cell::text(browser));
    r.set("Extension", Cell::text(name));
    r.set("Version", Cell::text(version));
    r.set("Permissions", Cell::text(perms));
    r.set("Identifiant", Cell::text(id));
    r.set("_chemin", Cell::text(chemin));
    out.push(r);
}
