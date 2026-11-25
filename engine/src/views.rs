//! Vues transversales : timeline unifiee, regroupement par domaine, recherche
//! globale, et carving d'un dossier de cache.

use crate::model::{Artifact, Category, Cell, Row};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;

pub const TIMELINE_COLUMNS: &[&str] =
    &["Date", "Type", "Navigateur", "Scelle", "Detail", "Source"];
pub const DOMAIN_COLUMNS: &[&str] =
    &["Domaine", "Evenements", "Visites", "Telechargements", "Premier", "Dernier"];
pub const SEARCH_COLUMNS: &[&str] =
    &["Scelle", "Navigateur", "Categorie", "Colonne", "Valeur"];
pub const SEARCH_TERMS_COLUMNS: &[&str] =
    &["Navigateur", "Moteur", "Terme recherche", "Date", "URL"];
pub const STATS_COLUMNS: &[&str] = &["Rubrique", "Detail", "Valeur"];
pub const GA_COLUMNS: &[&str] =
    &["Navigateur", "Hote", "Cookie", "Role", "Horodatage", "Valeur brute"];
pub const ONLINE_COLUMNS: &[&str] =
    &["Type", "Service", "Navigateur", "Compte / detail", "URL", "Date"];
pub const SEARCH_KW_COLUMNS: &[&str] =
    &["Mot-cle", "Liste", "Navigateur", "Categorie", "Colonne", "Valeur"];
pub const IDENTITY_COLUMNS: &[&str] = &["Type", "Valeur", "Champ", "Navigateur", "Source"];
pub const DOWNLOADS_COLUMNS: &[&str] = &["Navigateur", "Date", "Fichier", "URL", "Taille (o)"];
pub const GEO_COLUMNS: &[&str] = &["Type", "Valeur", "Navigateur", "Date"];
pub const VPN_COLUMNS: &[&str] = &["Service", "Navigateur", "URL", "Date"];
pub const HISTO_COLUMNS: &[&str] = &["Categorie", "Creneau", "Evenements", "Repartition"];
pub const TAG_COLUMNS: &[&str] = &[
    "Marqueur", "Navigateur", "Source (chemin)", "Modifie", "Taille", "Element", "Note", "Date",
];

pub const TIMELINE_CORE: &[&str] = &[
    "Visite",
    "Telechargement",
    "Recherche",
    "Saisie omnibox",
    "Marque-page",
    "Identifiant",
];
pub const TIMELINE_NOISY: &[&str] = &["Cookie", "Autofill", "Formulaire"];

// (categorie, type d'evenement, colonne horodatage ou "" = auto, colonnes detail)
fn timeline_cfg(cat: &str) -> Option<(&'static str, &'static str, Vec<&'static str>)> {
    match cat {
        "Visites" => Some(("Visite", "Date visite", vec!["URL", "Titre"])),
        "Historique (IE/Edge Legacy)" => {
            Some(("Visite", "Derniere visite", vec!["URL", "Fichier"]))
        }
        "Telechargements" => Some((
            "Telechargement",
            "",
            vec!["Fichier cible", "Fichier / URI", "Fichier", "URL onglet", "Page source", "URL"],
        )),
        "Termes de recherche" => Some(("Recherche", "Derniere visite", vec!["Terme recherche", "URL"])),
        "Saisies omnibox (raccourcis)" => {
            Some(("Saisie omnibox", "Dernier acces", vec!["Texte saisi", "URL"]))
        }
        "Marque-pages" => Some(("Marque-page", "", vec!["Nom", "Titre", "URL"])),
        "Cookies" => Some(("Cookie", "Creation", vec!["Hote", "Nom"])),
        "Saisie auto (Autofill)" => {
            Some(("Autofill", "Derniere utilisation", vec!["Champ", "Valeur"]))
        }
        "Historique de formulaires" => {
            Some(("Formulaire", "Derniere utilisation", vec!["Champ", "Valeur"]))
        }
        "Identifiants (metadonnees, sans mot de passe)" => Some((
            "Identifiant",
            "Derniere utilisation",
            vec!["Hote", "URL origine", "Nom utilisateur"],
        )),
        _ => None,
    }
}

fn first_time_col(cat: &Category) -> Option<String> {
    for c in &cat.columns {
        for r in cat.rows.iter().take(200) {
            if matches!(r.get(c), Some(Cell::Time(_))) {
                return Some(c.clone());
            }
        }
    }
    None
}

fn event_detail(cols: &[&str], row: &Row) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in cols {
        if let Some(cell) = row.get(c) {
            let s = cell.as_search();
            if !s.is_empty() && s != "0" && !parts.contains(&s) {
                parts.push(s);
            }
        }
    }
    parts.join("  \u{2014}  ")
}

pub fn build_timeline(arts: &[Artifact], include: &BTreeSet<String>) -> Vec<Row> {
    let mut events: Vec<Row> = Vec::new();
    for art in arts {
        for cat in &art.categories {
            let cfg = match timeline_cfg(&cat.name) {
                Some(c) => c,
                None => continue,
            };
            let (etype, tcol, detail_cols) = cfg;
            if !include.contains(etype) {
                continue;
            }
            let tcol: String = if !tcol.is_empty() && cat.columns.iter().any(|c| c == tcol) {
                tcol.to_string()
            } else {
                match first_time_col(cat) {
                    Some(c) => c,
                    None => continue,
                }
            };
            for r in &cat.rows {
                if let Some(Cell::Time(ms)) = r.get(&tcol) {
                    let mut rr = Row::new();
                    rr.set("Date", Cell::Time(*ms));
                    rr.set("Type", Cell::text(etype));
                    rr.set("Navigateur", Cell::text(art.browser.clone()));
                    rr.set("Scelle", Cell::text(art.evidence.clone()));
                    rr.set("Detail", Cell::text(event_detail(&detail_cols, r)));
                    rr.set("Source", Cell::text(cat.name.clone()));
                    events.push(rr);
                }
            }
        }
    }
    events.sort_by_key(|r| r.get("Date").and_then(|c| c.time_ms()).unwrap_or(i64::MAX));
    events
}

fn host_of(url: &str) -> String {
    let s = url.trim();
    let after = s.split("://").nth(1).unwrap_or(s);
    let host = after.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.split('@').last().unwrap_or(host);
    host.split(':').next().unwrap_or(host).to_lowercase()
}

