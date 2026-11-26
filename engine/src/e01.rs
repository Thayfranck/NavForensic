//! Extraction des artefacts de navigateurs depuis une image forensique E01.
//!
//! Chaîne : image EWF/E01 (`ewf`) → table de partitions (MBR/GPT) → volume
//! NTFS (`ntfs-core`, lecture seule) → copie des fichiers de profil des
//! navigateurs vers un répertoire temporaire, qui sont ensuite chargés comme
//! des sources ordinaires. Prend aussi en charge une image brute (`.dd`,
//! `.001`, `.raw`).
//!
//! Note : le lecteur EWF et le lecteur NTFS sont des bibliothèques dédiées ;
//! l'arborescence des profils (Users/AppData/…) est parcourue de façon
//! défensive — un fichier absent est ignoré sans erreur.

use crate::model::Artifact;
use ewf::EwfReader;
use ntfs_core::fs::NtfsFs;
use ntfs_core::source::OffsetReader;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const SECTOR: u64 = 512;
const FILE_ATTR_DIR: u32 = 0x1000_0000;
/// Plafond par fichier extrait.
const MAX_FILE: usize = 2 * 1024 * 1024 * 1024;

type R<T> = Result<T, String>;

/// Source de données brute : une image E01 (EWF) ou une image disque brute.
enum Image {
    Ewf(PathBuf),
    Raw(PathBuf),
}

impl Image {
    fn detect(path: &Path) -> Image {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if ext.starts_with('e') || ext.starts_with("ex") || ext.starts_with('l') {
            Image::Ewf(path.to_path_buf())
        } else {
            Image::Raw(path.to_path_buf())
        }
    }

    /// Lit `buf.len()` octets à l'offset absolu `off` (pour le balayage des
    /// partitions). Ouvre une instance fraîche à chaque appel : simple et sûr.
    fn read_at(&self, off: u64, buf: &mut [u8]) -> R<()> {
        match self {
            Image::Ewf(p) => {
                let r = EwfReader::open(p).map_err(|e| e.to_string())?;
                r.read_at(buf, off).map_err(|e| e.to_string())?;
                Ok(())
            }
            Image::Raw(p) => {
                let mut f = std::fs::File::open(p).map_err(|e| e.to_string())?;
                f.seek(SeekFrom::Start(off)).map_err(|e| e.to_string())?;
                f.read_exact(buf).map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }
}

/// Lecteur Read+Seek sur une partition donnée (fenêtre dans l'image).
fn open_ntfs(img: &Image, base: u64, len: u64) -> R<NtfsFs<OffsetReaderAny>> {
    let reader = match img {
        Image::Ewf(p) => {
            let r = EwfReader::open(p).map_err(|e| e.to_string())?;
            OffsetReaderAny::Ewf(OffsetReader::new(r, base, len).map_err(|e| e.to_string())?)
        }
        Image::Raw(p) => {
            let f = std::fs::File::open(p).map_err(|e| e.to_string())?;
            OffsetReaderAny::Raw(OffsetReader::new(f, base, len).map_err(|e| e.to_string())?)
        }
    };
    NtfsFs::open(reader).map_err(|e| e.to_string())
}

/// Enveloppe les deux types de fenêtres de partition derrière Read+Seek.
enum OffsetReaderAny {
    Ewf(OffsetReader<EwfReader>),
    Raw(OffsetReader<std::fs::File>),
}

impl Read for OffsetReaderAny {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            OffsetReaderAny::Ewf(r) => r.read(buf),
            OffsetReaderAny::Raw(r) => r.read(buf),
        }
    }
}
impl Seek for OffsetReaderAny {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            OffsetReaderAny::Ewf(r) => r.seek(pos),
            OffsetReaderAny::Raw(r) => r.seek(pos),
        }
    }
}

fn le32(b: &[u8], o: usize) -> u64 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as u64
}
fn le64(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}

/// Repère les partitions NTFS (base en octets, longueur en octets).
fn ntfs_partitions(img: &Image) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut mbr = [0u8; 512];
    if img.read_at(0, &mut mbr).is_err() || mbr[510] != 0x55 || mbr[511] != 0xAA {
        // Pas de MBR : tenter un volume NTFS à l'offset 0 (image de partition).
        if is_ntfs(img, 0) {
            out.push((0, 0));
        }
        return out;
    }
    let mut gpt = false;
    for i in 0..4 {
        let e = 446 + i * 16;
        let ptype = mbr[e + 4];
        let start = le32(&mbr, e + 8);
        let size = le32(&mbr, e + 12);
        if ptype == 0xEE {
            gpt = true;
            break;
        }
        if size > 0 && is_ntfs(img, start * SECTOR) {
            out.push((start * SECTOR, size * SECTOR));
        }
    }
    if gpt {
        out.extend(gpt_partitions(img));
    }
    // À défaut, tenter l'offset 0 (image de partition unique).
    if out.is_empty() && is_ntfs(img, 0) {
        out.push((0, 0));
    }
    out
}

