//! Extraction des artefacts de navigateurs depuis une image forensique E01.
//!
//! Chaîne : image EWF/E01 (`ewf`) → table de partitions (MBR/GPT) → volume
//! NTFS (`ntfs-core`) ou ext4 (`ext4-view`), tous deux en lecture seule →
//! copie des fichiers de profil des navigateurs vers un répertoire temporaire,
//! qui sont ensuite chargés comme des sources ordinaires. Prend aussi en charge
//! une image brute (`.dd`, `.001`, `.raw`). APFS et F2FS sont détectés mais
//! non lus.
//!
//! Note : le lecteur EWF et le lecteur NTFS sont des bibliothèques dédiées ;
//! l'arborescence des profils (Users/AppData/…) est parcourue de façon
//! défensive — un fichier absent est ignoré sans erreur.

use crate::model::Artifact;
use ewf::EwfReader;
use ntfs_core::fs::NtfsFs;
use ntfs_core::source::OffsetReader;
use ext4_view::{Ext4, Ext4Read};
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
fn open_window(img: &Image, base: u64, len: u64) -> R<OffsetReaderAny> {
    Ok(match img {
        Image::Ewf(p) => {
            let r = EwfReader::open(p).map_err(|e| e.to_string())?;
            OffsetReaderAny::Ewf(OffsetReader::new(r, base, len).map_err(|e| e.to_string())?)
        }
        Image::Raw(p) => {
            let f = std::fs::File::open(p).map_err(|e| e.to_string())?;
            OffsetReaderAny::Raw(OffsetReader::new(f, base, len).map_err(|e| e.to_string())?)
        }
    })
}

fn open_ntfs(img: &Image, base: u64, len: u64) -> R<NtfsFs<OffsetReaderAny>> {
    let reader = open_window(img, base, len)?;
    NtfsFs::open(reader).map_err(|e| e.to_string())
}

/// Adaptateur : `ext4-view` lit par offset absolu dans la partition.
struct ExtReader(OffsetReaderAny);

impl Ext4Read for ExtReader {
    fn read(
        &mut self,
        start_byte: u64,
        dst: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        self.0.seek(SeekFrom::Start(start_byte))?;
        self.0.read_exact(dst)?;
        Ok(())
    }
}

/// Ouvre un volume ext2/3/4 en lecture seule.
fn open_ext4(img: &Image, base: u64, len: u64) -> R<Ext4> {
    let reader = open_window(img, base, len)?;
    Ext4::load(Box::new(ExtReader(reader))).map_err(|e| e.to_string())
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

/// Partitions (base, longueur) dont la signature satisfait `want`, en MBR ou GPT.
/// Longueur 0 = inconnue (image de partition unique, à l'offset 0).
fn partitions_where(img: &Image, want: fn(&Image, u64) -> bool) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut mbr = [0u8; 512];
    if img.read_at(0, &mut mbr).is_err() || mbr[510] != 0x55 || mbr[511] != 0xAA {
        // Pas de MBR : tenter un volume à l'offset 0 (image de partition).
        if want(img, 0) {
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
        if size > 0 && want(img, start * SECTOR) {
            out.push((start * SECTOR, size * SECTOR));
        }
    }
    if gpt {
        out.extend(gpt_where(img, want));
    }
    // À défaut, tenter l'offset 0 (image de partition unique).
    if out.is_empty() && want(img, 0) {
        out.push((0, 0));
    }
    out
}

/// Partitions GPT dont la signature satisfait `want`.
fn gpt_where(img: &Image, want: fn(&Image, u64) -> bool) -> Vec<(u64, u64)> {
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
        if want(img, base) {
            out.push((base, (last - first + 1) * SECTOR));
        }
    }
    out
}

fn is_ntfs(img: &Image, base: u64) -> bool {
    let mut b = [0u8; 16];
    img.read_at(base, &mut b).is_ok() && &b[3..11] == b"NTFS    "
}

/// ext2/3/4 : magic 0xEF53 dans le superbloc (offset 1024), champ à +56.
fn is_ext4(img: &Image, base: u64) -> bool {
    let mut b = [0u8; 2];
    img.read_at(base + 1080, &mut b).is_ok() && b == [0x53, 0xEF]
}

/// APFS (iOS, macOS) : magic « NXSB » du superbloc de conteneur à l'offset 32.
fn is_apfs(img: &Image, base: u64) -> bool {
    let mut b = [0u8; 4];
    img.read_at(base + 32, &mut b).is_ok() && &b == b"NXSB"
}

