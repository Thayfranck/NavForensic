//! Extraction d'artefacts liés aux crypto-actifs.
//!
//! Parcourt toutes les sources chargées pour en tirer trois types d'éléments :
//! - les **adresses** de cryptomonnaies (BTC, ETH, LTC, Tron, Monero, XRP,
//!   Dogecoin, Bitcoin Cash) repérées dans les URLs, titres, champs de
//!   formulaire, recherches et téléchargements ;
//! - les **plateformes** fréquentées : places de marché (exchanges), wallets
//!   web et explorateurs de blocs, détectées d'après les domaines ;
//! - les **phrases mnémoniques possibles** (seed BIP-39) repérées dans les
//!   champs de saisie automatique / historique de formulaires.
//!
//! Tous ces éléments sont des indices à recouper : les adresses sont validées
//! par motif (préfixe, longueur, jeu de caractères) mais non confrontées à la
//! chaîne ; les phrases mnémoniques sont signalées par heuristique.

use crate::model::{Artifact, Cell, Row};
use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

pub const CRYPTO_COLUMNS: &[&str] = &["Type", "Valeur", "Détail", "Navigateur", "Source"];

/// Plateformes reconnues par fragment de domaine : (fragment, catégorie).
const PLATFORMS: &[(&str, &str)] = &[
    // Places de marché (exchanges)
    ("binance.", "Exchange"),
    ("coinbase.", "Exchange"),
    ("kraken.", "Exchange"),
    ("kucoin.", "Exchange"),
    ("bitget.", "Exchange"),
    ("okx.", "Exchange"),
    ("bybit.", "Exchange"),
    ("crypto.com", "Exchange"),
    ("bitstamp.", "Exchange"),
    ("gemini.com", "Exchange"),
    ("huobi.", "Exchange"),
    ("htx.com", "Exchange"),
    ("gate.io", "Exchange"),
    ("mexc.com", "Exchange"),
    ("bitfinex.", "Exchange"),
    ("paymium.com", "Exchange"),
    ("coinhouse.com", "Exchange"),
    ("bitpanda.", "Exchange"),
    // Wallets
    ("metamask.io", "Wallet"),
    ("phantom.app", "Wallet"),
    ("trustwallet.", "Wallet"),
    ("ledger.com", "Wallet"),
    ("trezor.io", "Wallet"),
    ("exodus.com", "Wallet"),
    ("electrum.org", "Wallet"),
    ("blockchain.com", "Wallet"),
    ("myetherwallet.", "Wallet"),
    ("rainbow.me", "Wallet"),
    // Explorateurs de blocs
    ("etherscan.io", "Explorateur"),
    ("blockchain.info", "Explorateur"),
    ("blockchair.com", "Explorateur"),
    ("blockstream.info", "Explorateur"),
    ("mempool.space", "Explorateur"),
    ("tronscan.", "Explorateur"),
    ("solscan.io", "Explorateur"),
    ("bscscan.com", "Explorateur"),
    ("polygonscan.com", "Explorateur"),
    ("btcscan.org", "Explorateur"),
    ("xrpscan.com", "Explorateur"),
    // Mélangeurs / services de confidentialité (à signaler)
    ("tornado.cash", "Mixeur"),
    ("wasabiwallet.", "Mixeur"),
    ("samouraiwallet.", "Mixeur"),
];