fn gpt_partitions(img: &Image) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut hdr = [0u8; 512];
    if img.read_at(SECTOR, &mut hdr).is_err() || &hdr[0..8] != b"EFI PART" {
        return out;
    }
    let entry_lba = le64(&hdr, 72);
    let num = le32(&hdr, 80).min(256) as usize;
    let esize = le32(&hdr, 84).max(128) as usize;
    let mut buf = vec![0u8; num * esize];
    if img.read_at(entry_lba * SECTOR, &mut buf).is_err() {
        return out;
    }
    for i in 0..num {
        let e = i * esize;
        if e + 48 > buf.len() {
            break;
        }
        let first = le64(&buf, e + 32);
        let last = le64(&buf, e + 40);
        if first == 0 && last == 0 {
            continue;
        }
        let base = first * SECTOR;
        if is_ntfs(img, base) {
            out.push((base, (last - first + 1) * SECTOR));
        }
    }
    out
}

fn is_ntfs(img: &Image, base: u64) -> bool {
    let mut b = [0u8; 16];
    img.read_at(base, &mut b).is_ok() && &b[3..11] == b"NTFS    "
}

// --- Parcours des profils --------------------------------------------------

/// Enfants d'un répertoire : (nom, est_répertoire). Noms Win32 dédoublonnés.
fn list_dir(fs: &NtfsFs<OffsetReaderAny>, path: &str) -> Vec<(String, bool)> {
    let rec = match fs.resolve_path(path) {
        Ok(n) => n,
        Err(_) => return vec![],
    };
    let data = match fs.read_record(rec) {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    let entries = match fs.directory_entries(&data) {
        Ok(e) => e,
        Err(_) => return vec![],
    };
    let mut map: std::collections::BTreeMap<String, (String, bool)> =
        std::collections::BTreeMap::new();
    for e in entries {
        if let Some(fnm) = e.file_name {
            let name = fnm.name;
            if name == "." || name.starts_with('$') {
                continue;
            }
            let is_dir = fnm.flags & FILE_ATTR_DIR != 0;
            let key = name.to_ascii_lowercase();
            // Garde le nom le plus long (Win32 plutôt que 8.3).
            map.entry(key)
                .and_modify(|v| {
                    if name.len() > v.0.len() {
                        *v = (name.clone(), is_dir);
                    }
                })
                .or_insert((name.clone(), is_dir));
        }
    }
    map.into_values().collect()
}

/// Copie un fichier NTFS vers `dest` s'il existe. Renvoie true si copié.
fn grab(fs: &NtfsFs<OffsetReaderAny>, path: &str, dest: &Path) -> bool {
    let data = match fs.read_file_capped(path, MAX_FILE) {
        Ok(d) if !d.is_empty() => d,
        _ => return false,
    };
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(dest, data).is_ok()
}

/// Fichiers SQLite d'un profil (avec leurs sidecars -wal/-shm).
const CHROMIUM_FILES: &[&str] = &[
    "History", "Cookies", "Web Data", "Login Data", "Bookmarks", "Favicons", "Top Sites",
    "Shortcuts", "Preferences", "Secure Preferences",
];
const FIREFOX_FILES: &[&str] = &[
    "places.sqlite",
    "cookies.sqlite",
    "formhistory.sqlite",
    "favicons.sqlite",
    "logins.json",
    "prefs.js",
    "signedInUser.json",
    "extensions.json",
];
/// (dossier éditeur, produit) sous AppData/Local pour les navigateurs Chromium
/// qui suivent le schéma `.../User Data/<profil>/`.
const CHROMIUM_VENDORS: &[(&str, &str)] = &[
    ("Google", "Chrome"),
    ("Microsoft", "Edge"),
    ("BraveSoftware", "Brave-Browser"),
    ("Chromium", "Chromium"),
    ("Vivaldi", "Vivaldi"),
    ("Yandex", "YandexBrowser"),
];
/// Variantes d'Opera : profil directement sous Roaming/Opera Software/<variante>
/// (pas de niveau « User Data/<profil> »).
const OPERA_VARIANTS: &[&str] = &["Opera Stable", "Opera GX Stable", "Opera Next", "Opera Developer"];

fn grab_with_sidecars(fs: &NtfsFs<OffsetReaderAny>, src: &str, dest: &Path) -> bool {
    let ok = grab(fs, src, dest);
    if ok {
        for sfx in ["-wal", "-shm"] {
            let s = format!("{src}{sfx}");
            let d = dest.with_file_name(format!(
                "{}{}",
                dest.file_name().unwrap().to_string_lossy(),
                sfx
            ));
            grab(fs, &s, &d);
        }
    }
    ok
}

/// Extrait tous les fichiers de profil de navigateur d'un volume NTFS.
fn extract_volume(fs: &NtfsFs<OffsetReaderAny>, out: &Path, extracted: &mut Vec<PathBuf>) {
    let users = list_dir(fs, "Users");
    for (user, is_dir) in users {
        if !is_dir {
            continue;
        }
        let la = format!("Users/{user}/AppData/Local");
        let ra = format!("Users/{user}/AppData/Roaming");

        // Navigateurs Chromium.
        for (vendor, product) in CHROMIUM_VENDORS {
            let udata = format!("{la}/{vendor}/{product}/User Data");
            for (profile, pdir) in list_dir(fs, &udata) {
                if !pdir {
                    continue;
                }
                let low = profile.to_ascii_lowercase();
                if !(low == "default" || low.starts_with("profile") || low == "guest profile") {
                    continue;
                }
                for f in CHROMIUM_FILES {
                    let src = format!("{udata}/{profile}/{f}");
                    let dest = out.join(&user).join(product).join(&profile).join(f);
                    if grab_with_sidecars(fs, &src, &dest) {
                        extracted.push(dest);
                    }
                }
                // Cookies récents (sous Network/).
                let src = format!("{udata}/{profile}/Network/Cookies");
                let dest = out.join(&user).join(product).join(&profile).join("Cookies-Network");
                if grab(fs, &src, &dest) {
                    extracted.push(dest);
                }
            }
        }

        // Internet Explorer / Edge Legacy.
        let src = format!("{la}/Microsoft/Windows/WebCache/WebCacheV01.dat");
        let dest = out.join(&user).join("IE_Edge").join("WebCacheV01.dat");
        if grab(fs, &src, &dest) {
            extracted.push(dest);
        }

        // Opera (Chromium) : profil directement sous Roaming/Opera Software/<variante>.
        for variant in OPERA_VARIANTS {
            let base = format!("{ra}/Opera Software/{variant}");
            let vdir = variant.replace(' ', "_");
            for f in CHROMIUM_FILES {
                let src = format!("{base}/{f}");
                let dest = out.join(&user).join(&vdir).join(f);
                if grab_with_sidecars(fs, &src, &dest) {
                    extracted.push(dest);
                }
            }
            let src = format!("{base}/Network/Cookies");
            let dest = out.join(&user).join(&vdir).join("Cookies-Network");
            if grab(fs, &src, &dest) {
                extracted.push(dest);
            }
        }

        // Firefox.
        let ff = format!("{ra}/Mozilla/Firefox/Profiles");
        for (profile, pdir) in list_dir(fs, &ff) {
            if !pdir {
                continue;
            }
            for f in FIREFOX_FILES {
                let src = format!("{ff}/{profile}/{f}");
                let dest = out.join(&user).join("Firefox").join(&profile).join(f);
                if grab_with_sidecars(fs, &src, &dest) {
                    extracted.push(dest);
                }
            }
        }
    }
}

/// Extrait les fichiers de profil de navigateur d'une image E01 (ou brute)
/// vers `out_dir`. Renvoie les chemins extraits.
pub fn extract_browser_files(image_path: &str, out_dir: &Path) -> R<Vec<PathBuf>> {
    let img = Image::detect(Path::new(image_path));
    let parts = ntfs_partitions(&img);
    if parts.is_empty() {
        return Err("Aucun volume NTFS trouvé dans l'image.".into());
    }
    let mut extracted = Vec::new();
    for (base, len) in parts {
        // Longueur inconnue (image de partition) -> fenêtre très large.
        let len = if len == 0 { u64::MAX / 2 } else { len };
        if let Ok(fs) = open_ntfs(&img, base, len) {
            extract_volume(&fs, out_dir, &mut extracted);
        }
    }
    Ok(extracted)
}

/// Charge une image E01 : extrait les fichiers de profil puis les parse comme
/// des sources ordinaires.
pub fn load_e01(image_path: &str) -> R<Vec<Artifact>> {
    let tmp = std::env::temp_dir().join(format!(
        "navforensics_e01_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let files = extract_browser_files(image_path, &tmp)?;

    let mut arts = Vec::new();
    for f in &files {
        // Chaque base est chargée ; les sidecars (-wal/-shm) sont des annexes,
        // pas des sources à charger seules.
        let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if name.ends_with("-wal") || name.ends_with("-shm") {
            continue;
        }
        let evidence = f
            .strip_prefix(&tmp)
            .ok()
            .map(|p| format!("E01: {}", p.display()));
        if let Ok(a) = crate::load_artifact(&f.to_string_lossy(), evidence) {
            arts.push(a);
        }
    }
    Ok(arts)
}
