//! Import d'une liste de surveillance (IOC) et recherche des correspondances.
//!
//! L'enquêteur fournit une liste d'indicateurs (un par ligne) : domaines,
//! URLs, courriels, adresses de cryptomonnaie, numéros, mots-clés. Le module
//! en déduit le type (ou le prend tel quel), puis recherche chaque indicateur
//! dans toutes les sources chargées et rassemble les correspondances.

use crate::model::{Artifact, Cell, Row};
use std::collections::HashSet;

pub const IOC_COLUMNS: &[&str] =
    &["Indicateur", "Type", "Navigateur", "Catégorie", "Colonne", "Valeur"];

/// Nombre maximal de correspondances (garde-fou).
const MAX_MATCHES: usize = 100_000;

#[derive(Clone, Debug)]
pub struct Ioc {
    pub value: String,
    pub kind: String,
}

/// Analyse le texte d'une liste de surveillance (un indicateur par ligne).
/// Les lignes vides et celles commençant par `#` sont ignorées. Une ligne
/// peut s'écrire `valeur` (type déduit) ou `valeur | type` (type imposé).
pub fn parse_iocs(text: &str) -> Vec<Ioc> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let (value, kind) = match l.split_once('|') {
            Some((v, k)) => (v.trim().to_string(), k.trim().to_string()),
            None => (l.to_string(), String::new()),
        };
        if value.is_empty() {
            continue;
        }
        let kind = if kind.is_empty() {
            detect_kind(&value)
        } else {
            kind
        };
        out.push(Ioc { value, kind });
    }
    out
}

/// Déduit le type d'un indicateur à partir de sa forme.
fn detect_kind(v: &str) -> String {
    let low = v.to_ascii_lowercase();
    if low.contains("://") {
        "URL".into()
    } else if v.contains('@') && v.contains('.') && !v.contains(' ') {
        "Email".into()
    } else if looks_crypto(v) {
        "Adresse crypto".into()
    } else if is_mostly_digits(v) && v.chars().filter(|c| c.is_ascii_digit()).count() >= 7 {
        "Numéro".into()
    } else if v.contains('.') && !v.contains(' ') && !v.contains('/') {
        "Domaine".into()
    } else {
        "Mot-clé".into()
    }
}

fn looks_crypto(v: &str) -> bool {
    // Heuristique légère (le typage précis relève du module crypto).
    if let Some(rest) = v.strip_prefix("0x") {
        return rest.len() == 40 && rest.chars().all(|c| c.is_ascii_hexdigit());
    }
    (v.starts_with("bc1") || v.starts_with("ltc1")) && v.len() >= 20
        || (matches!(v.chars().next(), Some('1' | '3' | 'T' | 'L' | 'M' | 'D'))
            && v.len() >= 26
            && v.len() <= 95
            && v.chars().all(|c| c.is_ascii_alphanumeric()))
}

fn is_mostly_digits(v: &str) -> bool {
    let digits = v.chars().filter(|c| c.is_ascii_digit()).count();
    let others = v
        .chars()
        .filter(|c| !c.is_ascii_digit() && !matches!(c, ' ' | '+' | '-' | '.' | '(' | ')'))
        .count();
    digits >= 1 && others == 0
}

/// Recherche toutes les correspondances des indicateurs dans les sources.
pub fn build_ioc_matches(arts: &[Artifact], iocs: &[Ioc]) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    let mut seen: HashSet<(String, String, String, String)> = HashSet::new();
    // Indicateurs pré-normalisés en minuscules.
    let needles: Vec<(String, &Ioc)> = iocs
        .iter()
        .map(|i| (i.value.to_ascii_lowercase(), i))
        .filter(|(n, _)| !n.is_empty())
        .collect();

    for art in arts {
        for cat in &art.categories {
            for r in &cat.rows {
                for (col, cell) in r.values.iter() {
                    let text = match cell {
                        Cell::Text(s) => s,
                        _ => continue,
                    };
                    let low = text.to_ascii_lowercase();
                    for (needle, ioc) in &needles {
                        if low.contains(needle.as_str()) {
                            let key = (
                                ioc.value.clone(),
                                art.browser.clone(),
                                col.clone(),
                                text.clone(),
                            );
                            if seen.insert(key) {
                                let mut row = Row::new();
                                row.set("Indicateur", Cell::text(ioc.value.clone()));
                                row.set("Type", Cell::text(ioc.kind.clone()));
                                row.set("Navigateur", Cell::text(art.browser.clone()));
                                row.set("_chemin", Cell::text(art.path.clone()));
                                row.set("Catégorie", Cell::text(cat.name.clone()));
                                row.set("Colonne", Cell::text(col.clone()));
                                row.set("Valeur", Cell::text(text.clone()));
                                out.push(row);
                                if out.len() >= MAX_MATCHES {
                                    return out;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}
