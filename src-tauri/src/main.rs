#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use navforensics_engine as nf;
use nf::model::{Artifact, Category, Cell, Row};
use nf::CaseInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// Marquage/annotation d'investigation d'une entree (persistant via save/load).
#[derive(Clone, Serialize, Deserialize)]
struct TagEntry {
    sig: String,     // signature de la ligne (valeurs)
    label: String,   // nom de la balise ("A retenir", "Preuve"... ou libre)
    #[serde(default)]
    color: String,   // couleur hex de la balise (#rrggbb)
    note: String,    // annotation libre
    context: String, // vue d'origine
    summary: String, // apercu court de la ligne
    when: String,    // horodatage ISO du marquage
    #[serde(default)]
    browser: String, // navigateur source
    #[serde(default)]
    path: String,    // chemin complet du fichier source
    #[serde(default)]
    modified: Option<i64>, // date de modif du fichier source (ms Unix)
    #[serde(default)]
    size: Option<i64>, // taille du fichier source (octets)
}

/// Pile du thread de travail : 64 Mo (le thread principal Windows n'a que 1 Mo,
/// ce qui fait deborder la pile du moteur SQLite sur certaines bases).
const WORKER_STACK: usize = 64 * 1024 * 1024;

struct AppState {
    arts: Mutex<Vec<Artifact>>,
    tags: Mutex<Vec<TagEntry>>,
    // Accumulateur de tous les résultats de recherche par mots-clés de la
    // session (conservés d'une recherche à l'autre), pour la vue Résultats.
    kw_results: Mutex<Vec<nf::model::Row>>,
    // Correspondances de la dernière liste de surveillance (IOC) chargée.
    ioc_results: Mutex<Vec<nf::model::Row>>,
}

type Arts = Mutex<Vec<Artifact>>;

fn lock(m: &Arts) -> MutexGuard<'_, Vec<Artifact>> {
    // recupere l'acces meme si un verrou precedent a ete empoisonne par une panique
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn lock_tags(m: &Mutex<Vec<TagEntry>>) -> MutexGuard<'_, Vec<TagEntry>> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn lock_kw(m: &Mutex<Vec<nf::model::Row>>) -> MutexGuard<'_, Vec<nf::model::Row>> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn panic_msg(e: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = e.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else {
        "panique inconnue".to_string()
    }
}

/// Execute le traitement sur un thread dedie a GROSSE pile, en capturant les
/// paniques. Resout les debordements de pile dus a la limite de 1 Mo du thread
/// principal sous Windows, et empeche l'app de se fermer sur une panique ordinaire.
fn run<T, F>(f: F) -> Result<T, String>
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    let joined = std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(WORKER_STACK)
            .name("nf-worker".to_string())
            .spawn_scoped(s, move || catch_unwind(AssertUnwindSafe(f)))
            .map_err(|e| e.to_string())
            .map(|h| h.join())
    });
    match joined {
        Ok(Ok(Ok(v))) => Ok(v),
        Ok(Ok(Err(p))) => Err(panic_msg(p)),
        Ok(Err(p)) => Err(panic_msg(p)),
        Err(e) => Err(e),
    }
}

#[derive(Serialize)]
struct CatMeta {
    name: String,
    count: usize,
}
#[derive(Serialize)]
struct Overview {
    evidence: String,
    browser: String,
    kind: String,
    sha256: String,
    path: String,
    size: i64,
    modified: Option<i64>,
    version: Option<String>,
    installed: Option<i64>,
    notes: Vec<String>,
    categories: Vec<CatMeta>,
}

/// Taille et date de dernière modification d'un fichier (ms Unix), si accessible.
fn file_meta(path: &str) -> (i64, Option<i64>) {
    match std::fs::metadata(path) {
        Ok(m) => {
            let size = m.len() as i64;
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64);
            (size, modified)
        }
        Err(_) => (0, None),
    }
}
#[derive(Serialize)]
struct LoadSummary {
    loaded: usize,
    errors: Vec<String>,
}
#[derive(Serialize)]
struct ViewData {
    columns: Vec<String>,
    rows: Vec<Row>,
}

#[derive(Serialize)]
struct SubItem {
    label: String,
    count: usize,
    kind: String, // "cat" | "flagged"
    ai: Option<usize>,
    ci: Option<usize>,
    browser: Option<String>,
}
#[derive(Serialize)]
struct SubGroup {
    name: String,
    items: Vec<SubItem>,
}
#[derive(Serialize)]
struct BrowserGroup {
    browser: String,
    subs: Vec<SubGroup>,
}

