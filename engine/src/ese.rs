//! Lecteur ESE pour `WebCacheV01.dat` (Internet Explorer / Edge Legacy).
//!
//! IE (Trident) et Edge Legacy (EdgeHTML/Spartan) stockent l'historique, les
//! cookies, le cache et le stockage DOM dans une base ESE (JET Blue), pas en
//! SQLite. Ce module pose la couche *schema* WebCache (table `Containers` puis
//! chaque `Container_<id>`) au-dessus du lecteur ESE pur Rust `ese-core`.
//!
//! Schema : table `Containers` (ContainerId, Name, Directory) ; pour chaque
//! conteneur de navigation, `Container_<id>` (Url, Filename, AccessCount, et les
//! FILETIME CreationTime / ExpiryTime / ModifiedTime / AccessedTime).
//! Cartographie adaptee du crate `browser-forensic-webcache` (Apache-2.0).
//!
//! Lecture seule : la base est copiee dans un dossier temporaire avant ouverture
//! (le fichier d'origine n'est jamais modifie ; sur machine live il est de toute
//! facon souvent verrouille, la copie echoue alors proprement).

use crate::model::{Category, Cell, Row};
use crate::time::from_filetime;
use ese_core::{decode_ese_record, decode_record, ColumnDef, EseDatabase, EseValue};
use std::path::Path;

type R<T> = Result<T, String>;

const COLUMNS: &[&str] = &[
    "URL", "Utilisateur", "Fichier", "Acces", "Derniere visite", "Modifie", "Creation",
    "Expiration", "Conteneur",
];

/// L'historique IE stocke l'URL sous la forme "Visited: <utilisateur>@<url>".
/// Sépare l'utilisateur Windows de l'URL réelle.
fn split_ie_user(url: Option<String>) -> (Option<String>, Option<String>) {
    let u = match url {
        Some(u) => u,
        None => return (None, None),
    };
    for pref in ["Visited: ", "iedownload:"] {
        if let Some(rest) = u.strip_prefix(pref) {
            if let Some(at) = rest.find('@') {
                let (user, real) = rest.split_at(at);
                let real = &real[1..];
                if real.contains("://") {
                    return (Some(user.trim().to_string()), Some(real.trim().to_string()));
                }
            }
            return (None, Some(rest.trim().to_string()));
        }
    }
    (None, Some(u))
}

fn col<'a>(cols: &'a [(String, EseValue)], name: &str) -> Option<&'a EseValue> {
    cols.iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

fn text(v: &EseValue) -> Option<String> {
    let s = match v {
        EseValue::Text(s) => s.clone(),
        EseValue::Binary(b) => String::from_utf8_lossy(b).into_owned(),
        _ => return None,
    };
    let s = s.trim_end_matches('\0').trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn filetime_ms(v: &EseValue) -> Option<i64> {
    let ticks = match v {
        EseValue::I64(n) => *n,
        EseValue::U64(n) => *n as i64,
        _ => return None,
    };
    from_filetime(ticks)
}

fn count_val(v: &EseValue) -> Option<i64> {
    match v {
        EseValue::U8(n) => Some(i64::from(*n)),
        EseValue::U16(n) => Some(i64::from(*n)),
        EseValue::U32(n) => Some(i64::from(*n)),
        EseValue::U64(n) => i64::try_from(*n).ok(),
        EseValue::I16(n) => Some(i64::from(*n)),
        EseValue::I32(n) => Some(i64::from(*n)),
        EseValue::I64(n) => Some(*n),
        _ => None,
    }
}

fn decode(data: &[u8], cols: &[ColumnDef], extended: bool) -> Option<Vec<(String, EseValue)>> {
    if extended {
        decode_ese_record(data, cols, true).ok()
    } else {
        decode_record(data, cols).ok()
    }
}

/// Catégorie-cible d'un conteneur d'après son nom (None = conteneur technique).
fn classify(name: &str) -> Option<&'static str> {
    if name.starts_with("MSHist") {
        return Some("Historique (IE/Edge Legacy)");
    }
    match name.to_ascii_lowercase().as_str() {
        "history" => Some("Historique (IE/Edge Legacy)"),
        "cookies" => Some("Cookies"),
        "content" => Some("URLs en cache (contenu charge)"),
        "iedownload" => Some("Telechargements"),
        "domstore" => Some("Stockage DOM (IE/Edge)"),
        _ => None,
    }
}

