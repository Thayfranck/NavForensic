//! Identifiants Chromium (Login Data) : lecture des métadonnées.
//!
//! Le mot de passe n'est jamais déchiffré ici. Chrome et Edge récents
//! (préfixe `v20`, chiffrement « app-bound ») ne se déchiffrent pas hors ligne ;
//! les entrées `v10` / `v11` nécessiteraient la clé DPAPI de l'utilisateur,
//! qui n'est pas implémentée. Seul l'état du champ est donc signalé.

use crate::db::ReadOnlyDb;
use crate::time::from_webkit;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ChrLogin {
    pub url: String,
    pub username: String,
    pub state: String,
    pub created_ms: Option<i64>,
    pub last_used_ms: Option<i64>,
}

/// État lisible du champ mot de passe, selon son préfixe de version.
pub fn password_state(blob: &[u8]) -> &'static str {
    if blob.is_empty() {
        return "vide";
    }
    match &blob[..blob.len().min(3)] {
        b"v10" => "chiffré (DPAPI, v10)",
        b"v11" => "chiffré (v11)",
        b"v20" => "chiffré (app-bound, v20) — non déchiffrable hors ligne",
        _ => "format inconnu",
    }
}

/// Lit les identifiants d'une base Login Data (copie en lecture seule).
pub fn read_logins(path: &Path) -> Result<Vec<ChrLogin>, String> {
    let db = ReadOnlyDb::open(path)?;
    let mut stmt = db
        .conn
        .prepare(
            "SELECT origin_url, username_value, password_value, date_created, date_last_used \
             FROM logins ORDER BY date_last_used DESC",
        )
        .map_err(|e| format!("table logins illisible : {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows.flatten() {
        let (url, user, pw, created, used) = row;
        out.push(ChrLogin {
            url: url.unwrap_or_default(),
            username: user.unwrap_or_default(),
            state: password_state(pw.as_deref().unwrap_or(&[])).to_string(),
            created_ms: created.and_then(from_webkit),
            last_used_ms: used.and_then(from_webkit),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etat_selon_prefixe() {
        assert_eq!(password_state(b""), "vide");
        assert!(password_state(b"v10xxxx").starts_with("chiffré (DPAPI"));
        assert!(password_state(b"v20xxxx").contains("app-bound"));
        assert_eq!(password_state(b"zz"), "format inconnu");
    }
}