fn cols_vec(cols: &[&str]) -> Vec<String> {
    cols.iter().map(|s| s.to_string()).collect()
}

fn noisy_set(noisy: bool) -> BTreeSet<String> {
    let mut s: BTreeSet<String> = nf::TIMELINE_CORE.iter().map(|x| x.to_string()).collect();
    if noisy {
        for x in nf::TIMELINE_NOISY {
            s.insert(x.to_string());
        }
    }
    s
}

fn label_for(p: &str) -> String {
    let path = Path::new(p);
    let parent = path
        .parent()
        .and_then(|x| x.file_name())
        .and_then(|x| x.to_str())
        .unwrap_or("");
    let fname = path.file_name().and_then(|x| x.to_str()).unwrap_or("?");
    if parent.is_empty() {
        fname.to_string()
    } else {
        format!("{} / {}", parent, fname)
    }
}

fn load_into(arts: &mut Vec<Artifact>, paths: Vec<String>) -> LoadSummary {
    let mut loaded = 0;
    let mut errors = Vec::new();
    for p in paths {
        let fname = Path::new(&p)
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("?")
            .to_string();
        // chaque fichier est isole : une panique sur l'un n'empeche pas les autres
        let res = catch_unwind(AssertUnwindSafe(|| nf::load_artifact(&p, Some(label_for(&p)))));
        match res {
            Ok(Ok(a)) => {
                arts.push(a);
                loaded += 1;
            }
            Ok(Err(e)) => errors.push(format!("{} : {}", fname, e)),
            Err(panic) => errors.push(format!("{} : panique — {}", fname, panic_msg(panic))),
        }
    }
    LoadSummary { loaded, errors }
}

#[tauri::command]
fn add_paths(state: tauri::State<AppState>, paths: Vec<String>) -> Result<LoadSummary, String> {
    let arts = &state.arts;
    run(move || {
        let mut a = lock(arts);
        load_into(&mut a, paths)
    })
}

#[tauri::command]
fn load_e01(state: tauri::State<AppState>, path: String) -> Result<LoadSummary, String> {
    let arts = &state.arts;
    run(move || -> Result<LoadSummary, String> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("navforensics_e01_{ts}"));
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let files = nf::e01::extract_browser_files(&path, &tmp)?;
        // Les bases SQLite sont chargées ; les sidecars -wal/-shm restent sur
        // disque à côté et sont repris automatiquement à la lecture.
        let paths: Vec<String> = files
            .into_iter()
            .filter(|f| {
                let n = f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                !n.ends_with("-wal") && !n.ends_with("-shm")
            })
            .map(|f| f.to_string_lossy().to_string())
            .collect();
        let mut a = lock(arts);
        Ok(load_into(&mut a, paths))
    })?
}

#[tauri::command]
fn add_folder(state: tauri::State<AppState>, folder: String) -> Result<LoadSummary, String> {
    let arts = &state.arts;
    run(move || {
        let found = nf::scan_folder(&folder);
        let mut a = lock(arts);
        load_into(&mut a, found)
    })
}

#[tauri::command]
fn add_cache(state: tauri::State<AppState>, folder: String) -> Result<LoadSummary, String> {
    let arts = &state.arts;
    run(move || {
        let art = nf::carve_cache_urls(Path::new(&folder), None);
        let ok = !art.categories.is_empty();
        lock(arts).push(art);
        LoadSummary {
            loaded: if ok { 1 } else { 0 },
            errors: if ok {
                vec![]
            } else {
                vec!["Aucune URL extraite".into()]
            },
        }
    })
}

#[tauri::command]
fn clear(state: tauri::State<AppState>) -> Result<(), String> {
    let arts = &state.arts;
    let kwres = &state.kw_results;
    let iocres = &state.ioc_results;
    run(move || {
        lock(arts).clear();
        lock_kw(kwres).clear();
        lock_kw(iocres).clear();
    })
}

/// Force/corrige le navigateur d'un scellé chargé (index dans la liste).
#[tauri::command]
fn set_browser(state: tauri::State<AppState>, ai: usize, browser: String) -> Result<(), String> {
    let mut a = lock(&state.arts);
    match a.get_mut(ai) {
        Some(art) => {
            art.browser = browser;
            Ok(())
        }
        None => Err("Scelle introuvable".into()),
    }
}