pub fn build_domain_summary(arts: &[Artifact], include: &BTreeSet<String>) -> Vec<Row> {
    let re = Regex::new(r"https?://[^\s\x{2014}]+").unwrap();
    let events = build_timeline(arts, include);
    use std::collections::HashMap;
    struct Agg {
        events: i64,
        visites: i64,
        dl: i64,
        first: i64,
        last: i64,
    }
    let mut map: HashMap<String, Agg> = HashMap::new();
    for e in &events {
        let detail = e.get("Detail").map(|c| c.as_search()).unwrap_or_default();
        let url = re.find(&detail).map(|m| m.as_str().to_string()).unwrap_or_default();
        let host = host_of(&url);
        if host.is_empty() {
            continue;
        }
        let ms = e.get("Date").and_then(|c| c.time_ms()).unwrap_or(0);
        let etype = e.get("Type").map(|c| c.as_search()).unwrap_or_default();
        let a = map.entry(host).or_insert(Agg {
            events: 0,
            visites: 0,
            dl: 0,
            first: i64::MAX,
            last: i64::MIN,
        });
        a.events += 1;
        if etype == "Visite" {
            a.visites += 1;
        } else if etype == "Telechargement" {
            a.dl += 1;
        }
        if ms < a.first {
            a.first = ms;
        }
        if ms > a.last {
            a.last = ms;
        }
    }
    let mut rows: Vec<Row> = map
        .into_iter()
        .map(|(host, a)| {
            let mut rr = Row::new();
            rr.set("Domaine", Cell::text(host));
            rr.set("Evenements", Cell::Int(a.events));
            rr.set("Visites", Cell::Int(a.visites));
            rr.set("Telechargements", Cell::Int(a.dl));
            rr.set("Premier", Cell::Time(a.first));
            rr.set("Dernier", Cell::Time(a.last));
            rr
        })
        .collect();
    rows.sort_by_key(|r| -r.get("Evenements").and_then(|c| match c {
        Cell::Int(n) => Some(*n),
        _ => None,
    }).unwrap_or(0));
    rows
}

pub fn search_all(arts: &[Artifact], term: &str) -> Vec<Row> {
    let tl = term.trim().to_lowercase();
    let mut out = Vec::new();
    if tl.is_empty() {
        return out;
    }
    for art in arts {
        for cat in &art.categories {
            for r in &cat.rows {
                for c in &cat.columns {
                    let s = r.get(c).map(|c| c.as_search()).unwrap_or_default();
                    if s.to_lowercase().contains(&tl) {
                        let mut rr = Row::new();
                        rr.set("Scelle", Cell::text(art.evidence.clone()));
                        rr.set("Navigateur", Cell::text(art.browser.clone()));
                        rr.set("Categorie", Cell::text(cat.name.clone()));
                        rr.set("Colonne", Cell::text(c.clone()));
                        rr.set("Valeur", Cell::text(s));
                        out.push(rr);
                        break;
                    }
                }
            }
        }
    }
    out
}

// --- Extraction des termes de recherche (decodage des requetes dans les URLs) ---

