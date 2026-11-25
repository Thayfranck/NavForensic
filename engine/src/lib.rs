//! NavForensics — coeur d'analyse forensique des bases de navigateurs.
//!
//! Lecture seule sur copie (sidecars -wal/-shm inclus), empreinte SHA-256,
//! conversions d'horodatage par navigateur, aucun secret dechiffre.

pub mod db;
pub mod ese;
pub mod model;
pub mod parsers;
pub mod pdf;
pub mod report;
pub mod sessions;
pub mod taxonomy;
pub mod time;
pub mod views;

pub use model::{Artifact, Category, Cell, Row};
pub use report::{CaseInfo, ReportOptions};
pub use taxonomy::{
    build_flagged, classify_url, sub_category, FLAGGED_COLUMNS, FLAGGED_SUB, SUBCATEGORY_ORDER,
};
pub use views::{
    b64_decode, build_domain_summary, build_downloads, build_ga_cookies, build_geolocation,
    build_histogram, build_identity, build_online_services, build_search_terms, build_stats,
    build_timeline, build_vpn, carve_cache_html, carve_cache_media, carve_cache_urls, search_all,
    search_keywords, HtmlItem, HtmlResource, MediaItem, DOMAIN_COLUMNS, DOWNLOADS_COLUMNS,
    GA_COLUMNS, GEO_COLUMNS, HISTO_COLUMNS, HTML_MAX_ITEMS, IDENTITY_COLUMNS, MEDIA_MAX_BYTES,
    MEDIA_MAX_ITEMS, ONLINE_COLUMNS, SEARCH_COLUMNS, SEARCH_KW_COLUMNS, SEARCH_TERMS_COLUMNS,
    STATS_COLUMNS, TAG_COLUMNS, TIMELINE_COLUMNS, TIMELINE_CORE, TIMELINE_NOISY, VPN_COLUMNS,
};

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Version et date d'installation du navigateur, déduites au mieux des fichiers
/// voisins du profil (Chromium « Last Version », Firefox compatibility.ini /
/// times.json, date de création du dossier de profil). Best-effort : renvoie
/// None quand l'information n'est pas disponible.
pub fn browser_version_install(path: &str) -> (Option<String>, Option<i64>) {
    let p = Path::new(path);
    let profile = match p.parent() {
        Some(d) => d,
        None => return (None, None),
    };
    let mut version: Option<String> = None;
    let mut installed: Option<i64> = None;

    // Firefox : compatibility.ini (LastVersion=...)
    if let Ok(txt) = std::fs::read_to_string(profile.join("compatibility.ini")) {
        for line in txt.lines() {
            if let Some(v) = line.strip_prefix("LastVersion=") {
                let v = v.split(['_', ' ']).next().unwrap_or(v).trim();
                if !v.is_empty() {
                    version = Some(v.to_string());
                }
            }
        }
    }
    // Firefox : times.json ("created": <ms>)
    if let Ok(txt) = std::fs::read_to_string(profile.join("times.json")) {
        if let Some(idx) = txt.find("\"created\"") {
            let digits: String = txt[idx..]
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(ms) = digits.parse::<i64>() {
                installed = Some(ms);
            }
        }
    }
    // Chromium : <User Data>/Last Version (parent du dossier de profil)
    if version.is_none() {
        if let Some(userdata) = profile.parent() {
            if let Ok(txt) = std::fs::read_to_string(userdata.join("Last Version")) {
                let v = txt.trim().to_string();
                if !v.is_empty() {
                    version = Some(v);
                }
            }
        }
    }
    // Installation ≈ création du dossier de profil
    if installed.is_none() {
        if let Ok(meta) = std::fs::metadata(profile) {
            if let Ok(ct) = meta.created() {
                if let Ok(d) = ct.duration_since(std::time::UNIX_EPOCH) {
                    installed = Some(d.as_millis() as i64);
                }
            }
        }
    }
    (version, installed)
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn json_kind(path: &Path) -> Option<&'static str> {
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let obj = data.as_object()?;
    if obj.contains_key("roots") {
        return Some("chromium_bookmarks");
    }
    if data.get("logins").map(|v| v.is_array()).unwrap_or(false) {
        return Some("firefox_logins");
    }
    if ["account_info", "extensions", "profile", "session"]
        .iter()
        .any(|k| obj.contains_key(*k))
    {
        return Some("chromium_preferences");
    }
    None
}

