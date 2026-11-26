//! Sessions : carving SNSS (Chromium) et sessionstore mozLz4 (Firefox).

use crate::model::{Category, Cell, Row};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;

type R<T> = Result<T, String>;

fn url_re() -> Regex {
    Regex::new(r#"https?://[^\s\x00-\x1f"'<>\\]{3,2000}"#).unwrap()
}

fn clean_token(u: &str) -> String {
    let mut out = String::new();
    for ch in u.chars() {
        let o = ch as u32;
        if o < 0x21 || o > 0x7e || "\"'<>\\".contains(ch) {
            break;
        }
        out.push(ch);
    }
    out
}

/// Extrait les URLs par carving (ASCII + UTF-16LE).
pub fn carve_urls(data: &[u8]) -> BTreeSet<String> {
    let re = url_re();
    let mut found = BTreeSet::new();

    let latin1: String = data.iter().map(|&b| b as char).collect();
    for m in re.find_iter(&latin1) {
        let u = clean_token(m.as_str());
        if u.len() >= 10 {
            found.insert(u);
        }
    }
    let mut u16v: Vec<u16> = Vec::with_capacity(data.len() / 2);
    let mut i = 0;
    while i + 1 < data.len() {
        u16v.push(u16::from_le_bytes([data[i], data[i + 1]]));
        i += 2;
    }
    let utf16 = String::from_utf16_lossy(&u16v);
    for m in re.find_iter(&utf16) {
        let u = clean_token(m.as_str());
        if u.len() >= 10 {
            found.insert(u);
        }
    }
    found
}

pub fn chromium_session(path: &Path) -> R<Vec<Category>> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for u in carve_urls(&data) {
        let mut rr = Row::new();
        rr.set("URL", Cell::text(u));
        out.push(rr);
    }
    Ok(vec![Category::new("Session / onglets (URLs reconstruites)", &["URL"], out)])
}

pub fn firefox_session(path: &Path) -> R<Vec<Category>> {
    let raw = std::fs::read(path).map_err(|e| e.to_string())?;
    if raw.len() < 12 || (&raw[0..8] != b"mozLz4a\0" && &raw[0..8] != b"mozLz40\0") {
        return Err("En-tete mozLz4 absent".into());
    }
    let size = u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]) as usize;
    let data = lz4_flex::block::decompress(&raw[12..], size)
        .map_err(|e| format!("decompression lz4 : {e}"))?;
    let json: serde_json::Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(windows) = json.get("windows").and_then(|v| v.as_array()) {
        for win in windows {
            if let Some(tabs) = win.get("tabs").and_then(|v| v.as_array()) {
                for tab in tabs {
                    let entries = tab.get("entries").and_then(|v| v.as_array());
                    if let Some(entries) = entries {
                        let idx = tab
                            .get("index")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(entries.len() as i64)
                            - 1;
                        if idx >= 0 && (idx as usize) < entries.len() {
                            let e = &entries[idx as usize];
                            let mut rr = Row::new();
                            rr.set("URL", Cell::opt_text(e.get("url").and_then(|v| v.as_str()).map(String::from)));
                            rr.set("Titre", Cell::opt_text(e.get("title").and_then(|v| v.as_str()).map(String::from)));
                            out.push(rr);
                        }
                    }
                }
            }
        }
    }
    Ok(vec![Category::new("Onglets ouverts (session)", &["URL", "Titre"], out)])
}