#[tauri::command]
fn overview(state: tauri::State<AppState>) -> Result<Vec<Overview>, String> {
    let arts = &state.arts;
    run(move || {
        lock(arts)
            .iter()
            .map(|a| {
                let (size, modified) = file_meta(&a.path);
                let (version, installed) = nf::browser_version_install(&a.path);
                Overview {
                evidence: a.evidence.clone(),
                browser: a.browser.clone(),
                kind: a.kind.clone(),
                sha256: a.sha256.clone(),
                path: a.path.clone(),
                size,
                modified,
                version,
                installed,
                notes: a.notes.clone(),
                categories: a
                    .categories
                    .iter()
                    .map(|c| CatMeta {
                        name: c.name.clone(),
                        count: c.rows.len(),
                    })
                    .collect(),
                }
            })
            .collect()
    })
}

#[tauri::command]
fn category(state: tauri::State<AppState>, ai: usize, ci: usize) -> Result<Option<Category>, String> {
    let arts = &state.arts;
    run(move || {
        lock(arts)
            .get(ai)
            .and_then(|a| a.categories.get(ci))
            .cloned()
    })
}

#[tauri::command]
fn timeline(state: tauri::State<AppState>, noisy: bool) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData {
            columns: cols_vec(nf::TIMELINE_COLUMNS),
            rows: nf::build_timeline(&a, &noisy_set(noisy)),
        }
    })
}

#[tauri::command]
fn domains(state: tauri::State<AppState>, noisy: bool) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData {
            columns: cols_vec(nf::DOMAIN_COLUMNS),
            rows: nf::build_domain_summary(&a, &noisy_set(noisy)),
        }
    })
}

#[tauri::command]
fn search(state: tauri::State<AppState>, term: String) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData {
            columns: cols_vec(nf::SEARCH_COLUMNS),
            rows: nf::search_all(&a, &term),
        }
    })
}

/// Arborescence Navigateur -> sous-categorie (dont la sous-categorie calculee
/// "Liens signales") pour le volet de navigation.
#[tauri::command]
fn browser_groups(state: tauri::State<AppState>) -> Result<Vec<BrowserGroup>, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        let mut browsers: Vec<String> = Vec::new();
        for art in a.iter() {
            if !browsers.iter().any(|b| b == &art.browser) {
                browsers.push(art.browser.clone());
            }
        }
        let mut groups: Vec<BrowserGroup> = Vec::new();
        for b in &browsers {
            let mut buckets: Vec<(String, Vec<SubItem>)> = nf::SUBCATEGORY_ORDER
                .iter()
                .map(|n| (n.to_string(), Vec::new()))
                .collect();
            for (ai, art) in a.iter().enumerate() {
                if &art.browser != b {
                    continue;
                }
                for (ci, cat) in art.categories.iter().enumerate() {
                    let sub = nf::sub_category(&cat.name);
                    if let Some(slot) = buckets.iter_mut().find(|(n, _)| n.as_str() == sub) {
                        slot.1.push(SubItem {
                            label: format!("{} — {}", art.evidence, cat.name),
                            count: cat.rows.len(),
                            kind: "cat".to_string(),
                            ai: Some(ai),
                            ci: Some(ci),
                            browser: None,
                        });
                    }
                }
            }
            let mut subs: Vec<SubGroup> = buckets
                .into_iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(name, items)| SubGroup { name, items })
                .collect();
            let flagged_count = nf::build_flagged(&a, Some(b)).len();
            if flagged_count > 0 {
                subs.push(SubGroup {
                    name: nf::FLAGGED_SUB.to_string(),
                    items: vec![SubItem {
                        label: format!("Liens signales ({})", flagged_count),
                        count: flagged_count,
                        kind: "flagged".to_string(),
                        ai: None,
                        ci: None,
                        browser: Some(b.clone()),
                    }],
                });
            }
            groups.push(BrowserGroup {
                browser: b.clone(),
                subs,
            });
        }
        groups
    })
}

/// Vue des liens signales ; `browser = None` = tous navigateurs confondus.
#[tauri::command]
fn flagged(state: tauri::State<AppState>, browser: Option<String>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData {
            columns: cols_vec(nf::FLAGGED_COLUMNS),
            rows: nf::build_flagged(&a, browser.as_deref()),
        }
    })
}

/// Termes de recherche extraits (decodage des requetes dans les URLs).
#[tauri::command]
fn search_terms(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData {
            columns: cols_vec(nf::SEARCH_TERMS_COLUMNS),
            rows: nf::build_search_terms(&a),
        }
    })
}