fn hexval(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decode le pourcentage-encodage d'une valeur de requete (%XX et '+').
fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'+' {
            out.push(b' ');
            i += 1;
        } else if c == b'%' && i + 2 < b.len() {
            match (hexval(b[i + 1]), hexval(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push((h << 4) | l);
                    i += 3;
                }
                _ => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Reconnait un moteur de recherche d'apres l'hote et renvoie (nom, parametres a tester).
fn search_engine(host: &str) -> Option<(&'static str, &'static [&'static str])> {
    let h = host;
    if h.contains("google.") {
        Some(("Google", &["q"]))
    } else if h.contains("bing.com") {
        Some(("Bing", &["q"]))
    } else if h.contains("search.yahoo") {
        Some(("Yahoo", &["p"]))
    } else if h.contains("duckduckgo.com") {
        Some(("DuckDuckGo", &["q"]))
    } else if h.contains("qwant.com") {
        Some(("Qwant", &["q"]))
    } else if h.contains("ecosia.org") {
        Some(("Ecosia", &["q"]))
    } else if h.contains("startpage.com") {
        Some(("Startpage", &["query", "q"]))
    } else if h.contains("search.brave.com") {
        Some(("Brave", &["q"]))
    } else if h.contains("yandex.") {
        Some(("Yandex", &["text"]))
    } else if h.contains("baidu.com") {
        Some(("Baidu", &["wd", "word"]))
    } else if h.contains("youtube.com") {
        Some(("YouTube", &["search_query"]))
    } else {
        None
    }
}

fn query_param(url: &str, keys: &[&str]) -> Option<String> {
    let q = url.split('?').nth(1)?;
    let q = q.split('#').next().unwrap_or(q);
    for pair in q.split('&') {
        let mut it = pair.splitn(2, '=');
        let k = it.next().unwrap_or("");
        let v = it.next().unwrap_or("");
        if keys.iter().any(|kk| *kk == k) {
            let dec = url_decode(v);
            let dec = dec.trim();
            if !dec.is_empty() {
                return Some(dec.to_string());
            }
        }
    }
    None
}

fn row_first_time(r: &Row, columns: &[String]) -> Option<i64> {
    for c in columns {
        if let Some(Cell::Time(ms)) = r.get(c) {
            return Some(*ms);
        }
    }
    None
}

/// Extrait les termes de recherche saisis par l'utilisateur, en decodant les
/// requetes contenues dans les URLs d'historique (facon NetAnalysis).
pub fn build_search_terms(arts: &[Artifact]) -> Vec<Row> {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Row> = Vec::new();
    for art in arts {
        for cat in &art.categories {
            if !cat.columns.iter().any(|c| c == "URL") {
                continue;
            }
            for r in &cat.rows {
                let url = r.get("URL").map(|c| c.as_search()).unwrap_or_default();
                if url.is_empty() {
                    continue;
                }
                let host = host_of(&url);
                let (engine, keys) = match search_engine(&host) {
                    Some(e) => e,
                    None => continue,
                };
                let term = match query_param(&url, keys) {
                    Some(t) => t,
                    None => continue,
                };
                let date = row_first_time(r, &cat.columns);
                let key = format!("{}|{}|{}|{}", art.browser, engine, term.to_lowercase(), date.unwrap_or(0));
                if !seen.insert(key) {
                    continue;
                }
                let mut rr = Row::new();
                rr.set("Navigateur", Cell::text(art.browser.clone()));
                rr.set("Moteur", Cell::text(engine));
                rr.set("Terme recherche", Cell::text(term));
                rr.set("Date", date.map(Cell::Time).unwrap_or(Cell::Null));
                rr.set("URL", Cell::text(url));
                out.push(rr);
            }
        }
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.get("Date").and_then(|c| c.time_ms()).unwrap_or(i64::MIN)));
    out
}

// --- Synthese statistique (facon tableau de bord NetAnalysis) ---------------

fn all_event_types() -> BTreeSet<String> {
    TIMELINE_CORE
        .iter()
        .chain(TIMELINE_NOISY.iter())
        .map(|s| s.to_string())
        .collect()
}

fn stat_row(rub: &str, detail: &str, value: Cell) -> Row {
    let mut rr = Row::new();
    rr.set("Rubrique", Cell::text(rub));
    rr.set("Detail", Cell::text(detail));
    rr.set("Valeur", value);
    rr
}

/// Produit une synthese chiffree de l'activite (periode, volumes, navigateurs,
/// liens signales, recherches, domaines les plus actifs).
pub fn build_stats(arts: &[Artifact]) -> Vec<Row> {
    use std::collections::BTreeMap;
    let mut out: Vec<Row> = Vec::new();

    // periode d'activite + volume total (balayage de toutes les cellules Time)
    let mut min_ms = i64::MAX;
    let mut max_ms = i64::MIN;
    let mut total_rows: i64 = 0;
    let mut per_browser: BTreeMap<String, i64> = BTreeMap::new();
    for art in arts {
        for cat in &art.categories {
            total_rows += cat.rows.len() as i64;
            *per_browser.entry(art.browser.clone()).or_insert(0) += cat.rows.len() as i64;
            for r in &cat.rows {
                for c in &cat.columns {
                    if let Some(Cell::Time(ms)) = r.get(c) {
                        if *ms > 0 {
                            if *ms < min_ms {
                                min_ms = *ms;
                            }
                            if *ms > max_ms {
                                max_ms = *ms;
                            }
                        }
                    }
                }
            }
        }
    }
    if min_ms != i64::MAX {
        out.push(stat_row("Periode", "Premiere activite datee", Cell::Time(min_ms)));
        out.push(stat_row("Periode", "Derniere activite datee", Cell::Time(max_ms)));
    }
    out.push(stat_row("Volume", "Entrees totales (toutes categories)", Cell::Int(total_rows)));
    let tl = build_timeline(arts, &all_event_types());
    out.push(stat_row("Volume", "Evenements dates (timeline)", Cell::Int(tl.len() as i64)));

    for (b, n) in &per_browser {
        out.push(stat_row("Par navigateur", b, Cell::Int(*n)));
    }

    // liens signales par type
    let flagged = crate::taxonomy::build_flagged(arts, None);
    let mut tor = 0i64;
    let mut adulte = 0i64;
    let mut suspect = 0i64;
    for r in &flagged {
        match r.get("Signalement").map(|c| c.as_search()).unwrap_or_default().as_str() {
            "Tor (.onion)" => tor += 1,
            "Adulte" => adulte += 1,
            "Suspect" => suspect += 1,
            _ => {}
        }
    }
    out.push(stat_row("Liens signales", "Total", Cell::Int(flagged.len() as i64)));
    out.push(stat_row("Liens signales", "Tor (.onion)", Cell::Int(tor)));
    out.push(stat_row("Liens signales", "Adulte", Cell::Int(adulte)));
    out.push(stat_row("Liens signales", "Suspect", Cell::Int(suspect)));

    // recherches
    let terms = build_search_terms(arts);
    out.push(stat_row("Recherches", "Termes de recherche extraits", Cell::Int(terms.len() as i64)));

    // domaines
    let doms = build_domain_summary(arts, &all_event_types());
    out.push(stat_row("Domaines", "Domaines distincts", Cell::Int(doms.len() as i64)));
    for (i, d) in doms.iter().take(10).enumerate() {
        let host = d.get("Domaine").map(|c| c.as_search()).unwrap_or_default();
        let ev = d.get("Evenements").cloned().unwrap_or(Cell::Null);
        out.push(stat_row("Domaines", &format!("Top {:02} : {}", i + 1, host), ev));
    }

    out
}

// --- Décodage des cookies Google Analytics (horodatages) -------------------

fn unix_s_to_ms(s: i64) -> Option<i64> {
    // plage plausible : 1995-01-01 .. 2100-01-01 (secondes Unix)
    if (788_918_400..=4_102_444_800).contains(&s) {
        Some(s * 1000)
    } else {
        None
    }
}

fn ga_push(out: &mut Vec<Row>, browser: &str, host: &str, cookie: &str, role: &str, ts_s: i64, raw: &str) {
    if let Some(ms) = unix_s_to_ms(ts_s) {
        let mut rr = Row::new();
        rr.set("Navigateur", Cell::text(browser.to_string()));
        rr.set("Hote", Cell::text(host.to_string()));
        rr.set("Cookie", Cell::text(cookie.to_string()));
        rr.set("Role", Cell::text(role.to_string()));
        rr.set("Horodatage", Cell::Time(ms));
        rr.set("Valeur brute", Cell::text(raw.to_string()));
        out.push(rr);
    }
}

/// Décode les horodatages contenus dans les cookies Google Analytics
/// (__utma, __utmz, _ga). Les valeurs de cookies Chromium étant chiffrées, ce
/// décodage s'applique surtout aux cookies Firefox (valeurs en clair).
pub fn build_ga_cookies(arts: &[Artifact]) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for art in arts {
        for cat in &art.categories {
            let has = |n: &str| cat.columns.iter().any(|c| c == n);
            if !(has("Nom") && has("Valeur")) {
                continue;
            }
            for r in &cat.rows {
                let name = r.get("Nom").map(|c| c.as_search()).unwrap_or_default();
                let value = r.get("Valeur").map(|c| c.as_search()).unwrap_or_default();
                let host = r.get("Hote").map(|c| c.as_search()).unwrap_or_default();
                if value.is_empty() {
                    continue;
                }
                let parts: Vec<&str> = value.split('.').collect();
                match name.as_str() {
                    "__utma" if parts.len() >= 6 => {
                        let p = |i: usize| parts[i].parse::<i64>().unwrap_or(0);
                        ga_push(&mut out, &art.browser, &host, "__utma", "1ere visite", p(2), &value);
                        ga_push(&mut out, &art.browser, &host, "__utma", "Visite precedente", p(3), &value);
                        ga_push(&mut out, &art.browser, &host, "__utma", "Derniere visite", p(4), &value);
                    }
                    "__utmz" if parts.len() >= 2 => {
                        let ts = parts[1].parse::<i64>().unwrap_or(0);
                        ga_push(&mut out, &art.browser, &host, "__utmz", "Campagne / referent", ts, &value);
                    }
                    "__utmb" if parts.len() >= 4 => {
                        let ts = parts[3].parse::<i64>().unwrap_or(0);
                        ga_push(&mut out, &art.browser, &host, "__utmb", "Debut de session", ts, &value);
                    }
                    "_ga" if parts.len() >= 4 => {
                        // GA1.2.<clientId>.<premiereVisiteUnix> : dernier champ = horodatage
                        let ts = parts[parts.len() - 1].parse::<i64>().unwrap_or(0);
                        ga_push(&mut out, &art.browser, &host, "_ga", "1ere visite (GA4)", ts, &value);
                    }
                    _ => {}
                }
            }
        }
    }
    out.sort_by_key(|r| r.get("Horodatage").and_then(|c| c.time_ms()).unwrap_or(i64::MAX));
    out
}

// --- Détection des services en ligne (cloud, webmail, réseaux sociaux) ------

// (fragment d'hote, type, nom lisible)
const ONLINE_SERVICES: &[(&str, &str, &str)] = &[
    // Stockage cloud
    ("dropbox.com", "Stockage cloud", "Dropbox"),
    ("drive.google.com", "Stockage cloud", "Google Drive"),
    ("docs.google.com", "Stockage cloud", "Google Docs"),
    ("onedrive.live.com", "Stockage cloud", "OneDrive"),
    ("1drv.ms", "Stockage cloud", "OneDrive"),
    ("icloud.com", "Stockage cloud", "iCloud"),
    ("mega.nz", "Stockage cloud", "MEGA"),
    ("mega.io", "Stockage cloud", "MEGA"),
    ("box.com", "Stockage cloud", "Box"),
    ("pcloud.com", "Stockage cloud", "pCloud"),
    ("wetransfer.com", "Stockage cloud", "WeTransfer"),
    ("mediafire.com", "Stockage cloud", "MediaFire"),
    ("sync.com", "Stockage cloud", "Sync.com"),
    ("tresorit.com", "Stockage cloud", "Tresorit"),
    ("smash.com", "Stockage cloud", "Smash"),
    ("grosfichiers.com", "Stockage cloud", "Grosfichiers"),
    ("swisstransfer.com", "Stockage cloud", "SwissTransfer"),
    // Webmail
    ("mail.google.com", "Webmail", "Gmail"),
    ("outlook.live.com", "Webmail", "Outlook.com"),
    ("outlook.office.com", "Webmail", "Outlook (O365)"),
    ("outlook.office365.com", "Webmail", "Outlook (O365)"),
    ("mail.yahoo.com", "Webmail", "Yahoo Mail"),
    ("mail.proton.me", "Webmail", "Proton Mail"),
    ("protonmail.com", "Webmail", "Proton Mail"),
    ("mail.gmx.", "Webmail", "GMX"),
    ("mail.com", "Webmail", "Mail.com"),
    ("webmail.laposte.net", "Webmail", "Laposte.net"),
    ("laposte.net", "Webmail", "Laposte.net"),
    ("webmail.orange.fr", "Webmail", "Orange Mail"),
    ("mail.sfr.fr", "Webmail", "SFR Mail"),
    ("webmail.free.fr", "Webmail", "Free Webmail"),
    ("zimbra", "Webmail", "Zimbra"),
    ("roundcube", "Webmail", "Roundcube"),
    ("mail.yandex.", "Webmail", "Yandex Mail"),
    ("mail.zoho.", "Webmail", "Zoho Mail"),
    ("tuta.com", "Webmail", "Tuta (Tutanota)"),
    ("tutanota.com", "Webmail", "Tuta (Tutanota)"),
    // Réseaux sociaux / messageries
    ("facebook.com", "Reseau social", "Facebook"),
    ("instagram.com", "Reseau social", "Instagram"),
    ("twitter.com", "Reseau social", "X (Twitter)"),
    ("x.com", "Reseau social", "X (Twitter)"),
    ("tiktok.com", "Reseau social", "TikTok"),
    ("snapchat.com", "Reseau social", "Snapchat"),
    ("linkedin.com", "Reseau social", "LinkedIn"),
    ("web.telegram.org", "Messagerie", "Telegram Web"),
    ("web.whatsapp.com", "Messagerie", "WhatsApp Web"),
    ("discord.com", "Messagerie", "Discord"),
    ("reddit.com", "Reseau social", "Reddit"),
];

fn online_match(host: &str) -> Option<(&'static str, &'static str)> {
    for (frag, kind, name) in ONLINE_SERVICES {
        if host.contains(frag) {
            return Some((kind, name));
        }
    }
    None
}

/// Repère les services en ligne consultés (stockage cloud, webmail, réseaux
/// sociaux, messageries) à partir des URLs d'historique.
pub fn build_online_services(arts: &[Artifact]) -> Vec<Row> {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Row> = Vec::new();
    for art in arts {
        for cat in &art.categories {
            if !cat.columns.iter().any(|c| c == "URL") {
                continue;
            }
            for r in &cat.rows {
                let url = r.get("URL").map(|c| c.as_search()).unwrap_or_default();
                if url.is_empty() {
                    continue;
                }
                let host = host_of(&url);
                let (kind, name) = match online_match(&host) {
                    Some(v) => v,
                    None => continue,
                };
                if !seen.insert(format!("{}|{}|{}", art.browser, name, url)) {
                    continue;
                }
                let detail = r.get("Titre").map(|c| c.as_search()).unwrap_or_default();
                let date = row_first_time(r, &cat.columns);
                let mut rr = Row::new();
                rr.set("Type", Cell::text(kind));
                rr.set("Service", Cell::text(name));
                rr.set("Navigateur", Cell::text(art.browser.clone()));
                rr.set("Compte / detail", Cell::text(detail));
                rr.set("URL", Cell::text(url));
                rr.set("Date", date.map(Cell::Time).unwrap_or(Cell::Null));
                out.push(rr);
            }
        }
    }
    out.sort_by(|a, b| {
        let ka = a.get("Type").map(|c| c.as_search()).unwrap_or_default();
        let kb = b.get("Type").map(|c| c.as_search()).unwrap_or_default();
        ka.cmp(&kb)
    });
    out
}

// --- Recherche par listes de mots-clés --------------------------------------

/// Recherche la présence de mots-clés (insensible à la casse) dans toutes les
/// données. `keywords` est une liste de (mot, nom de liste).
pub fn search_keywords(arts: &[Artifact], keywords: &[(String, String)]) -> Vec<Row> {
    let kws: Vec<(String, String, String)> = keywords
        .iter()
        .filter(|(k, _)| !k.trim().is_empty())
        .map(|(k, list)| (k.clone(), k.to_lowercase(), list.clone()))
        .collect();
    let mut out: Vec<Row> = Vec::new();
    for art in arts {
        for cat in &art.categories {
            for r in &cat.rows {
                for c in &cat.columns {
                    let s = r.get(c).map(|x| x.as_search()).unwrap_or_default();
                    if s.is_empty() {
                        continue;
                    }
                    let low = s.to_lowercase();
                    for (kw, kwl, list) in &kws {
                        if low.contains(kwl) {
                            let mut rr = Row::new();
                            rr.set("Mot-cle", Cell::text(kw.clone()));
                            rr.set("Liste", Cell::text(list.clone()));
                            rr.set("Navigateur", Cell::text(art.browser.clone()));
                            rr.set("Categorie", Cell::text(cat.name.clone()));
                            rr.set("Colonne", Cell::text(c.clone()));
                            rr.set("Valeur", Cell::text(s.clone()));
                            out.push(rr);
                            break; // un seul mot-cle par cellule
                        }
                    }
                }
            }
        }
    }
    out
}

// --- Extraction des éléments d'identité (depuis l'autofill / formulaires) ---

fn luhn_ok(digits: &str) -> bool {
    let ds: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
    if ds.len() < 13 || ds.len() > 19 {
        return false;
    }
    let mut sum = 0u32;
    let mut alt = false;
    for &d in ds.iter().rev() {
        let mut d = d;
        if alt {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        sum += d;
        alt = !alt;
    }
    sum % 10 == 0
}

fn only_digits(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Classe une paire (champ, valeur) en type d'identité, ou None.
fn classify_identity(
    field: &str,
    value: &str,
    re_email: &Regex,
    re_iban: &Regex,
    re_immat: &Regex,
) -> Option<&'static str> {
    let v = value.trim();
    if v.len() < 3 || v.len() > 200 {
        return None;
    }
    // 1) validation par la valeur (haute confiance)
    if re_email.is_match(v) {
        return Some("Email");
    }
    let digits = only_digits(v);
    if (13..=19).contains(&digits.len()) && luhn_ok(&digits) {
        return Some("Carte bancaire");
    }
    let compact = v.replace([' ', '\u{00a0}'], "").to_ascii_uppercase();
    if re_iban.is_match(&compact) {
        return Some("IBAN / banque");
    }
    // NIR (securite sociale FR) : 15 chiffres, sexe 1/2 au debut
    if digits.len() == 15 && matches!(digits.as_bytes()[0], b'1' | b'2') {
        return Some("Securite sociale (NIR)");
    }
    // telephone FR
    if (9..=12).contains(&digits.len())
        && (v.starts_with('0') || v.starts_with("+33") || v.starts_with("0033"))
    {
        return Some("Telephone");
    }
    if re_immat.is_match(&v.to_ascii_uppercase()) {
        return Some("Immatriculation");
    }
    // 2) indices par le nom du champ
    let f = field.to_lowercase();
    let has = |keys: &[&str]| keys.iter().any(|k| f.contains(k));
    if has(&["mail"]) {
        Some("Email")
    } else if has(&["phone", "tel", "mobile", "portable"]) {
        Some("Telephone")
    } else if has(&["firstname", "lastname", "fullname", "prenom", "nom"]) || f == "name" {
        Some("Nom / Prenom")
    } else if has(&["birth", "naissance", "dob", "anniversaire"]) {
        Some("Date de naissance")
    } else if has(&["address", "adresse", "street", "rue", "postal", "zip", "city", "ville"]) {
        Some("Adresse")
    } else if has(&["card", "carte", "cc-", "cardnumber"]) {
        Some("Carte bancaire")
    } else if has(&["iban", "bank", "banque", "account", "compte"]) {
        Some("Banque")
    } else if has(&["ssn", "secu", "nir", "social", "insee"]) {
        Some("Securite sociale")
    } else if has(&["plate", "immat", "vehic", "voiture", "car", "marque", "modele"]) {
        Some("Vehicule / immatriculation")
    } else {
        None
    }
}

/// Extrait les éléments d'identité remplis dans l'autofill / l'historique de
/// formulaires (et les comptes connectés), classés par type.
pub fn build_identity(arts: &[Artifact]) -> Vec<Row> {
    use std::collections::HashSet;
    let re_email = Regex::new(r"(?i)^[\w.%+\-]+@[\w.\-]+\.[a-z]{2,}$").unwrap();
    let re_iban = Regex::new(r"^[A-Z]{2}\d{2}[A-Z0-9]{10,30}$").unwrap();
    let re_immat = Regex::new(r"^[A-Z]{2}-\d{3}-[A-Z]{2}$").unwrap();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Row> = Vec::new();
    let mut push = |ty: &str, val: &str, field: &str, br: &str, src: &str| {
        let key = format!("{}|{}", ty, val.to_lowercase());
        if val.trim().is_empty() || !seen.insert(key) {
            return;
        }
        let mut rr = Row::new();
        rr.set("Type", Cell::text(ty.to_string()));
        rr.set("Valeur", Cell::text(val.to_string()));
        rr.set("Champ", Cell::text(field.to_string()));
        rr.set("Navigateur", Cell::text(br.to_string()));
        rr.set("Source", Cell::text(src.to_string()));
        out.push(rr);
    };
    for art in arts {
        for cat in &art.categories {
            match cat.name.as_str() {
                "Saisie auto (Autofill)" | "Historique de formulaires" => {
                    for r in &cat.rows {
                        let field = r.get("Champ").map(|c| c.as_search()).unwrap_or_default();
                        let value = r.get("Valeur").map(|c| c.as_search()).unwrap_or_default();
                        if let Some(ty) =
                            classify_identity(&field, &value, &re_email, &re_iban, &re_immat)
                        {
                            push(ty, &value, &field, &art.browser, &cat.name);
                        }
                    }
                }
                "Compte(s) connecte(s)" => {
                    for r in &cat.rows {
                        if let Some(email) = r.get("Email").map(|c| c.as_search()) {
                            if !email.is_empty() {
                                push("Email", &email, "Compte connecte", &art.browser, &cat.name);
                            }
                        }
                        if let Some(nom) = r.get("Nom").map(|c| c.as_search()) {
                            if !nom.is_empty() {
                                push("Nom / Prenom", &nom, "Compte connecte", &art.browser, &cat.name);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out.sort_by(|a, b| {
        a.get("Type")
            .map(|c| c.as_search())
            .unwrap_or_default()
            .cmp(&b.get("Type").map(|c| c.as_search()).unwrap_or_default())
    });
    out
}

/// Agrège tous les téléchargements de tous les navigateurs.
pub fn build_downloads(arts: &[Artifact]) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for art in arts {
        for cat in &art.categories {
            if cat.name != "Telechargements" {
                continue;
            }
            for r in &cat.rows {
                let date = row_first_time(r, &cat.columns);
                let url = r
                    .get("URL")
                    .or_else(|| r.get("URL onglet"))
                    .map(|c| c.as_search())
                    .unwrap_or_default();
                let file = r
                    .get("Fichier")
                    .or_else(|| r.get("Fichier cible"))
                    .map(|c| c.as_search())
                    .unwrap_or_default();
                let size = r
                    .get("Total (o)")
                    .or_else(|| r.get("Recu (o)"))
                    .cloned()
                    .unwrap_or(Cell::Null);
                let mut rr = Row::new();
                rr.set("Navigateur", Cell::text(art.browser.clone()));
                rr.set("Date", date.map(Cell::Time).unwrap_or(Cell::Null));
                rr.set("Fichier", Cell::text(file));
                rr.set("URL", Cell::text(url));
                rr.set("Taille (o)", size);
                out.push(rr);
            }
        }
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.get("Date").and_then(|c| c.time_ms()).unwrap_or(i64::MIN)));
    out
}

// --- Géolocalisations / adresses recherchées -------------------------------

const MAP_HOSTS: &[(&str, &[&str])] = &[
    ("maps.google", &["q", "query", "daddr", "saddr"]),
    ("google.com/maps", &["q", "query"]),
    ("waze.com", &["q", "latlng"]),
    ("mappy.com", &["q"]),
    ("openstreetmap.org", &["query", "mlat"]),
    ("bing.com/maps", &["q", "where1"]),
    ("geoportail", &["c", "q"]),
    ("ratp.fr", &["q"]),
    ("maps.apple.com", &["q", "address", "daddr"]),
];

/// Extrait les géolocalisations : requêtes cartographiques et adresses saisies.
pub fn build_geolocation(arts: &[Artifact]) -> Vec<Row> {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Row> = Vec::new();
    for art in arts {
        for cat in &art.categories {
            // adresses issues de l'autofill / formulaires
            if cat.name == "Saisie auto (Autofill)" || cat.name == "Historique de formulaires" {
                for r in &cat.rows {
                    let field = r.get("Champ").map(|c| c.as_search()).unwrap_or_default().to_lowercase();
                    let value = r.get("Valeur").map(|c| c.as_search()).unwrap_or_default();
                    if value.len() >= 4
                        && ["address", "adresse", "street", "rue", "ville", "city", "postal", "zip"]
                            .iter()
                            .any(|k| field.contains(k))
                        && seen.insert(format!("adr|{}", value.to_lowercase()))
                    {
                        let mut rr = Row::new();
                        rr.set("Type", Cell::text("Adresse saisie"));
                        rr.set("Valeur", Cell::text(value));
                        rr.set("Navigateur", Cell::text(art.browser.clone()));
                        rr.set("Date", Cell::Null);
                        out.push(rr);
                    }
                }
            }
            // requêtes cartographiques dans les URLs
            if !cat.columns.iter().any(|c| c == "URL") {
                continue;
            }
            for r in &cat.rows {
                let url = r.get("URL").map(|c| c.as_search()).unwrap_or_default();
                if url.is_empty() {
                    continue;
                }
                let low = url.to_lowercase();
                let hit = MAP_HOSTS.iter().find(|(h, _)| low.contains(h));
                if let Some((_, keys)) = hit {
                    let place = query_param(&url, keys).unwrap_or_else(|| url.clone());
                    if seen.insert(format!("map|{}", place.to_lowercase())) {
                        let date = row_first_time(r, &cat.columns);
                        let mut rr = Row::new();
                        rr.set("Type", Cell::text("Recherche carto"));
                        rr.set("Valeur", Cell::text(place));
                        rr.set("Navigateur", Cell::text(art.browser.clone()));
                        rr.set("Date", date.map(Cell::Time).unwrap_or(Cell::Null));
                        out.push(rr);
                    }
                }
            }
        }
    }
    out
}

// --- VPN / anonymiseurs utilisés -------------------------------------------

const VPN_HOSTS: &[(&str, &str)] = &[
    ("nordvpn.com", "NordVPN"),
    ("expressvpn.com", "ExpressVPN"),
    ("protonvpn.com", "Proton VPN"),
    ("cyberghostvpn.com", "CyberGhost"),
    ("surfshark.com", "Surfshark"),
    ("mullvad.net", "Mullvad"),
    ("windscribe.com", "Windscribe"),
    ("tunnelbear.com", "TunnelBear"),
    ("privateinternetaccess.com", "Private Internet Access"),
    ("hide.me", "hide.me"),
    ("ipvanish.com", "IPVanish"),
    ("purevpn.com", "PureVPN"),
    ("atlasvpn.com", "Atlas VPN"),
    ("hotspotshield.com", "Hotspot Shield"),
    ("vyprvpn.com", "VyprVPN"),
    ("ProtonVPN", "Proton VPN"),
];

/// Repère les VPN / anonymiseurs consultés dans l'historique.
pub fn build_vpn(arts: &[Artifact]) -> Vec<Row> {
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Row> = Vec::new();
    for art in arts {
        for cat in &art.categories {
            if !cat.columns.iter().any(|c| c == "URL") {
                continue;
            }
            for r in &cat.rows {
                let url = r.get("URL").map(|c| c.as_search()).unwrap_or_default();
                if url.is_empty() {
                    continue;
                }
                let host = host_of(&url);
                if let Some((_, name)) = VPN_HOSTS.iter().find(|(h, _)| host.contains(h)) {
                    if seen.insert(format!("{}|{}", art.browser, name)) {
                        let date = row_first_time(r, &cat.columns);
                        let mut rr = Row::new();
                        rr.set("Service", Cell::text(name.to_string()));
                        rr.set("Navigateur", Cell::text(art.browser.clone()));
                        rr.set("URL", Cell::text(url));
                        rr.set("Date", date.map(Cell::Time).unwrap_or(Cell::Null));
                        out.push(rr);
                    }
                }
            }
        }
    }
    out
}

// --- Histogramme d'activité (par heure et par jour de semaine, en UTC) ------

/// Répartition temporelle des événements datés (timeline), par heure de la
/// journée et par jour de la semaine, avec une barre ASCII proportionnelle.
pub fn build_histogram(arts: &[Artifact]) -> Vec<Row> {
    use chrono::{Datelike, TimeZone, Timelike, Utc};
    let events = build_timeline(arts, &all_event_types());
    let mut hours = [0i64; 24];
    let mut days = [0i64; 7]; // 0=lundi .. 6=dimanche
    for e in &events {
        if let Some(ms) = e.get("Date").and_then(|c| c.time_ms()) {
            if let Some(dt) = Utc.timestamp_millis_opt(ms).single() {
                hours[dt.hour() as usize] += 1;
                days[dt.weekday().num_days_from_monday() as usize] += 1;
            }
        }
    }
    let bar = |n: i64, max: i64| -> String {
        if max <= 0 {
            return String::new();
        }
        let len = ((n as f64 / max as f64) * 30.0).round() as usize;
        "\u{2588}".repeat(len)
    };
    let mut out: Vec<Row> = Vec::new();
    let hmax = *hours.iter().max().unwrap_or(&0);
    for (h, n) in hours.iter().enumerate() {
        let mut rr = Row::new();
        rr.set("Categorie", Cell::text("Heure (UTC)"));
        rr.set("Creneau", Cell::text(format!("{:02}h", h)));
        rr.set("Evenements", Cell::Int(*n));
        rr.set("Repartition", Cell::text(bar(*n, hmax)));
        out.push(rr);
    }
    let noms = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
    let dmax = *days.iter().max().unwrap_or(&0);
    for (d, n) in days.iter().enumerate() {
        let mut rr = Row::new();
        rr.set("Categorie", Cell::text("Jour (UTC)"));
        rr.set("Creneau", Cell::text(noms[d]));
        rr.set("Evenements", Cell::Int(*n));
        rr.set("Repartition", Cell::text(bar(*n, dmax)));
        out.push(rr);
    }
    out
}

// --- Galerie des médias en cache (images / vidéos par signature) ------------

/// Un média extrait d'un cache (données en base64, prêtes pour un data: URI).
#[derive(serde::Serialize)]
pub struct MediaItem {
    pub name: String,
    pub mime: String,
    pub size: usize,
    pub data: String, // base64
}

pub const MEDIA_MAX_ITEMS: usize = 400;
pub const MEDIA_MAX_BYTES: usize = 6 * 1024 * 1024; // par fichier
pub const MEDIA_TOTAL_CAP: usize = 90 * 1024 * 1024; // cumul base64

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(B64[(b0 >> 2) as usize] as char);
        out.push(B64[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn b64_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Décode une chaîne base64 (ignore les caractères non base64).
pub fn b64_decode(s: &str) -> Vec<u8> {
    let vals: Vec<u8> = s.bytes().filter_map(b64_val).collect();
    let mut out = Vec::with_capacity(vals.len() / 4 * 3);
    for ch in vals.chunks(4) {
        if ch.len() < 2 {
            break;
        }
        let n = ((ch[0] as u32) << 18)
            | ((ch[1] as u32) << 12)
            | ((*ch.get(2).unwrap_or(&0) as u32) << 6)
            | (*ch.get(3).unwrap_or(&0) as u32);
        out.push((n >> 16) as u8);
        if ch.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if ch.len() > 3 {
            out.push(n as u8);
        }
    }
    out
}

/// Reconnaît le type d'un média d'après ses premiers octets.
fn sniff_media(d: &[u8]) -> Option<&'static str> {
    if d.len() < 12 {
        return None;
    }
    if d[0..3] == [0xFF, 0xD8, 0xFF] {
        return Some("image/jpeg");
    }
    if d[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some("image/png");
    }
    if &d[0..6] == b"GIF87a" || &d[0..6] == b"GIF89a" {
        return Some("image/gif");
    }
    if &d[0..4] == b"RIFF" && &d[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if &d[0..2] == b"BM" {
        return Some("image/bmp");
    }
    if &d[4..8] == b"ftyp" {
        return Some("video/mp4");
    }
    if d[0..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        return Some("video/webm");
    }
    None
}

/// Parcourt un dossier (cache ou profil) et extrait les images/vidéos
/// détectées par signature, prêtes à l'affichage (data: URI).
pub fn carve_cache_media(folder: &Path, max_items: usize, max_bytes: usize) -> Vec<MediaItem> {
    use std::collections::HashSet;
    let mut out: Vec<MediaItem> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut total = 0usize;
    if let Ok(files) = walk(folder) {
        for p in files {
            if out.len() >= max_items || total >= MEDIA_TOTAL_CAP {
                break;
            }
            let data = match read_capped(&p, max_bytes) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let mime = match sniff_media(&data) {
                Some(m) => m,
                None => continue,
            };
            let head: Vec<u8> = data.iter().take(24).copied().collect();
            let key = format!("{}:{}", data.len(), hex::encode(&head));
            if !seen.insert(key) {
                continue;
            }
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            let b64 = b64_encode(&data);
            total += b64.len();
            out.push(MediaItem {
                name,
                mime: mime.to_string(),
                size: data.len(),
                data: b64,
            });
        }
    }
    out
}

// --- Extraction des pages HTML en cache (code source, lecture seule) --------

#[derive(serde::Serialize)]
pub struct HtmlResource {
    pub kind: String,
    pub url: String,
}

#[derive(serde::Serialize)]
pub struct HtmlItem {
    pub name: String,
    pub title: String,
    pub size: usize,
    pub source: String,
    pub resources: Vec<HtmlResource>,
}

pub const HTML_MAX_ITEMS: usize = 200;
pub const HTML_READ_BYTES: usize = 4 * 1024 * 1024; // lecture brute (avant décompression)
pub const HTML_SOURCE_CAP: usize = 1_500_000; // code source affiché

/// Décompresse un corps gzip/zlib si besoin (sinon renvoie tel quel).
fn maybe_decompress(data: &[u8]) -> Vec<u8> {
    use std::io::Read;
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        let mut d = flate2::read::GzDecoder::new(data);
        let mut out = Vec::new();
        if d.read_to_end(&mut out).is_ok() && !out.is_empty() {
            return out;
        }
    }
    if data.len() >= 2 && data[0] == 0x78 && matches!(data[1], 0x01 | 0x9c | 0xda) {
        let mut d = flate2::read::ZlibDecoder::new(data);
        let mut out = Vec::new();
        if d.read_to_end(&mut out).is_ok() && !out.is_empty() {
            return out;
        }
    }
    data.to_vec()
}

fn looks_html(s: &str) -> bool {
    let head: String = s.chars().take(1024).collect::<String>().to_ascii_lowercase();
    head.contains("<!doctype html")
        || head.contains("<html")
        || head.contains("<head")
        || head.contains("<body")
}

fn classify_resource(url: &str) -> &'static str {
    let u = url.split(['?', '#']).next().unwrap_or(url).to_ascii_lowercase();
    if u.ends_with(".js") || u.ends_with(".mjs") {
        "script"
    } else if u.ends_with(".css") {
        "style"
    } else if u.ends_with(".png") || u.ends_with(".jpg") || u.ends_with(".jpeg")
        || u.ends_with(".gif") || u.ends_with(".webp") || u.ends_with(".svg")
        || u.ends_with(".ico") || u.ends_with(".bmp")
    {
        "image"
    } else if u.ends_with(".mp4") || u.ends_with(".webm") || u.ends_with(".m3u8")
        || u.ends_with(".mpd")
    {
        "media"
    } else if u.ends_with(".woff") || u.ends_with(".woff2") || u.ends_with(".ttf")
        || u.ends_with(".otf")
    {
        "police"
    } else {
        "lien"
    }
}

/// Parcourt un dossier (cache/profil) et extrait les pages HTML : code source
/// (décompressé si besoin, jamais exécuté) et ressources liées (src/href/url()).
pub fn carve_cache_html(folder: &Path, max_items: usize) -> Vec<HtmlItem> {
    use std::collections::HashSet;
    let re_attr = Regex::new(r#"(?i)(?:src|href)\s*=\s*["']([^"'>\s]{1,2048})["']"#).unwrap();
    let re_css = Regex::new(r#"(?i)url\(\s*['"]?([^'")]{1,2048})"#).unwrap();
    let re_title = Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap();
    let mut out: Vec<HtmlItem> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    if let Ok(files) = walk(folder) {
        for p in files {
            if out.len() >= max_items {
                break;
            }
            let raw = match read_capped(&p, HTML_READ_BYTES) {
                Ok(d) => d,
                Err(_) => continue,
            };
            // ignore les binaires évidents (images/vidéos) avant décompression
            if sniff_media(&raw).is_some() {
                continue;
            }
            let body = maybe_decompress(&raw);
            let text = String::from_utf8_lossy(&body);
            if !looks_html(&text) {
                continue;
            }
            let key = format!(
                "{}:{}",
                text.len(),
                text.chars().take(80).collect::<String>()
            );
            if !seen.insert(key) {
                continue;
            }
            let title = re_title
                .captures(&text)
                .map(|c| c[1].trim().chars().take(160).collect::<String>())
                .unwrap_or_default();
            let mut resources: Vec<HtmlResource> = Vec::new();
            let mut rseen: HashSet<String> = HashSet::new();
            for caps in re_attr.captures_iter(&text).chain(re_css.captures_iter(&text)) {
                let url = caps[1].trim().to_string();
                if url.is_empty() || url.starts_with("data:") || url.starts_with("javascript:") {
                    continue;
                }
                if rseen.insert(url.clone()) {
                    resources.push(HtmlResource {
                        kind: classify_resource(&url).to_string(),
                        url,
                    });
                }
                if resources.len() >= 1000 {
                    break;
                }
            }
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            let mut source = text.into_owned();
            if source.len() > HTML_SOURCE_CAP {
                let mut cap = HTML_SOURCE_CAP;
                while cap < source.len() && !source.is_char_boundary(cap) {
                    cap += 1;
                }
                source.truncate(cap);
                source.push_str("\n\n<!-- [tronque par NavForensics] -->");
            }
            out.push(HtmlItem {
                name,
                title,
                size: body.len(),
                source,
                resources,
            });
        }
    }
    out
}

pub fn carve_cache_urls(folder: &Path, label: Option<String>) -> Artifact {
    use std::collections::BTreeMap;
    let mut urls: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if let Ok(entries) = walk(folder) {
        for p in entries {
            if let Ok(data) = read_capped(&p, 8_000_000) {
                let name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                for u in crate::sessions::carve_urls(&data) {
                    urls.entry(u).or_default().insert(name.clone());
                }
            }
        }
    }
    let rows: Vec<Row> = urls
        .into_iter()
        .map(|(u, files)| {
            let mut rr = Row::new();
            rr.set("URL", Cell::text(u));
            let names: Vec<String> = files.into_iter().take(5).collect();
            rr.set("Fichiers", Cell::text(names.join(", ")));
            rr
        })
        .collect();
    let cats = if rows.is_empty() {
        vec![]
    } else {
        vec![Category::new("URLs en cache (contenu charge)", &["URL", "Fichiers"], rows)]
    };
    let name = label.unwrap_or_else(|| {
        folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("cache")
            .to_string()
    });
    Artifact {
        evidence: name,
        path: folder.to_string_lossy().to_string(),
        sha256: "— (dossier, carving)".into(),
        browser: "Cache (carving)".into(),
        kind: "Cache".into(),
        categories: cats,
        notes: vec![
            "Reconstruction par carving : preuve que le contenu a ete charge/mis en cache. \
             A verifier sur la source."
                .into(),
        ],
    }
}

fn walk(dir: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    Ok(out)
}

fn read_capped(p: &Path, cap: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(p)?;
    let mut buf = vec![0u8; cap];
    let n = f.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}