fn id_of(v: &EseValue) -> Option<i64> {
    match v {
        EseValue::I64(n) => Some(*n),
        EseValue::U64(n) => i64::try_from(*n).ok(),
        EseValue::I32(n) => Some(i64::from(*n)),
        EseValue::U32(n) => Some(i64::from(*n)),
        _ => None,
    }
}

/// Lit un `WebCacheV01.dat` (base ESE) et renvoie des catégories NavForensics.
pub fn parse_ie_webcache(path: &Path) -> R<Vec<Category>> {
    // Copie temporaire (lecture seule sur copie).
    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let copy = tmp.path().join("WebCacheV01.dat");
    std::fs::copy(path, &copy).map_err(|e| {
        format!(
            "Impossible de copier la base ESE ({e}). Sur une machine active le fichier \
             WebCacheV01.dat est verrouille par Windows : fournir une copie hors ligne."
        )
    })?;

    let db = EseDatabase::open(&copy).map_err(|e| format!("Ouverture ESE : {e}"))?;
    let extended = db.is_extended_format();

    let ccols = db
        .table_columns("Containers")
        .map_err(|e| format!("Table Containers : {e}"))?;
    let ccur = db
        .table_records("Containers")
        .map_err(|e| format!("Table Containers : {e}"))?;

    let mut containers: Vec<(i64, String)> = Vec::new();
    for rec in ccur {
        let data = match rec {
            Ok((_, _, d)) => d,
            Err(_) => continue,
        };
        let cols = match decode(&data, &ccols, extended) {
            Some(c) => c,
            None => continue,
        };
        let id = col(&cols, "ContainerId").and_then(id_of);
        let name = col(&cols, "Name").and_then(text);
        if let (Some(id), Some(name)) = (id, name) {
            containers.push((id, name));
        }
    }

    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<&'static str, Vec<Row>> = BTreeMap::new();
    for (id, name) in containers {
        let cat = match classify(&name) {
            Some(c) => c,
            None => continue,
        };
        let table = format!("Container_{id}");
        let tcols = match db.table_columns(&table) {
            Ok(c) => c,
            Err(_) => continue, // table absente pour ce conteneur : on ignore
        };
        let cur = match db.table_records(&table) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for rec in cur {
            let data = match rec {
                Ok((_, _, d)) => d,
                Err(_) => continue,
            };
            let cols = match decode(&data, &tcols, extended) {
                Some(c) => c,
                None => continue,
            };
            let raw_url = col(&cols, "Url").and_then(text);
            let filename = col(&cols, "Filename").and_then(text);
            if raw_url.is_none() && filename.is_none() {
                continue;
            }
            let (user, url) = split_ie_user(raw_url);
            let mut rr = Row::new();
            rr.set("URL", Cell::opt_text(url));
            rr.set("Utilisateur", Cell::opt_text(user));
            rr.set("Fichier", Cell::opt_text(filename));
            rr.set("Acces", Cell::opt_int(col(&cols, "AccessCount").and_then(count_val)));
            rr.set(
                "Derniere visite",
                Cell::opt_time(col(&cols, "AccessedTime").and_then(filetime_ms)),
            );
            rr.set("Modifie", Cell::opt_time(col(&cols, "ModifiedTime").and_then(filetime_ms)));
            rr.set("Creation", Cell::opt_time(col(&cols, "CreationTime").and_then(filetime_ms)));
            rr.set("Expiration", Cell::opt_time(col(&cols, "ExpiryTime").and_then(filetime_ms)));
            rr.set("Conteneur", Cell::text(name.clone()));
            buckets.entry(cat).or_default().push(rr);
        }
    }

    let mut cats = Vec::new();
    for cat in [
        "Historique (IE/Edge Legacy)",
        "URLs en cache (contenu charge)",
        "Cookies",
        "Telechargements",
        "Stockage DOM (IE/Edge)",
    ] {
        if let Some(rows) = buckets.remove(cat) {
            if !rows.is_empty() {
                cats.push(Category::new(cat, COLUMNS, rows));
            }
        }
    }
    Ok(cats)
}