/// Synthese statistique de l'activite.
#[tauri::command]
fn stats(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData {
            columns: cols_vec(nf::STATS_COLUMNS),
            rows: nf::build_stats(&a),
        }
    })
}

/// Cookies Google Analytics decodes (horodatages __utma / _ga / __utmz).
#[tauri::command]
fn ga_cookies(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData {
            columns: cols_vec(nf::GA_COLUMNS),
            rows: nf::build_ga_cookies(&a),
        }
    })
}

/// Histogramme d'activite (par heure et par jour, UTC).
#[tauri::command]
fn histogram(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData {
            columns: cols_vec(nf::HISTO_COLUMNS),
            rows: nf::build_histogram(&a),
        }
    })
}

/// Services en ligne consultes (cloud, webmail, reseaux sociaux, messageries).
#[tauri::command]
fn online_services(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData {
            columns: cols_vec(nf::ONLINE_COLUMNS),
            rows: nf::build_online_services(&a),
        }
    })
}

/// Éléments d'identité extraits de l'autofill / formulaires.
#[tauri::command]
fn identity(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData { columns: cols_vec(nf::IDENTITY_COLUMNS), rows: nf::build_identity(&a) }
    })
}

/// Tous les téléchargements agrégés.
#[tauri::command]
fn downloads(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData { columns: cols_vec(nf::DOWNLOADS_COLUMNS), rows: nf::build_downloads(&a) }
    })
}

/// Géolocalisations / adresses recherchées.
#[tauri::command]
fn geolocation(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData { columns: cols_vec(nf::GEO_COLUMNS), rows: nf::build_geolocation(&a) }
    })
}

/// VPN / anonymiseurs consultés.
#[tauri::command]
fn vpn(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData { columns: cols_vec(nf::VPN_COLUMNS), rows: nf::build_vpn(&a) }
    })
}

#[tauri::command]
fn recovered(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData { columns: cols_vec(nf::RECOVERED_COLUMNS), rows: nf::build_recovered(&a) }
    })
}

#[tauri::command]
fn crypto(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData { columns: cols_vec(nf::CRYPTO_COLUMNS), rows: nf::build_crypto(&a) }
    })
}

/// Empreinte SHA-256 d'un fichier exporté (traçabilité).
#[tauri::command]
fn sha256_of(path: String) -> Result<String, String> {
    nf::sha256_file(Path::new(&path))
}

#[derive(Deserialize)]
struct KwItem {
    word: String,
    list: String,
}

/// Recherche par listes de mots-cles (chaque mot porte le nom de sa liste).
/// Signature d'une ligne de résultat mot-clé, pour le dédoublonnage.
fn kw_sig(r: &nf::model::Row) -> String {
    let g = |c: &str| r.get(c).map(|x| x.as_search()).unwrap_or_default();
    format!("{}|{}|{}|{}", g("Mot-cle"), g("Navigateur"), g("Categorie"), g("Valeur"))
}

#[tauri::command]
fn keyword_search(state: tauri::State<AppState>, keywords: Vec<KwItem>) -> Result<ViewData, String> {
    let arts = &state.arts;
    let kwres = &state.kw_results;
    run(move || {
        let pairs: Vec<(String, String)> =
            keywords.into_iter().map(|k| (k.word, k.list)).collect();
        let a = lock(arts);
        let rows = nf::search_keywords(&a, &pairs);
        // Accumule dans la session (sans doublon) pour la vue Résultats.
        {
            let mut acc = lock_kw(kwres);
            let mut seen: std::collections::HashSet<String> =
                acc.iter().map(kw_sig).collect();
            for r in &rows {
                if seen.insert(kw_sig(r)) {
                    acc.push(r.clone());
                }
            }
        }
        ViewData { columns: cols_vec(nf::SEARCH_KW_COLUMNS), rows }
    })
}

/// Vue Résultats : tous les résultats de recherche par mots-clés accumulés.
#[tauri::command]
fn kw_results_view(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let kwres = &state.kw_results;
    run(move || ViewData {
        columns: cols_vec(nf::SEARCH_KW_COLUMNS),
        rows: lock_kw(kwres).clone(),
    })
}

