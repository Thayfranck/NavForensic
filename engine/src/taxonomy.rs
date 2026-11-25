//! Taxonomie de presentation : regroupement Navigateur -> sous-categorie, et
//! detection heuristique des liens signales (usage non conventionnel).
//!
//! AVERTISSEMENT : les signalements sont des INDICATEURS AUTOMATIQUES bases sur
//! des motifs d'URL. Ils ne constituent pas une preuve d'infraction et doivent
//! etre verifies humainement. L'identification de contenus illegaux (en
//! particulier impliquant des mineurs) releve d'outils certifies (correspondance
//! d'empreintes sur bases de reference) et du cadre judiciaire, jamais d'une
//! simple detection par mots-cles.

use crate::model::{Artifact, Cell, Row};
use std::collections::HashSet;

/// Ordre d'affichage des sous-categories (le bucket "Liens signales" est ajoute
/// a part, en fin de liste, car il est calcule et non issu d'un parseur).
pub const SUBCATEGORY_ORDER: &[&str] = &[
    "Historique & navigation",
    "Favoris",
    "Comptes utilisateurs",
    "Telechargements",
    "Cookies & formulaires",
    "Configuration",
];

/// Libelle de la sous-categorie "liens signales" (bucket calcule).
pub const FLAGGED_SUB: &str = "Liens signales";

/// Colonnes de la vue des liens signales.
pub const FLAGGED_COLUMNS: &[&str] =
    &["Navigateur", "Source", "Signalement", "Motif", "URL", "Titre"];

/// Associe le nom d'une categorie-source a une sous-categorie normalisee.
pub fn sub_category(cat_name: &str) -> &'static str {
    match cat_name {
        "Marque-pages" => "Favoris",
        "Identifiants (metadonnees, sans mot de passe)" | "Compte(s) connecte(s)" => {
            "Comptes utilisateurs"
        }
        "Telechargements" => "Telechargements",
        "Cookies" | "Historique de formulaires" | "Saisie auto (Autofill)" => {
            "Cookies & formulaires"
        }
        "Configuration" | "Extensions" => "Configuration",
        // Historique (URLs), Visites, Sites les plus visites, Saisies omnibox,
        // Termes de recherche, URLs en cache, Onglets, Session, Favicons...
        _ => "Historique & navigation",
    }
}

// --- Detection heuristique des liens signales -------------------------------

const ADULT_HOSTS: &[&str] = &[
    "pornhub", "xvideos", "xnxx", "redtube", "youporn", "xhamster", "brazzers",
    "onlyfans", "chaturbate", "stripchat", "livejasmin", "adultfriendfinder",
    "spankbang", "tube8", "youjizz", "motherless", "rule34",
];
const ADULT_KEYWORDS: &[&str] = &[
    "porn", "xxx", "/sex", "/sexe", "escort", "hentai", "camgirl", "nsfw", "fetish",
];
const SHORTENERS: &[&str] = &[
    "bit.ly", "tinyurl.com", "t.co", "is.gd", "goo.gl", "ow.ly", "buff.ly",
    "cutt.ly", "rb.gy", "shorturl.at", "adf.ly",
];
const SUSPECT_KEYWORDS: &[&str] = &[
    "crack", "keygen", "warez", "nulled", "serialkey", "cardable", "combolist",
    "darkweb", "hacktool",
];
const EXE_EXT: &[&str] = &[
    ".exe", ".scr", ".msi", ".bat", ".cmd", ".ps1", ".vbs", ".jar", ".apk",
    ".dll", ".lnk", ".hta",
];

fn host_of(url: &str) -> String {
    let after = url.trim().split("://").nth(1).unwrap_or(url);
    let host = after.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host); // enleve userinfo@
    let host = host.split(':').next().unwrap_or(host); // enleve :port
    host.to_ascii_lowercase()
}

fn is_ip_host(h: &str) -> bool {
    let parts: Vec<&str> = h.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 3
                && p.chars().all(|c| c.is_ascii_digit())
                && p.parse::<u16>().map(|n| n <= 255).unwrap_or(false)
        })
}

