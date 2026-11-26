//! Récupération d'enregistrements supprimés par carving SQLite.
//!
//! Les navigateurs stockent l'historique, les téléchargements et les cookies
//! dans des bases SQLite. Lorsqu'une ligne est supprimée, SQLite ne l'efface
//! pas : la cellule rejoint la liste des blocs libres (`freeblock`) de sa page,
//! ou la page entière rejoint la *freelist* de la base. Tant que l'espace n'est
//! pas réécrit (ou purgé par `VACUUM`), l'enregistrement reste lisible.
//!
//! Ce module scanne l'espace libre — pages de la freelist, blocs libres
//! intra-page, zone non allouée — ainsi que les versions anciennes de pages
//! présentes dans le journal `-wal`, et tente de reconstituer les
//! enregistrements contenant une URL. Lecture seule, sur une copie.

use crate::model::{Category, Cell, Row};
use crate::time;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const COLUMNS: &[&str] = &["Zone", "URL", "Titre", "Horodatage", "Source"];

/// Taille maximale d'un fichier soumis au carving (octets). Au-delà, on
/// renonce pour ne pas saturer la mémoire.
const MAX_FILE: u64 = 1_024 * 1_024 * 1_024; // 1 Gio

/// Nombre maximal d'enregistrements récupérés (garde-fou anti-emballement).
const MAX_RECOVERED: usize = 200_000;

/// Fenêtre d'horodatages plausibles pour éviter de prendre un entier
/// quelconque pour une date : 2005-01-01 à 2035-01-01 (ms Unix).
const TS_MIN_MS: i64 = 1_104_537_600_000;
const TS_MAX_MS: i64 = 2_051_222_400_000;

#[derive(Debug)]
enum Field {
    Null,
    Int(i64),
    /// Valeur flottante : décodée pour consommer les octets, non exploitée.
    Float(#[allow(dead_code)] f64),
    Text(String),
    Blob,
}

/// Point d'entrée : renvoie une catégorie « Enregistrements supprimés
/// (récupérés) » si des traces exploitables sont retrouvées, sinon rien.
pub fn carve_sqlite(path: &Path) -> Vec<Category> {
    let mut out: Vec<Row> = Vec::new();
    let mut seen: HashSet<(String, String, i64)> = HashSet::new();

    if let Ok(bytes) = read_capped(path) {
        if let Some(ps) = page_size(&bytes) {
            carve_main(&bytes, ps, &mut out, &mut seen);
        }
    }

    // Journal WAL (versions antérieures des pages).
    let wal = with_suffix(path, "-wal");
    if let Ok(wbytes) = read_capped(&wal) {
        carve_wal(&wbytes, &mut out, &mut seen);
    }

    if out.is_empty() {
        return vec![];
    }
    vec![Category::new(
        "Enregistrements supprimés (récupérés)",
        COLUMNS,
        out,
    )]
}

fn read_capped(path: &Path) -> std::io::Result<Vec<u8>> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_FILE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "fichier trop volumineux pour le carving",
        ));
    }
    std::fs::read(path)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

/// Taille de page lue dans l'en-tête SQLite, ou None si ce n'est pas une base.
fn page_size(b: &[u8]) -> Option<usize> {
    if b.len() < 100 || &b[0..16] != b"SQLite format 3\0" {
        return None;
    }
    let ps = be16(b, 16);
    let ps = if ps == 1 { 65536 } else { ps };
    if ps >= 512 && ps.is_power_of_two() && ps <= 65536 {
        Some(ps)
    } else {
        None
    }
}

fn be16(b: &[u8], off: usize) -> usize {
    match (b.get(off), b.get(off + 1)) {
        (Some(&h), Some(&l)) => ((h as usize) << 8) | l as usize,
        _ => 0,
    }
}

fn be32(b: &[u8], off: usize) -> usize {
    (0..4).fold(0usize, |acc, i| (acc << 8) | *b.get(off + i).unwrap_or(&0) as usize)
}