/// Charge une liste de surveillance (IOC) depuis un fichier texte, recherche
/// les correspondances dans les sources, les conserve et les renvoie.
#[tauri::command]
fn ioc_load(state: tauri::State<AppState>, path: String) -> Result<ViewData, String> {
    let arts = &state.arts;
    let iocres = &state.ioc_results;
    run(move || -> Result<ViewData, String> {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let iocs = nf::parse_iocs(&text);
        let a = lock(arts);
        let rows = nf::build_ioc_matches(&a, &iocs);
        *lock_kw(iocres) = rows.clone();
        Ok(ViewData { columns: cols_vec(nf::IOC_COLUMNS), rows })
    })?
}

/// Vue Résultats : correspondances de la liste de surveillance chargée.
#[tauri::command]
fn ioc_results_view(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let iocres = &state.ioc_results;
    run(move || ViewData {
        columns: cols_vec(nf::IOC_COLUMNS),
        rows: lock_kw(iocres).clone(),
    })
}

#[tauri::command]
fn sync_accounts(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData { columns: cols_vec(nf::SYNC_COLUMNS), rows: nf::build_sync_accounts(&a) }
    })
}

#[tauri::command]
fn extensions(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData { columns: cols_vec(nf::EXT_COLUMNS), rows: nf::build_extensions(&a) }
    })
}

#[tauri::command]
fn geopoints(state: tauri::State<AppState>) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData { columns: cols_vec(nf::GEOPOINT_COLUMNS), rows: nf::build_geopoints(&a) }
    })
}

#[tauri::command]
fn heatmap(state: tauri::State<AppState>, tz: String) -> Result<ViewData, String> {
    let arts = &state.arts;
    run(move || {
        let a = lock(arts);
        ViewData { columns: cols_vec(nf::HEATMAP_COLUMNS), rows: nf::build_heatmap(&a, &tz) }
    })
}

// --- Marquage / annotation d'investigation ---------------------------------

#[tauri::command]
fn set_tag(state: tauri::State<AppState>, entry: TagEntry) -> Result<(), String> {
    let mut t = lock_tags(&state.tags);
    if let Some(e) = t.iter_mut().find(|e| e.sig == entry.sig) {
        *e = entry;
    } else {
        t.push(entry);
    }
    Ok(())
}

#[tauri::command]
fn remove_tag(state: tauri::State<AppState>, sig: String) -> Result<(), String> {
    lock_tags(&state.tags).retain(|e| e.sig != sig);
    Ok(())
}

/// Marquage par lot (multi-selection).
#[tauri::command]
fn set_tags(state: tauri::State<AppState>, entries: Vec<TagEntry>) -> Result<(), String> {
    let mut t = lock_tags(&state.tags);
    for entry in entries {
        if let Some(e) = t.iter_mut().find(|e| e.sig == entry.sig) {
            *e = entry;
        } else {
            t.push(entry);
        }
    }
    Ok(())
}

#[tauri::command]
fn remove_tags(state: tauri::State<AppState>, sigs: Vec<String>) -> Result<(), String> {
    lock_tags(&state.tags).retain(|e| !sigs.contains(&e.sig));
    Ok(())
}

#[tauri::command]
fn list_tags(state: tauri::State<AppState>) -> Result<Vec<TagEntry>, String> {
    Ok(lock_tags(&state.tags).clone())
}

#[tauri::command]
fn save_tags(state: tauri::State<AppState>, path: String) -> Result<(), String> {
    let t = lock_tags(&state.tags);
    let json = serde_json::to_string_pretty(&*t).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_tags(state: tauri::State<AppState>, path: String) -> Result<Vec<TagEntry>, String> {
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let loaded: Vec<TagEntry> = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    let mut t = lock_tags(&state.tags);
    *t = loaded.clone();
    Ok(loaded)
}

// --- Auto-detection des profils de navigateurs (machine locale, Windows) ---

fn exists_push(out: &mut Vec<String>, p: std::path::PathBuf) {
    if p.is_file() {
        out.push(p.to_string_lossy().to_string());
    }
}

fn chromium_profile_files(out: &mut Vec<String>, profile: &Path) {
    for f in [
        "History", "Cookies", "Web Data", "Login Data", "Bookmarks", "Shortcuts",
        "Top Sites", "Favicons", "Preferences",
    ] {
        exists_push(out, profile.join(f));
    }
    // Chromium recent : cookies dans Network/
    exists_push(out, profile.join("Network").join("Cookies"));
}

fn scan_chromium_root(out: &mut Vec<String>, root: &Path) {
    if !root.is_dir() {
        return;
    }
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let name = p.file_name().and_then(|x| x.to_str()).unwrap_or("");
            if name == "Default" || name.starts_with("Profile ") || p.join("History").is_file() {
                chromium_profile_files(out, &p);
            }
        }
    }
}