fn plist_kind(path: &Path) -> Option<&'static str> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    if name.contains("download") {
        return Some("safari_downloads");
    }
    if name.contains("bookmark") {
        return Some("safari_bookmarks");
    }
    let v = plist::Value::from_file(path).ok()?;
    let d = v.as_dictionary()?;
    if d.contains_key("DownloadHistory") {
        return Some("safari_downloads");
    }
    if d.get("WebBookmarkType").and_then(|x| x.as_string()) == Some("WebBookmarkTypeList")
        || d.contains_key("Children")
    {
        return Some("safari_bookmarks");
    }
    None
}

pub fn detect(path: &Path) -> Option<&'static str> {
    // Reconnaissance par nom : WebCacheV*.dat (base ESE IE/Edge Legacy), souvent
    // verrouillee en live donc parfois illisible — on la reconnait quand meme.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let n = name.to_ascii_lowercase();
        if n.starts_with("webcachev") && n.ends_with(".dat") {
            return Some("ie_webcache");
        }
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 16];
    let n = f.read(&mut head).ok()?;
    let head = &head[..n];
    if head.starts_with(b"SNSS") {
        return Some("chromium_session");
    }
    if head.starts_with(b"mozLz4a\0") || head.starts_with(b"mozLz40\0") {
        return Some("firefox_session");
    }
    if head.len() >= 8 && head[4..8] == [0xef, 0xcd, 0xab, 0x89] {
        return Some("ie_webcache");
    }
    let trimmed: &[u8] = {
        let mut i = 0;
        while i < head.len() && head[i].is_ascii_whitespace() {
            i += 1;
        }
        &head[i..]
    };
    if trimmed.first() == Some(&b'{') {
        return json_kind(path);
    }
    if head.starts_with(b"bplist00") || trimmed.starts_with(b"<?xml") || trimmed.starts_with(b"<plist")
    {
        return plist_kind(path);
    }

    // n'essaie d'ouvrir en SQLite (ce qui copie le fichier) que si l'en-tete
    // est bien celle d'une base SQLite : evite de copier caches/images lors d'un
    // scan de dossier.
    if !head.starts_with(b"SQLite format 3\0") {
        return None;
    }
    let dbh = db::ReadOnlyDb::open(path).ok()?;
    let t = dbh.table_names();
    let has = |n: &str| t.contains(n);
    if has("moz_places") {
        Some("firefox_places")
    } else if has("moz_cookies") {
        Some("firefox_cookies")
    } else if has("moz_formhistory") {
        Some("firefox_formhistory")
    } else if has("moz_icons") && has("moz_pages_w_icons") {
        Some("firefox_favicons")
    } else if has("favicons") && has("icon_mapping") {
        Some("chromium_favicons")
    } else if has("history_items") && has("history_visits") {
        Some("safari_history")
    } else if has("omni_box_shortcuts") {
        Some("chromium_shortcuts")
    } else if has("top_sites") {
        Some("chromium_topsites")
    } else if has("urls") && has("visits") {
        Some("chromium_history")
    } else if has("logins") {
        Some("chromium_logins")
    } else if has("autofill") {
        Some("chromium_webdata")
    } else if has("cookies") {
        Some("chromium_cookies")
    } else {
        None
    }
}

const CHROMIUM: &str = "Chromium (Chrome/Edge/Brave/Opera/Vivaldi)";

/// Précise le vrai navigateur Chromium d'après le chemin (dossier éditeur).
fn refine_browser(browser: &str, path: &str) -> String {
    if browser == CHROMIUM {
        let p = path.to_ascii_lowercase().replace('\\', "/");
        if p.contains("/google/chrome") {
            return "Google Chrome".into();
        }
        if p.contains("/microsoft/edge") {
            return "Microsoft Edge".into();
        }
        if p.contains("/bravesoftware") || p.contains("/brave-browser") {
            return "Brave".into();
        }
        if p.contains("/opera software") || p.contains("/opera stable") || p.contains("/opera gx") {
            return "Opera".into();
        }
        if p.contains("/vivaldi") {
            return "Vivaldi".into();
        }
        if p.contains("/chromium") {
            return "Chromium".into();
        }
    }
    browser.to_string()
}

