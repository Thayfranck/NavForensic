//! NavForensics — coeur d'analyse forensique des bases de navigateurs.
//!
//! Lecture seule sur copie (sidecars -wal/-shm inclus), empreinte SHA-256,
//! conversions d'horodatage par navigateur, aucun secret dechiffre.

pub mod db;
pub mod ese;
pub mod model;
pub mod carve;
pub mod crypto;
pub mod e01;
pub mod firefox_login;
pub mod ioc;
pub mod parsers;
pub mod profiles;
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
pub use crypto::{build_crypto, CRYPTO_COLUMNS};
pub use firefox_login::{decrypt_profile, FxLogin};
pub use ioc::{build_ioc_matches, parse_iocs, Ioc, IOC_COLUMNS};
pub use profiles::{build_extensions, build_sync_accounts, EXT_COLUMNS, SYNC_COLUMNS};
pub use views::{
    b64_decode, build_domain_summary, build_downloads, build_ga_cookies, build_geolocation,
    build_geopoints, build_heatmap, build_histogram, build_identity, build_online_services,
    build_recovered, build_search_terms, build_stats, build_timeline, build_vpn, carve_cache_html,
    carve_cache_media, carve_cache_urls, search_all, search_keywords, HtmlItem, HtmlResource,
    MediaItem, DOMAIN_COLUMNS, DOWNLOADS_COLUMNS, GA_COLUMNS, GEOPOINT_COLUMNS, GEO_COLUMNS,
    HEATMAP_COLUMNS, HISTO_COLUMNS, HTML_MAX_ITEMS, IDENTITY_COLUMNS, MEDIA_MAX_BYTES,
    MEDIA_MAX_ITEMS, ONLINE_COLUMNS, RECOVERED_COLUMNS, SEARCH_COLUMNS, SEARCH_KW_COLUMNS,
    SEARCH_TERMS_COLUMNS, STATS_COLUMNS, TAG_COLUMNS, TIMELINE_COLUMNS, TIMELINE_CORE,
    TIMELINE_NOISY, VPN_COLUMNS,
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

/// Signatures de chemin (desktop + conteneurs/paquets mobiles) → navigateur.
/// Reconnaît les éditeurs Chromium au-delà des cinq de bureau : Samsung
/// Internet, UC Browser, Arc, Yandex, Opera GX, ainsi que les paquets Android
/// (`com.*`) et les conteneurs iOS. Les signatures les plus spécifiques
/// passent avant les plus génériques.
const BROWSER_PATH_SIGS: &[(&[&str], &str)] = &[
    // Google Chrome (desktop, Android com.android.chrome, iOS com.google.chrome.ios)
    (&["/google/chrome", "com.android.chrome", "com.google.chrome", "chrome.ios"], "Google Chrome"),
    // Microsoft Edge (desktop ; Android com.microsoft.emmx ; iOS com.microsoft.msedge)
    (&["/microsoft/edge", "com.microsoft.emmx", "com.microsoft.msedge"], "Microsoft Edge"),
    // Brave (desktop ; Android com.brave.browser ; iOS com.brave.ios.browser)
    (&["/bravesoftware", "/brave-browser", "com.brave"], "Brave"),
    // Opera GX (avant Opera générique)
    (&["/opera gx", "com.opera.gx"], "Opera GX"),
    // Opera (desktop ; Android com.opera.browser ; Opera Mini com.opera.mini.native)
    (&["/opera software", "/opera stable", "com.opera.browser", "com.opera.mini", "/com.opera"], "Opera"),
    // Vivaldi (desktop ; Android com.vivaldi.browser)
    (&["/vivaldi", "com.vivaldi"], "Vivaldi"),
    // Samsung Internet (Android com.sec.android.app.sbrowser)
    (&["com.sec.android.app.sbrowser", "/sbrowser", "samsung internet"], "Samsung Internet"),
    // UC Browser (Android com.ucmobile / com.uc.browser)
    (&["com.ucmobile", "com.uc.browser", "ucbrowser", "/ucmobile"], "UC Browser"),
    // Yandex (desktop /yandex ; Android com.yandex.browser)
    (&["yandexbrowser", "com.yandex.browser", "/yandex"], "Yandex"),
    // Arc (The Browser Company ; desktop + iOS company.thebrowser.*)
    (&["/thebrowsercompany", "company.thebrowser", "/arc/user data", "/arc.app"], "Arc"),
    // Chromium générique (en dernier)
    (&["/chromium", "org.chromium"], "Chromium"),
];

/// Précise le vrai navigateur d'après le chemin (dossier éditeur desktop ou
/// paquet/conteneur mobile). S'applique quand la détection par contenu a rendu
/// l'étiquette Chromium générique.
fn refine_browser(browser: &str, path: &str) -> String {
    if browser == CHROMIUM {
        let p = path.to_ascii_lowercase().replace('\\', "/");
        for (sigs, name) in BROWSER_PATH_SIGS {
            if sigs.iter().any(|s| p.contains(s)) {
                return (*name).to_string();
            }
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

    let mut categories: Vec<Category> =
        categories.into_iter().filter(|c| !c.rows.is_empty()).collect();

    // Carving des enregistrements supprimes (freelist, freeblocks, zone non
    // allouee, versions anciennes du -wal) pour les bases SQLite porteuses d'URL.
    if carve_target(kind) {
        categories.extend(carve::carve_sqlite(path));
    }

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

/// Bases SQLite sur lesquelles tenter le carving des enregistrements supprimes.
fn carve_target(kind: &str) -> bool {
    matches!(
        kind,
        "chromium_history"
            | "chromium_cookies"
            | "firefox_places"
            | "firefox_cookies"
            | "safari_history"
    )
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
        // mobiles + éditeurs supplémentaires reconnus par signature de chemin
        assert_eq!(refine_browser(CHROMIUM, "/ffs/data/data/com.sec.android.app.sbrowser/app_sbrowser/Default/History"), "Samsung Internet");
        assert_eq!(refine_browser(CHROMIUM, "/ffs/data/data/com.android.chrome/app_chrome/Default/History"), "Google Chrome");
        assert_eq!(refine_browser(CHROMIUM, "/ffs/data/data/com.microsoft.emmx/app_chrome/Default/History"), "Microsoft Edge");
        assert_eq!(refine_browser(CHROMIUM, "/ffs/data/data/com.UCMobile.intl/History".to_ascii_lowercase().as_str()), "UC Browser");
        assert_eq!(refine_browser(CHROMIUM, "/Containers/Data/Application/ABC/Library/Application Support/company.thebrowser.arc/History"), "Arc");
        assert_eq!(refine_browser(CHROMIUM, "C:/Users/x/AppData/Local/Yandex/YandexBrowser/User Data/Default/History"), "Yandex");
        assert_eq!(refine_browser(CHROMIUM, "%APPDATA%/Opera Software/Opera GX Stable/History"), "Opera GX");
        assert_eq!(refine_browser(CHROMIUM, "E:/extraction/History"), CHROMIUM); // inconnu -> generique
        assert_eq!(refine_browser("Firefox", "/x/places.sqlite"), "Firefox");
    }
}