// --- Base principale -------------------------------------------------------

fn carve_main(b: &[u8], ps: usize, out: &mut Vec<Row>, seen: &mut HashSet<(String, String, i64)>) {
    let npages = b.len() / ps;
    let free = freelist_pages(b, ps, npages);

    for i in 1..=npages {
        if out.len() >= MAX_RECOVERED {
            return;
        }
        let base = (i - 1) * ps;
        let page = match b.get(base..base + ps) {
            Some(p) => p,
            None => break,
        };
        if free.contains(&i) {
            // Page entièrement libre : tout le contenu est carvable.
            scan_region(page, 4, ps, &format!("Page libre #{i}"), out, seen);
            continue;
        }
        let hoff = if i == 1 { 100 } else { 0 };
        scan_page_free(page, hoff, &format!("p#{i}"), out, seen);
    }
}

/// Numéros (1-based) des pages de la freelist.
fn freelist_pages(b: &[u8], ps: usize, npages: usize) -> HashSet<usize> {
    let mut set = HashSet::new();
    let mut trunk = be32(b, 32); // première page trunk de la freelist
    let mut guard = 0;
    while trunk != 0 && trunk <= npages && guard < npages + 1 {
        set.insert(trunk);
        let base = (trunk - 1) * ps;
        let next = be32(b, base);
        let leaves = be32(b, base + 4);
        for k in 0..leaves {
            let pno = be32(b, base + 8 + k * 4);
            if pno >= 1 && pno <= npages {
                set.insert(pno);
            }
            if base + 8 + k * 4 > b.len() {
                break;
            }
        }
        if next == trunk {
            break;
        }
        trunk = next;
        guard += 1;
    }
    set
}

/// Scanne l'espace libre d'une page b-tree « table leaf » (type 0x0D) :
/// zone non allouée + chaîne de blocs libres. Les cellules vivantes sont
/// ignorées (on ne récupère que le supprimé).
fn scan_page_free(
    page: &[u8],
    hoff: usize,
    tag: &str,
    out: &mut Vec<Row>,
    seen: &mut HashSet<(String, String, i64)>,
) {
    if hoff + 8 > page.len() || page.get(hoff) != Some(&0x0D) {
        return; // seules les pages de type « table leaf » portent des lignes
    }
    let cellcount = be16(page, hoff + 3);
    let mut content = be16(page, hoff + 5);
    if content == 0 {
        content = 65536;
    }
    let cellptr_end = hoff + 8 + 2 * cellcount;

    // Zone non allouée (entre le tableau de pointeurs et le contenu).
    if content <= page.len() && cellptr_end <= content {
        scan_region(page, cellptr_end, content, &format!("Non alloué {tag}"), out, seen);
    }

    // Chaîne de blocs libres.
    let mut fb = be16(page, hoff + 1);
    let mut guard = 0;
    while fb != 0 && fb + 4 <= page.len() && guard < 4096 {
        let size = be16(page, fb + 2);
        let end = (fb + size).min(page.len());
        scan_region(page, fb, end, &format!("Freeblock {tag}"), out, seen);
        let next = be16(page, fb);
        if next <= fb {
            break; // chaîne incohérente : on s'arrête
        }
        fb = next;
        guard += 1;
    }
}

/// Fait glisser une fenêtre sur `buf[start..end]` et tente d'y lire des
/// enregistrements SQLite contenant une URL.
fn scan_region(
    buf: &[u8],
    start: usize,
    end: usize,
    zone: &str,
    out: &mut Vec<Row>,
    seen: &mut HashSet<(String, String, i64)>,
) {
    let end = end.min(buf.len());
    if start >= end {
        return;
    }
    let mut pos = start;
    while pos + 3 < end {
        if out.len() >= MAX_RECOVERED {
            return;
        }
        match parse_record(&buf[pos..end]) {
            Some((total, fields)) => {
                if let Some(row) = row_from_fields(&fields, zone) {
                    let key = row_key(&row);
                    if seen.insert(key) {
                        out.push(row);
                    }
                }
                pos += total.max(1);
            }
            None => pos += 1,
        }
    }
}