fn autodetect_paths() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let l = Path::new(&local);
        scan_chromium_root(out.as_mut(), &l.join("Google").join("Chrome").join("User Data"));
        scan_chromium_root(out.as_mut(), &l.join("Microsoft").join("Edge").join("User Data"));
        scan_chromium_root(
            out.as_mut(),
            &l.join("BraveSoftware").join("Brave-Browser").join("User Data"),
        );
        scan_chromium_root(out.as_mut(), &l.join("Vivaldi").join("User Data"));
        exists_push(
            &mut out,
            l.join("Microsoft").join("Windows").join("WebCache").join("WebCacheV01.dat"),
        );
    }
    if let Ok(roaming) = std::env::var("APPDATA") {
        let r = Path::new(&roaming);
        chromium_profile_files(&mut out, &r.join("Opera Software").join("Opera Stable"));
        let ff = r.join("Mozilla").join("Firefox").join("Profiles");
        if let Ok(rd) = std::fs::read_dir(&ff) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    for f in [
                        "places.sqlite", "cookies.sqlite", "formhistory.sqlite", "logins.json",
                        "favicons.sqlite", "sessionstore.jsonlz4",
                    ] {
                        exists_push(&mut out, p.join(f));
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[tauri::command]
fn autodetect(state: tauri::State<AppState>) -> Result<LoadSummary, String> {
    let arts = &state.arts;
    run(move || {
        let paths = autodetect_paths();
        if paths.is_empty() {
            return LoadSummary {
                loaded: 0,
                errors: vec![
                    "Aucun profil de navigateur detecte sur cette machine (ou profils \
                     verrouilles : fermez les navigateurs et reessayez)."
                        .into(),
                ],
            };
        }
        let mut a = lock(arts);
        load_into(&mut a, paths)
    })
}

#[tauri::command]
fn version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

/// Extrait les images/vidéos d'un dossier de cache (ou profil) pour la galerie.
#[tauri::command]
fn cache_media(folder: String) -> Result<Vec<nf::MediaItem>, String> {
    run(move || {
        nf::carve_cache_media(Path::new(&folder), nf::MEDIA_MAX_ITEMS, nf::MEDIA_MAX_BYTES)
    })
}

/// Exporte un média (base64) vers un fichier choisi.
#[tauri::command]
fn save_media(path: String, data: String) -> Result<(), String> {
    let bytes = nf::b64_decode(&data);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())
}

/// Extrait les pages HTML en cache (code source décompressé + ressources liées).
#[tauri::command]
fn cache_html(folder: String) -> Result<Vec<nf::HtmlItem>, String> {
    run(move || nf::carve_cache_html(Path::new(&folder), nf::HTML_MAX_ITEMS))
}

fn tags_to_rows(tags: &[TagEntry]) -> Vec<Row> {
    tags.iter()
        .map(|t| {
            let mut rr = Row::new();
            rr.set("Marqueur", Cell::text(t.label.clone()));
            rr.set("Navigateur", Cell::text(t.browser.clone()));
            rr.set("Source (chemin)", Cell::text(t.path.clone()));
            rr.set("Date fichier", t.modified.map(Cell::Time).unwrap_or(Cell::Null));
            rr.set("Taille", t.size.map(Cell::Int).unwrap_or(Cell::Null));
            rr.set("Element", Cell::text(t.summary.clone()));
            rr.set("Note", Cell::text(t.note.clone()));
            rr.set("Marque le", Cell::text(t.when.clone()));
            rr.set("_couleur", Cell::text(t.color.clone())); // valeur cachee (non affichee)
            rr.set("_contexte", Cell::text(t.context.clone())); // vue d'origine (cachee)
            rr
        })
        .collect()
}

fn write_table(
    path: &str,
    columns: &[String],
    rows: &[Row],
    tz: &str,
    format: &str,
    sheet: &str,
) -> Result<(), String> {
    if format == "xlsx" {
        nf::report::export_xlsx(path, columns, rows, tz, sheet)
    } else {
        nf::report::export_csv(path, columns, rows, tz)
    }
}

#[tauri::command]
fn export_category(
    state: tauri::State<AppState>,
    ai: usize,
    ci: usize,
    path: String,
    tz: String,
    format: String,
) -> Result<(), String> {
    let arts = &state.arts;
    run(move || -> Result<(), String> {
        let a = lock(arts);
        let cat = a
            .get(ai)
            .and_then(|x| x.categories.get(ci))
            .ok_or("Categorie introuvable")?;
        write_table(&path, &cat.columns, &cat.rows, &tz, &format, &cat.name)
    })?
}

#[tauri::command]
fn export_view(
    state: tauri::State<AppState>,
    view: String,
    term: Option<String>,
    noisy: bool,
    path: String,
    tz: String,
    format: String,
) -> Result<(), String> {
    let arts = &state.arts;
    let kwres = &state.kw_results;
    let iocres = &state.ioc_results;
    run(move || -> Result<(), String> {
        let a = lock(arts);
        let (columns, rows, sheet) = match view.as_str() {
            "timeline" => (
                cols_vec(nf::TIMELINE_COLUMNS),
                nf::build_timeline(&a, &noisy_set(noisy)),
                "Timeline",
            ),
            "domains" => (
                cols_vec(nf::DOMAIN_COLUMNS),
                nf::build_domain_summary(&a, &noisy_set(noisy)),
                "Domaines",
            ),
            "search" => (
                cols_vec(nf::SEARCH_COLUMNS),
                nf::search_all(&a, &term.clone().unwrap_or_default()),
                "Recherche",
            ),
            "flagged" => (
                cols_vec(nf::FLAGGED_COLUMNS),
                nf::build_flagged(&a, term.as_deref()),
                "Liens signales",
            ),
            "search_terms" => (
                cols_vec(nf::SEARCH_TERMS_COLUMNS),
                nf::build_search_terms(&a),
                "Termes recherches",
            ),
            "stats" => (
                cols_vec(nf::STATS_COLUMNS),
                nf::build_stats(&a),
                "Statistiques",
            ),
            "ga" => (
                cols_vec(nf::GA_COLUMNS),
                nf::build_ga_cookies(&a),
                "Cookies GA",
            ),
            "histogram" => (
                cols_vec(nf::HISTO_COLUMNS),
                nf::build_histogram(&a),
                "Histogramme",
            ),
            "online" => (
                cols_vec(nf::ONLINE_COLUMNS),
                nf::build_online_services(&a),
                "Services en ligne",
            ),
            "identity" => (cols_vec(nf::IDENTITY_COLUMNS), nf::build_identity(&a), "Identite"),
            "downloads" => (cols_vec(nf::DOWNLOADS_COLUMNS), nf::build_downloads(&a), "Telechargements"),
            "geolocation" => (cols_vec(nf::GEO_COLUMNS), nf::build_geolocation(&a), "Geolocalisations"),
            "vpn" => (cols_vec(nf::VPN_COLUMNS), nf::build_vpn(&a), "VPN"),
            "recovered" => (cols_vec(nf::RECOVERED_COLUMNS), nf::build_recovered(&a), "Recuperes"),
            "crypto" => (cols_vec(nf::CRYPTO_COLUMNS), nf::build_crypto(&a), "Crypto-actifs"),
            "kwresults" => (cols_vec(nf::SEARCH_KW_COLUMNS), lock_kw(kwres).clone(), "Recherches mots-cles"),
            "ioc" => (cols_vec(nf::IOC_COLUMNS), lock_kw(iocres).clone(), "Correspondances IOC"),
            "sync" => (cols_vec(nf::SYNC_COLUMNS), nf::build_sync_accounts(&a), "Comptes sync"),
            "extensions" => (cols_vec(nf::EXT_COLUMNS), nf::build_extensions(&a), "Extensions"),
            "geopoints" => (cols_vec(nf::GEOPOINT_COLUMNS), nf::build_geopoints(&a), "Points geolocalises"),
            "heatmap" => (cols_vec(nf::HEATMAP_COLUMNS), nf::build_heatmap(&a, &tz), "Pattern of life"),
            _ => return Err("Vue inconnue".into()),
        };
        write_table(&path, &columns, &rows, &tz, &format, sheet)
    })?
}

/// Reconstruit des `Row` moteur à partir des éléments d'identité
/// sélectionnés côté interface (map colonne → texte affiché). La vue
/// Identité ne contient que des chaînes, donc la conversion est fidèle.
fn picks_to_rows(picks: Vec<std::collections::HashMap<String, String>>) -> Vec<nf::model::Row> {
    picks
        .into_iter()
        .map(|m| {
            let mut r = nf::model::Row::new();
            for (k, v) in m {
                if !v.is_empty() {
                    r.set(&k, nf::model::Cell::text(&v));
                }
            }
            r
        })
        .collect()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn export_report(
    state: tauri::State<AppState>,
    path: String,
    tz: String,
    noisy: bool,
    case: CaseInfo,
    options: nf::ReportOptions,
    identity_pick: Vec<std::collections::HashMap<String, String>>,
) -> Result<(), String> {
    let arts = &state.arts;
    let tag_rows = tags_to_rows(&lock_tags(&state.tags));
    let picks = picks_to_rows(identity_pick);
    let ioc_rows = lock_kw(&state.ioc_results).clone();
    run(move || -> Result<(), String> {
        let a = lock(arts);
        let tl = nf::build_timeline(&a, &noisy_set(noisy));
        let dom = nf::build_domain_summary(&a, &noisy_set(noisy));
        let tags = if tag_rows.is_empty() {
            None
        } else {
            Some(tag_rows.as_slice())
        };
        let pick = if picks.is_empty() {
            None
        } else {
            Some(picks.as_slice())
        };
        let iocs = if ioc_rows.is_empty() {
            None
        } else {
            Some(ioc_rows.as_slice())
        };
        nf::report::build_html_report(
            &path,
            &case,
            &a,
            &tz,
            Some(&tl),
            Some(&dom),
            tags,
            pick,
            iocs,
            &options,
        )
    })?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn export_report_pdf(
    state: tauri::State<AppState>,
    path: String,
    tz: String,
    noisy: bool,
    case: CaseInfo,
    options: nf::ReportOptions,
    identity_pick: Vec<std::collections::HashMap<String, String>>,
) -> Result<(), String> {
    let arts = &state.arts;
    let tag_rows = tags_to_rows(&lock_tags(&state.tags));
    let picks = picks_to_rows(identity_pick);
    let ioc_rows = lock_kw(&state.ioc_results).clone();
    run(move || -> Result<(), String> {
        let a = lock(arts);
        let tl = nf::build_timeline(&a, &noisy_set(noisy));
        let dom = nf::build_domain_summary(&a, &noisy_set(noisy));
        let tags = if tag_rows.is_empty() {
            None
        } else {
            Some(tag_rows.as_slice())
        };
        let pick = if picks.is_empty() {
            None
        } else {
            Some(picks.as_slice())
        };
        let iocs = if ioc_rows.is_empty() {
            None
        } else {
            Some(ioc_rows.as_slice())
        };
        nf::pdf::build_pdf_report(
            &path,
            &case,
            &a,
            &tz,
            Some(&tl),
            Some(&dom),
            tags,
            pick,
            iocs,
            &options,
        )
    })?
}

/// Journalise toute panique rattrapable dans %TEMP%/navforensics-panic.log.
/// (Un debordement de pile n'est PAS rattrapable et n'apparaitra pas ici ; il
/// est traite par la grosse pile du thread de travail ci-dessus.)
fn init_panic_log() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let path = std::env::temp_dir().join("navforensics-panic.log");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            use std::io::Write;
            let _ = writeln!(f, "[NavForensics] {}", info);
        }
        previous(info);
    }));
}

fn main() {
    init_panic_log();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            arts: Mutex::new(Vec::new()),
            tags: Mutex::new(Vec::new()),
            kw_results: Mutex::new(Vec::new()),
            ioc_results: Mutex::new(Vec::new()),
        })
        .invoke_handler(tauri::generate_handler![
            add_paths,
            add_folder,
            add_cache,
            clear,
            overview,
            set_browser,
            category,
            timeline,
            domains,
            search,
            browser_groups,
            flagged,
            search_terms,
            stats,
            ga_cookies,
            histogram,
            online_services,
            keyword_search,
            identity,
            downloads,
            geolocation,
            vpn,
            recovered,
            crypto,
            kw_results_view,
            ioc_load,
            ioc_results_view,
            sync_accounts,
            extensions,
            geopoints,
            heatmap,
            load_e01,
            sha256_of,
            set_tag,
            set_tags,
            remove_tag,
            remove_tags,
            list_tags,
            save_tags,
            load_tags,
            autodetect,
            version,
            cache_media,
            save_media,
            cache_html,
            export_category,
            export_view,
            export_report,
            export_report_pdf
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de NavForensics");
}