fn labels(kind: &str) -> (&'static str, String) {
    let browser = match kind {
        k if k.starts_with("chromium_") => CHROMIUM,
        k if k.starts_with("firefox_") => "Firefox",
        k if k.starts_with("safari_") => "Safari",
        "ie_webcache" => "Internet Explorer / Edge Legacy",
        _ => "?",
    };
    let kindlbl = match kind {
        "chromium_history" => "History",
        "chromium_cookies" => "Cookies",
        "chromium_logins" => "Login Data",
        "chromium_webdata" => "Web Data",
        "chromium_bookmarks" => "Bookmarks",
        "chromium_shortcuts" => "Shortcuts",
        "chromium_topsites" => "Top Sites",
        "chromium_favicons" => "Favicons",
        "chromium_preferences" => "Preferences",
        "chromium_session" => "Session (SNSS)",
        "firefox_places" => "places.sqlite",
        "firefox_cookies" => "cookies.sqlite",
        "firefox_formhistory" => "formhistory.sqlite",
        "firefox_favicons" => "favicons.sqlite",
        "firefox_logins" => "logins.json",
        "firefox_session" => "sessionstore.jsonlz4",
        "safari_history" => "History.db",
        "safari_downloads" => "Downloads.plist",
        "safari_bookmarks" => "Bookmarks.plist",
        "ie_webcache" => "WebCacheV01.dat (ESE)",
        other => other,
    };
    (browser, kindlbl.to_string())
}

pub fn load_artifact(path_str: &str, evidence: Option<String>) -> Result<Artifact, String> {
    let path = Path::new(path_str);
    let kind = detect(path)
        .ok_or_else(|| format!("Type de base non reconnu : {}", path.display()))?;
    let (browser, kindlbl) = labels(kind);
    let browser = refine_browser(browser, path_str);
    let sha = sha256_file(path)?;
    let evidence = evidence.unwrap_or_else(|| {
        path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string()
    });

    let categories: Vec<Category> = match kind {
        "chromium_bookmarks" => parsers::chromium_bookmarks(path)?,
        "chromium_preferences" => parsers::chromium_preferences(path)?,
        "chromium_session" => sessions::chromium_session(path)?,
        "firefox_logins" => parsers::firefox_logins(path)?,
        "firefox_session" => sessions::firefox_session(path)?,
        "safari_downloads" => parsers::safari_downloads(path)?,
        "safari_bookmarks" => parsers::safari_bookmarks(path)?,
        "ie_webcache" => ese::parse_ie_webcache(path)?,
        _ => {
            let dbh = db::ReadOnlyDb::open(path)?;
            match kind {
                "chromium_history" => parsers::chromium_history(&dbh)?,
                "chromium_cookies" => parsers::chromium_cookies(&dbh)?,
                "chromium_webdata" => parsers::chromium_webdata(&dbh)?,
                "chromium_logins" => parsers::chromium_logins(&dbh)?,
                "chromium_shortcuts" => parsers::chromium_shortcuts(&dbh)?,
                "chromium_topsites" => parsers::chromium_topsites(&dbh)?,
                "chromium_favicons" => parsers::chromium_favicons(&dbh)?,
                "firefox_places" => parsers::firefox_places(&dbh)?,
                "firefox_cookies" => parsers::firefox_cookies(&dbh)?,
                "firefox_formhistory" => parsers::firefox_formhistory(&dbh)?,
                "firefox_favicons" => parsers::firefox_favicons(&dbh)?,
                "safari_history" => parsers::safari_history(&dbh)?,
                _ => vec![],
            }
        }
    };

    let categories: Vec<Category> = categories.into_iter().filter(|c| !c.rows.is_empty()).collect();
    let mut notes = Vec::new();
    if categories.is_empty() {
        notes.push("Aucune donnee exploitable trouvee.".to_string());
    }

    Ok(Artifact {
        evidence,
        path: path_str.to_string(),
        sha256: sha,
        browser: browser.to_string(),
        kind: kindlbl,
        categories,
        notes,
    })
}

pub fn scan_folder(folder: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(folder)];
    while let Some(d) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&d) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if detect(&p).is_some() {
                    found.push(p.to_string_lossy().to_string());
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod browser_tests {
    use super::{refine_browser, CHROMIUM};
    #[test]
    fn refine_real_browser() {
        assert_eq!(refine_browser(CHROMIUM, "C:/Users/x/AppData/Local/Google/Chrome/User Data/Default/History"), "Google Chrome");
        assert_eq!(refine_browser(CHROMIUM, "C:\\Users\\x\\AppData\\Local\\Microsoft\\Edge\\User Data\\Default\\History"), "Microsoft Edge");
        assert_eq!(refine_browser(CHROMIUM, "/x/BraveSoftware/Brave-Browser/User Data/Default/History"), "Brave");
        assert_eq!(refine_browser(CHROMIUM, "D:/dump/Vivaldi/User Data/Default/History"), "Vivaldi");
        assert_eq!(refine_browser(CHROMIUM, "E:/extraction/History"), CHROMIUM); // inconnu -> generique
        assert_eq!(refine_browser("Firefox", "/x/places.sqlite"), "Firefox");
    }
}