// --- Journal WAL -----------------------------------------------------------

fn carve_wal(b: &[u8], out: &mut Vec<Row>, seen: &mut HashSet<(String, String, i64)>) {
    if b.len() < 32 {
        return;
    }
    let magic = be32(b, 0);
    if magic != 0x377f_0682 && magic != 0x377f_0683 {
        return;
    }
    let ps = {
        let p = be32(b, 8);
        if p == 1 {
            65536
        } else {
            p
        }
    };
    if ps < 512 || !ps.is_power_of_two() || ps > 65536 {
        return;
    }
    let frame = 24 + ps; // en-tête de frame + page
    let mut off = 32;
    let mut idx = 0;
    while off + frame <= b.len() {
        if out.len() >= MAX_RECOVERED {
            return;
        }
        let pageno = be32(b, off); // numéro de page porté par la frame
        let page = &b[off + 24..off + 24 + ps];
        let hoff = if pageno == 1 { 100 } else { 0 };
        scan_page_free(page, hoff, &format!("WAL frame #{idx}"), out, seen);
        off += frame;
        idx += 1;
    }
}

// --- Décodage d'un enregistrement SQLite -----------------------------------

/// Tente de lire un enregistrement (en-tête + corps) au début de `buf`.
/// Renvoie (longueur totale consommée, champs décodés).
fn parse_record(buf: &[u8]) -> Option<(usize, Vec<Field>)> {
    let (hlen, k) = varint(buf, 0)?;
    let hlen = hlen as usize;
    if hlen < 2 || hlen > buf.len() || hlen < k || hlen > 2048 {
        return None;
    }
    // Types sériels, jusqu'à épuiser exactement l'en-tête.
    let mut types: Vec<i64> = Vec::new();
    let mut p = k;
    while p < hlen {
        let (st, kk) = varint(buf, p)?;
        types.push(st);
        p += kk;
    }
    if p != hlen || types.is_empty() || types.len() > 64 {
        return None;
    }
    // Tailles de corps.
    let mut body = hlen;
    let mut fields: Vec<Field> = Vec::with_capacity(types.len());
    for st in &types {
        let size = serial_size(*st)?;
        if body + size > buf.len() {
            return None;
        }
        let slice = &buf[body..body + size];
        fields.push(decode(*st, slice));
        body += size;
    }
    Some((body, fields))
}

fn serial_size(st: i64) -> Option<usize> {
    let s = match st {
        0 | 8 | 9 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 6,
        6 | 7 => 8,
        10 | 11 => return None, // réservés : signe d'un faux positif
        n if n >= 12 => {
            if n % 2 == 0 {
                ((n - 12) / 2) as usize
            } else {
                ((n - 13) / 2) as usize
            }
        }
        _ => return None,
    };
    Some(s)
}

fn decode(st: i64, b: &[u8]) -> Field {
    match st {
        0 => Field::Null,
        1 => Field::Int(b[0] as i8 as i64),
        2 => Field::Int(i16::from_be_bytes([b[0], b[1]]) as i64),
        3 => {
            let v = ((b[0] as i64) << 16) | ((b[1] as i64) << 8) | b[2] as i64;
            Field::Int(if v & 0x80_0000 != 0 { v - 0x100_0000 } else { v })
        }
        4 => Field::Int(i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as i64),
        5 => {
            let mut a = [0u8; 8];
            a[2..8].copy_from_slice(&b[0..6]);
            let mut v = i64::from_be_bytes(a);
            if b[0] & 0x80 != 0 {
                v -= 1 << 48;
            }
            Field::Int(v)
        }
        6 => Field::Int(i64::from_be_bytes(b[0..8].try_into().unwrap())),
        7 => Field::Float(f64::from_be_bytes(b[0..8].try_into().unwrap())),
        8 => Field::Int(0),
        9 => Field::Int(1),
        n if n >= 13 && n % 2 == 1 => match std::str::from_utf8(b) {
            Ok(s) => Field::Text(s.to_string()),
            Err(_) => Field::Blob,
        },
        _ => Field::Blob,
    }
}