/// Motifs d'adresses, du plus spécifique au plus générique (priorité).
fn address_patterns() -> &'static [(&'static str, Regex)] {
    static PATS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    PATS.get_or_init(|| {
        vec![
            ("Adresse Ethereum (EVM)", Regex::new(r"\b0x[0-9a-fA-F]{40}\b").unwrap()),
            ("Adresse Monero", Regex::new(r"\b[48][0-9AB][1-9A-HJ-NP-Za-km-z]{93}\b").unwrap()),
            ("Adresse Bitcoin (bech32)", Regex::new(r"\bbc1[ac-hj-np-z02-9]{11,71}\b").unwrap()),
            ("Adresse Litecoin (bech32)", Regex::new(r"\bltc1[ac-hj-np-z02-9]{11,71}\b").unwrap()),
            ("Adresse Bitcoin Cash", Regex::new(r"\b(?:bitcoincash:)?[qp][a-z0-9]{41}\b").unwrap()),
            ("Adresse Tron", Regex::new(r"\bT[1-9A-HJ-NP-Za-km-z]{33}\b").unwrap()),
            ("Adresse Dogecoin", Regex::new(r"\bD[5-9A-HJ-NP-U][1-9A-HJ-NP-Za-km-z]{32}\b").unwrap()),
            ("Adresse Litecoin", Regex::new(r"\b[LM][1-9A-HJ-NP-Za-km-z]{26,33}\b").unwrap()),
            ("Adresse Bitcoin", Regex::new(r"\b[13][1-9A-HJ-NP-Za-km-z]{25,34}\b").unwrap()),
            ("Adresse XRP (Ripple)", Regex::new(r"\br[1-9A-HJ-NP-Za-km-z]{24,34}\b").unwrap()),
        ]
    })
}

/// Point d'entrée : un seul tableau « Crypto-actifs », toutes sources.
pub fn build_crypto(arts: &[Artifact]) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    // Dédoublonnage par (type, valeur) — une même adresse n'apparaît qu'une fois.
    let mut seen: HashSet<(String, String)> = HashSet::new();

    for art in arts {
        for cat in &art.categories {
            for r in &cat.rows {
                scan_row(art, cat.name.as_str(), r, &mut out, &mut seen);
            }
        }
    }
    out
}

fn scan_row(
    art: &Artifact,
    cat: &str,
    r: &Row,
    out: &mut Vec<Row>,
    seen: &mut HashSet<(String, String)>,
) {
    // Chaînes texte de la ligne.
    let texts: Vec<String> = r
        .values
        .values()
        .filter_map(|c| match c {
            Cell::Text(s) => Some(s.clone()),
            _ => None,
        })
        .collect();

    for s in &texts {
        // Adresses.
        for (label, re) in address_patterns() {
            for m in re.find_iter(s) {
                let val = m.as_str().to_string();
                push(out, seen, label, &val, "Adresse", art, cat);
            }
        }
        // Plateformes (une par domaine rencontré).
        let low = s.to_ascii_lowercase();
        for (frag, kind) in PLATFORMS {
            if low.contains(frag) {
                push(out, seen, kind, frag.trim_end_matches('.'), "Plateforme", art, cat);
            }
        }
    }

    // Phrases mnémoniques : uniquement dans les champs de saisie / formulaires.
    if cat.contains("Autofill") || cat.contains("formulaire") || cat.contains("Historique de formulaires") {
        if let Some(Cell::Text(v)) = r.get("Valeur") {
            if looks_like_mnemonic(v) {
                push(out, seen, "Phrase mnémonique possible", v, "Seed (à vérifier)", art, cat);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push(
    out: &mut Vec<Row>,
    seen: &mut HashSet<(String, String)>,
    typ: &str,
    val: &str,
    detail: &str,
    art: &Artifact,
    cat: &str,
) {
    if !seen.insert((typ.to_string(), val.to_string())) {
        return;
    }
    let mut row = Row::new();
    row.set("Type", Cell::text(typ));
    row.set("Valeur", Cell::text(val));
    row.set("Détail", Cell::text(detail));
    row.set("Navigateur", Cell::text(art.browser.clone()));
    row.set("Source", Cell::text(cat));
    out.push(row);
}

/// Heuristique BIP-39 : 12/15/18/21/24 mots alphabétiques minuscules courts.
fn looks_like_mnemonic(s: &str) -> bool {
    let words: Vec<&str> = s.split_whitespace().collect();
    if !matches!(words.len(), 12 | 15 | 18 | 21 | 24) {
        return false;
    }
    words
        .iter()
        .all(|w| (3..=8).contains(&w.len()) && w.chars().all(|c| c.is_ascii_lowercase()))
}