/// Classe une URL. Renvoie (type de signalement, motif) ou None si rien a signaler.
pub fn classify_url(url: &str) -> Option<(&'static str, String)> {
    if url.is_empty() {
        return None;
    }
    let host = host_of(url);
    let low = url.to_ascii_lowercase();
    let path = low.split(['?', '#']).next().unwrap_or(&low);

    // 1. Tor (.onion) — priorite maximale
    if host.ends_with(".onion") {
        return Some(("Tor (.onion)", "Service cache Tor (reseau anonymise)".into()));
    }
    // 2. Contenu a caractere adulte
    for h in ADULT_HOSTS {
        if host.contains(h) {
            return Some(("Adulte", format!("Domaine a caractere adulte : {}", h)));
        }
    }
    for k in ADULT_KEYWORDS {
        if low.contains(k) {
            return Some((
                "Adulte",
                format!("Mot-cle a caractere adulte : {}", k.trim_start_matches('/')),
            ));
        }
    }
    // 3. Suspect / potentiellement malveillant
    if is_ip_host(&host) {
        return Some(("Suspect", "Connexion directe par adresse IP".into()));
    }
    if host.contains("xn--") {
        return Some(("Suspect", "Domaine punycode (risque d'homographe)".into()));
    }
    for s in SHORTENERS {
        if host == *s || host.ends_with(&format!(".{}", s)) {
            return Some(("Suspect", format!("Raccourcisseur d'URL : {}", s)));
        }
    }
    for e in EXE_EXT {
        if path.ends_with(e) {
            return Some(("Suspect", format!("Telechargement executable direct ({})", e)));
        }
    }
    for k in SUSPECT_KEYWORDS {
        if low.contains(k) {
            return Some(("Suspect", format!("Mot-cle sensible : {}", k)));
        }
    }
    None
}

/// Parcourt tous les artefacts (filtres par navigateur si `browser` est fourni),
/// signale les URLs remarquables, et deduplique par (navigateur + URL).
pub fn build_flagged(arts: &[Artifact], browser: Option<&str>) -> Vec<Row> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Row> = Vec::new();
    for a in arts {
        if let Some(b) = browser {
            if a.browser != b {
                continue;
            }
        }
        for c in &a.categories {
            if !c.columns.iter().any(|col| col == "URL") {
                continue;
            }
            for r in &c.rows {
                let url = match r.get("URL") {
                    Some(cell) => cell.as_search(),
                    None => continue,
                };
                if url.is_empty() {
                    continue;
                }
                if let Some((sig, motif)) = classify_url(&url) {
                    if !seen.insert(format!("{}|{}", a.browser, url)) {
                        continue;
                    }
                    let title = r.get("Titre").map(|c| c.as_search()).unwrap_or_default();
                    let mut rr = Row::new();
                    rr.set("Navigateur", Cell::text(a.browser.clone()));
                    rr.set("Source", Cell::text(a.evidence.clone()));
                    rr.set("Signalement", Cell::text(sig));
                    rr.set("Motif", Cell::text(motif));
                    rr.set("URL", Cell::text(url));
                    rr.set("Titre", Cell::text(title));
                    out.push(rr);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_onion_adult_suspect() {
        assert_eq!(
            classify_url("http://abcdefabcdef1234.onion/market").map(|x| x.0),
            Some("Tor (.onion)")
        );
        assert_eq!(
            classify_url("https://www.pornhub.com/view").map(|x| x.0),
            Some("Adulte")
        );
        assert_eq!(
            classify_url("http://185.220.101.5/payload.exe").map(|x| x.0),
            Some("Suspect") // IP directe (teste avant l'extension)
        );
        assert_eq!(
            classify_url("https://files.example.com/setup.exe").map(|x| x.0),
            Some("Suspect")
        );
        assert_eq!(classify_url("https://www.wikipedia.org/wiki/Rust"), None);
        assert_eq!(classify_url(""), None);
    }

    #[test]
    fn sub_category_mapping() {
        assert_eq!(sub_category("Marque-pages"), "Favoris");
        assert_eq!(sub_category("Visites"), "Historique & navigation");
        assert_eq!(
            sub_category("Identifiants (metadonnees, sans mot de passe)"),
            "Comptes utilisateurs"
        );
        assert_eq!(sub_category("Cookies"), "Cookies & formulaires");
    }
}