/// Varint SQLite (1 à 9 octets, big-endian 7 bits ; 9e octet = 8 bits).
fn varint(buf: &[u8], pos: usize) -> Option<(i64, usize)> {
    let mut res: i64 = 0;
    let mut i = 0;
    while i < 9 {
        let byte = *buf.get(pos + i)?;
        if i == 8 {
            res = (res << 8) | byte as i64;
            return Some((res, 9));
        }
        res = (res << 7) | (byte & 0x7f) as i64;
        if byte & 0x80 == 0 {
            return Some((res, i + 1));
        }
        i += 1;
    }
    Some((res, 9))
}

// --- Construction d'une ligne récupérée ------------------------------------

/// Transforme des champs décodés en ligne, à condition qu'un champ texte
/// ressemble à une URL (présence de « :// »). C'est le filtre qui écarte la
/// grande majorité des faux positifs.
fn row_from_fields(fields: &[Field], zone: &str) -> Option<Row> {
    let mut url: Option<&str> = None;
    let mut url_idx = usize::MAX;
    for (i, f) in fields.iter().enumerate() {
        if let Field::Text(s) = f {
            if s.len() >= 8 && s.len() <= 4096 && s.contains("://") && looks_printable(s) {
                url = Some(s);
                url_idx = i;
                break;
            }
        }
    }
    let url = url?;

    // Titre = le plus long autre champ texte imprimable raisonnable.
    let mut title = String::new();
    for (i, f) in fields.iter().enumerate() {
        if i == url_idx {
            continue;
        }
        if let Field::Text(s) = f {
            if s.len() <= 512 && looks_printable(s) && s.len() > title.len() {
                title = s.clone();
            }
        }
    }

    // Horodatage = premier entier convertible dans une fenêtre plausible.
    let mut ts: Option<i64> = None;
    for f in fields {
        if let Field::Int(v) = f {
            if let Some(ms) = guess_timestamp(*v) {
                ts = Some(ms);
                break;
            }
        }
    }

    let mut row = Row::new();
    row.set("Zone", Cell::text(zone));
    row.set("URL", Cell::text(url));
    row.set("Titre", Cell::text(title.clone()));
    row.set("Horodatage", Cell::opt_time(ts));
    row.set("Source", Cell::text("Carving (supprimé)"));
    row.raw("URL", url);
    if !title.is_empty() {
        row.raw("Titre", title);
    }
    row.raw("Zone", zone);
    Some(row)
}

fn looks_printable(s: &str) -> bool {
    // Rejette les chaînes truffées de caractères de contrôle (faux positifs).
    let ctrl = s
        .chars()
        .filter(|c| c.is_control() && *c != '\t' && *c != '\n' && *c != '\r')
        .count();
    ctrl == 0
}

fn guess_timestamp(v: i64) -> Option<i64> {
    if v <= 0 {
        return None;
    }
    for ms in [
        time::from_webkit(v),
        time::from_prtime(v),
        time::from_unix_ms(v),
        time::from_unix_s(v),
    ]
    .into_iter()
    .flatten()
    {
        if (TS_MIN_MS..=TS_MAX_MS).contains(&ms) {
            return Some(ms);
        }
    }
    None
}

fn row_key(row: &Row) -> (String, String, i64) {
    let g = |c: &str| row.get(c).map(|x| x.as_search()).unwrap_or_default();
    let ts = row.get("Horodatage").and_then(|c| c.time_ms()).unwrap_or(0);
    (g("URL"), g("Titre"), ts)
}