/// F2FS (Android récent) : magic 0xF2F52010 au superbloc (offset 1024).
fn is_f2fs(img: &Image, base: u64) -> bool {
    let mut b = [0u8; 4];
    img.read_at(base + 1024, &mut b).is_ok() && b == [0x10, 0x20, 0xF5, 0xF2]
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

// --- Volumes ext4 (Android) ------------------------------------------------

/// Copie un fichier ext4 vers `dest` s'il existe et tient dans le plafond.
fn grab_ext4(ext: &Ext4, path: &str, dest: &Path) -> bool {
    match ext.metadata(path) {
        Ok(m) if m.len() > 0 && m.len() as usize <= MAX_FILE => {}
        _ => return false,
    }
    let data = match ext.read(path) {
        Ok(d) if !d.is_empty() => d,
        _ => return false,
    };
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(dest, data).is_ok()
}

/// Fichier ext4 et ses sidecars -wal/-shm.
fn grab_ext4_with_sidecars(ext: &Ext4, src: &str, dest: &Path) -> bool {
    let ok = grab_ext4(ext, src, dest);
    if ok {
        for sfx in ["-wal", "-shm"] {
            let d = dest.with_file_name(format!(
                "{}{}",
                dest.file_name().unwrap().to_string_lossy(),
                sfx
            ));
            grab_ext4(ext, &format!("{src}{sfx}"), &d);
        }
    }
    ok
}

/// Paquets Android : (paquet, sous-dossier du profil sous le dossier de données).
/// Les chemins Opera et Samsung sont à vérifier sur image réelle.
const ANDROID_CHROMIUM: &[(&str, &str)] = &[
    ("com.android.chrome", "app_chrome/Default"),
    ("com.microsoft.emmx", "app_chrome/Default"),
    ("com.sec.android.app.sbrowser", "app_sbrowser/Default"),
    ("com.opera.browser", "app_opera/Default"),
    ("com.opera.gx", "app_opera/Default"),
];
/// Racines de données applicatives : `/data/data` puis `/data/user/0`.
const ANDROID_ROOTS: &[&str] = &["/data/data", "/data/user/0"];

/// Extrait les profils de navigateurs Android d'un volume ext4.
fn extract_android(ext: &Ext4, out: &Path, extracted: &mut Vec<PathBuf>) {
    for (pkg, rel) in ANDROID_CHROMIUM {
        for root in ANDROID_ROOTS {
            let before = extracted.len();
            let base = format!("{root}/{pkg}/{rel}");
            let dir = out.join("android").join(pkg);
            for f in CHROMIUM_FILES {
                let dest = dir.join(f);
                if grab_ext4_with_sidecars(ext, &format!("{base}/{f}"), &dest) {
                    extracted.push(dest);
                }
            }
            let dest = dir.join("Cookies-Network");
            if grab_ext4(ext, &format!("{base}/Network/Cookies"), &dest) {
                extracted.push(dest);
            }
            if extracted.len() > before {
                break;
            }
        }
    }

    // Firefox Android : files/mozilla/<profil>/.
    for root in ANDROID_ROOTS {
        let before = extracted.len();
        let pkg = "org.mozilla.firefox";
        let ff = format!("{root}/{pkg}/files/mozilla");
        if let Ok(entries) = ext.read_dir(ff.as_str()) {
            for entry in entries.flatten() {
                let name = match entry.file_name().as_str() {
                    Ok(n) if n != "." && n != ".." => n.to_string(),
                    _ => continue,
                };
                for f in FIREFOX_FILES {
                    let dest = out.join("android").join(pkg).join(&name).join(f);
                    if grab_ext4_with_sidecars(ext, &format!("{ff}/{name}/{f}"), &dest) {
                        extracted.push(dest);
                    }
                }
            }
        }
        if extracted.len() > before {
            break;
        }
    }
}

/// Extrait les fichiers de profil de navigateur d'une image E01 (ou brute)
/// vers `out_dir`. Renvoie les chemins extraits.
///
/// Volumes lus : NTFS (Windows) et ext4 (Android). APFS (iOS, macOS) et F2FS
/// (Android récent) sont détectés mais non lus : un message explicite est
/// renvoyé lorsque rien n'a pu être extrait.
pub fn extract_browser_files(image_path: &str, out_dir: &Path) -> R<Vec<PathBuf>> {
    let img = Image::detect(Path::new(image_path));
    let ntfs = partitions_where(&img, is_ntfs);
    let ext = partitions_where(&img, is_ext4);
    let apfs = partitions_where(&img, is_apfs);
    let f2fs = partitions_where(&img, is_f2fs);
    let mut extracted = Vec::new();
    for (base, len) in ntfs.iter().copied() {
        // Longueur inconnue (image de partition) -> fenêtre très large.
        let len = if len == 0 { u64::MAX / 2 } else { len };
        if let Ok(fs) = open_ntfs(&img, base, len) {
            extract_volume(&fs, out_dir, &mut extracted);
        }
    }
    for (base, len) in ext.iter().copied() {
        let len = if len == 0 { u64::MAX / 2 } else { len };
        if let Ok(fs) = open_ext4(&img, base, len) {
            extract_android(&fs, out_dir, &mut extracted);
        }
    }
    if extracted.is_empty() && ntfs.is_empty() {
        if !apfs.is_empty() {
            return Err(
                "Volume APFS détecté (iOS ou macOS) : sa lecture n'est pas prise en charge, \
                 aucun profil extrait."
                    .into(),
            );
        }
        if !f2fs.is_empty() {
            return Err(
                "Volume F2FS détecté (Android récent) : sa lecture n'est pas prise en charge, \
                 aucun profil extrait."
                    .into(),
            );
        }
        if ext.is_empty() {
            return Err("Aucun volume NTFS ou ext4 reconnu dans l'image.".into());
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
