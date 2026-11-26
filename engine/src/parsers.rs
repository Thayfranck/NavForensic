//! Parseurs par navigateur. Chaque fonction renvoie une liste de categories.

use crate::db::{pick, ReadOnlyDb};
use crate::model::{Category, Cell, Row};
use crate::time::*;
use std::collections::HashSet;

type R<T> = Result<T, String>;

fn yes(v: Option<i64>) -> Cell {
    match v {
        Some(n) if n != 0 => Cell::Text("oui".into()),
        _ => Cell::Null,
    }
}

fn chrome_transition(v: i64) -> &'static str {
    match v & 0xFF {
        0 => "Lien",
        1 => "Saisie URL",
        2 => "Marque-page auto",
        3 => "Sous-cadre auto",
        4 => "Sous-cadre manuel",
        5 => "Genere",
        6 => "Page de demarrage",
        7 => "Envoi formulaire",
        8 => "Rechargement",
        9 => "Mot-cle",
        10 => "Mot-cle genere",
        _ => "?",
    }
}

fn ff_visit(v: i64) -> &'static str {
    match v {
        1 => "Lien",
        2 => "Saisie URL",
        3 => "Marque-page",
        4 => "Integre (embed)",
        5 => "Redirection permanente",
        6 => "Redirection temporaire",
        7 => "Telechargement",
        8 => "Lien dans cadre",
        9 => "Rechargement",
        _ => "?",
    }
}

// ------------------------------- Chromium ---------------------------------
pub fn chromium_history(db: &ReadOnlyDb) -> R<Vec<Category>> {
    let mut cats = Vec::new();
    let tables = db.table_names();

    let mut stmt = db
        .conn
        .prepare(
            "SELECT url,title,visit_count,typed_count,last_visit_time,hidden \
             FROM urls ORDER BY last_visit_time DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (url, title, vc, tc, lv, hid) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("URL", Cell::opt_text(url));
        rr.set("Titre", Cell::opt_text(title));
        rr.set("Visites", Cell::opt_int(vc));
        rr.set("Saisies clavier", Cell::opt_int(tc));
        rr.set("Derniere visite", Cell::opt_time(lv.and_then(from_webkit)));
        rr.set("Masque", yes(hid));
        rr.raw("last_visit_time", lv.map(|v| v.to_string()).unwrap_or_default());
        out.push(rr);
    }
    cats.push(Category::new(
        "Historique (URLs)",
        &["URL", "Titre", "Visites", "Saisies clavier", "Derniere visite", "Masque"],
        out,
    ));

    // Visites
    let vcols = db.columns("visits");
    let dur = pick(&vcols, "visit_duration");
    let sql = format!(
        "SELECT v.visit_time, u.url, u.title, v.transition, {} AS dur, v.from_visit \
         FROM visits v LEFT JOIN urls u ON u.id=v.url ORDER BY v.visit_time DESC",
        dur
    );
    let mut stmt = db.conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (vt, url, title, trans, dur, from) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("Date visite", Cell::opt_time(vt.and_then(from_webkit)));
        rr.set("URL", Cell::opt_text(url));
        rr.set("Titre", Cell::opt_text(title));
        rr.set("Transition", Cell::text(chrome_transition(trans.unwrap_or(-1))));
        rr.set(
            "Duree (s)",
            match dur {
                Some(d) if d > 0 => Cell::Int(d / 1_000_000),
                _ => Cell::Null,
            },
        );
        rr.set("Depuis visite #", Cell::opt_int(from.filter(|v| *v != 0)));
        out.push(rr);
    }
    cats.push(Category::new(
        "Visites",
        &["Date visite", "URL", "Titre", "Transition", "Duree (s)", "Depuis visite #"],
        out,
    ));

    // Telechargements
    if tables.contains("downloads") {
        let dc = db.columns("downloads");
        let sql = format!(
            "SELECT start_time, {} AS end_time, target_path, {} AS tab_url, \
             received_bytes, total_bytes, {} AS mime_type, {} AS state \
             FROM downloads ORDER BY start_time DESC",
            pick(&dc, "end_time"),
            pick(&dc, "tab_url"),
            pick(&dc, "mime_type"),
            pick(&dc, "state"),
        );
        let mut stmt = db.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, Option<i64>>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let (st, en, path, tab, rb, tb, mime, state) = row.map_err(|e| e.to_string())?;
            let mut rr = Row::new();
            rr.set("Debut", Cell::opt_time(st.and_then(from_webkit)));
            rr.set("Fin", Cell::opt_time(en.and_then(from_webkit)));
            rr.set("Fichier cible", Cell::opt_text(path));
            rr.set("URL onglet", Cell::opt_text(tab));
            rr.set("Recu (o)", Cell::opt_int(rb));
            rr.set("Total (o)", Cell::opt_int(tb));
            rr.set("Type MIME", Cell::opt_text(mime));
            rr.set("Etat", Cell::opt_int(state));
            out.push(rr);
        }
        cats.push(Category::new(
            "Telechargements",
            &["Debut", "Fin", "Fichier cible", "URL onglet", "Recu (o)", "Total (o)", "Type MIME", "Etat"],
            out,
        ));
    }

    // Termes de recherche
    if tables.contains("keyword_search_terms") {
        let mut stmt = db
            .conn
            .prepare(
                "SELECT k.term, u.url, u.last_visit_time FROM keyword_search_terms k \
                 JOIN urls u ON u.id=k.url_id ORDER BY u.last_visit_time DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let (term, url, lv) = row.map_err(|e| e.to_string())?;
            let mut rr = Row::new();
            rr.set("Terme recherche", Cell::opt_text(term));
            rr.set("URL", Cell::opt_text(url));
            rr.set("Derniere visite", Cell::opt_time(lv.and_then(from_webkit)));
            out.push(rr);
        }
        cats.push(Category::new(
            "Termes de recherche",
            &["Terme recherche", "URL", "Derniere visite"],
            out,
        ));
    }
    Ok(cats)
}

pub fn chromium_cookies(db: &ReadOnlyDb) -> R<Vec<Category>> {
    let cc = db.columns("cookies");
    let enc = if cc.contains("encrypted_value") {
        "length(encrypted_value)"
    } else {
        "0"
    };
    let sql = format!(
        "SELECT host_key,name,path,creation_utc,expires_utc,last_access_utc,\
         is_secure,is_httponly,value,{} AS enclen FROM cookies ORDER BY host_key,name",
        enc
    );
    let mut stmt = db.conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<i64>>(9)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (host, name, path, cr, ex, la, sec, http, val, enclen) =
            row.map_err(|e| e.to_string())?;
        let value = match (val, enclen) {
            (Some(v), _) if !v.is_empty() => Cell::Text(v),
            (_, Some(n)) if n > 0 => Cell::Text(format!("[chiffre : {} octets]", n)),
            _ => Cell::Null,
        };
        let mut rr = Row::new();
        rr.set("Hote", Cell::opt_text(host));
        rr.set("Nom", Cell::opt_text(name));
        rr.set("Chemin", Cell::opt_text(path));
        rr.set("Creation", Cell::opt_time(cr.and_then(from_webkit)));
        rr.set("Expiration", Cell::opt_time(ex.and_then(from_webkit)));
        rr.set("Dernier acces", Cell::opt_time(la.and_then(from_webkit)));
        rr.set("Securise", yes(sec));
        rr.set("HttpOnly", yes(http));
        rr.set("Valeur / chiffrement", value);
        out.push(rr);
    }
    Ok(vec![Category::new(
        "Cookies",
        &["Hote", "Nom", "Chemin", "Creation", "Expiration", "Dernier acces", "Securise", "HttpOnly", "Valeur / chiffrement"],
        out,
    )])
}

pub fn chromium_webdata(db: &ReadOnlyDb) -> R<Vec<Category>> {
    if !db.table_names().contains("autofill") {
        return Ok(vec![]);
    }
    let mut stmt = db
        .conn
        .prepare("SELECT name,value,count,date_created,date_last_used FROM autofill ORDER BY date_last_used DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (name, val, count, dc, dl) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("Champ", Cell::opt_text(name));
        rr.set("Valeur", Cell::opt_text(val));
        rr.set("Occurrences", Cell::opt_int(count));
        rr.set("Premiere utilisation", Cell::opt_time(dc.and_then(from_unix_s)));
        rr.set("Derniere utilisation", Cell::opt_time(dl.and_then(from_unix_s)));
        out.push(rr);
    }
    Ok(vec![Category::new(
        "Saisie auto (Autofill)",
        &["Champ", "Valeur", "Occurrences", "Premiere utilisation", "Derniere utilisation"],
        out,
    )])
}

pub fn chromium_logins(db: &ReadOnlyDb) -> R<Vec<Category>> {
    let lc = db.columns("logins");
    let sql = format!(
        "SELECT origin_url, signon_realm, username_value, {} AS tu, {} AS dc, {} AS dl, {} AS blk \
         FROM logins ORDER BY date_last_used DESC",
        pick(&lc, "times_used"),
        pick(&lc, "date_created"),
        pick(&lc, "date_last_used"),
        pick(&lc, "blacklisted_by_user"),
    );
    let mut stmt = db.conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<i64>>(6)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (url, realm, user, tu, dc, dl, blk) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("URL origine", Cell::opt_text(url));
        rr.set("Realm", Cell::opt_text(realm));
        rr.set("Nom utilisateur", Cell::opt_text(user));
        rr.set("Utilisations", Cell::opt_int(tu));
        rr.set("Creation", Cell::opt_time(dc.and_then(from_webkit)));
        rr.set("Derniere utilisation", Cell::opt_time(dl.and_then(from_webkit)));
        rr.set("Sur liste noire", yes(blk));
        out.push(rr);
    }
    Ok(vec![Category::new(
        "Identifiants (metadonnees, sans mot de passe)",
        &["URL origine", "Realm", "Nom utilisateur", "Utilisations", "Creation", "Derniere utilisation", "Sur liste noire"],
        out,
    )])
}

pub fn chromium_shortcuts(db: &ReadOnlyDb) -> R<Vec<Category>> {
    let sc = db.columns("omni_box_shortcuts");
    let sql = format!(
        "SELECT text, url, {} AS contents, {} AS lat, {} AS hits FROM omni_box_shortcuts \
         ORDER BY {} DESC",
        pick(&sc, "contents"),
        pick(&sc, "last_access_time"),
        pick(&sc, "number_of_hits"),
        pick(&sc, "last_access_time"),
    );
    let mut stmt = db.conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (text, url, cont, lat, hits) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("Texte saisi", Cell::opt_text(text));
        rr.set("URL", Cell::opt_text(url));
        rr.set("Contenu", Cell::opt_text(cont));
        rr.set("Dernier acces", Cell::opt_time(lat.and_then(from_webkit)));
        rr.set("Occurrences", Cell::opt_int(hits));
        out.push(rr);
    }
    Ok(vec![Category::new(
        "Saisies omnibox (raccourcis)",
        &["Texte saisi", "URL", "Contenu", "Dernier acces", "Occurrences"],
        out,
    )])
}

pub fn chromium_topsites(db: &ReadOnlyDb) -> R<Vec<Category>> {
    let tc = db.columns("top_sites");
    let sql = format!(
        "SELECT {} AS rk, url, {} AS title FROM top_sites ORDER BY {}",
        pick(&tc, "url_rank"),
        pick(&tc, "title"),
        pick(&tc, "url_rank"),
    );
    let mut stmt = db.conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (rk, url, title) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("Rang", Cell::opt_int(rk));
        rr.set("URL", Cell::opt_text(url));
        rr.set("Titre", Cell::opt_text(title));
        out.push(rr);
    }
    Ok(vec![Category::new("Sites les plus visites", &["Rang", "URL", "Titre"], out)])
}

pub fn chromium_favicons(db: &ReadOnlyDb) -> R<Vec<Category>> {
    let mut stmt = db
        .conn
        .prepare(
            "SELECT DISTINCT im.page_url, f.url FROM icon_mapping im \
             LEFT JOIN favicons f ON f.id=im.icon_id ORDER BY im.page_url",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (page, icon) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("Page visitee", Cell::opt_text(page));
        rr.set("URL icone", Cell::opt_text(icon));
        out.push(rr);
    }
    Ok(vec![Category::new("Favicons (pages visitees)", &["Page visitee", "URL icone"], out)])
}

// ------------------------------- Firefox ----------------------------------
pub fn firefox_places(db: &ReadOnlyDb) -> R<Vec<Category>> {
    let mut cats = Vec::new();
    let mut stmt = db
        .conn
        .prepare(
            "SELECT url,title,visit_count,last_visit_date FROM moz_places \
             WHERE url IS NOT NULL ORDER BY last_visit_date DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (url, title, vc, lv) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("URL", Cell::opt_text(url));
        rr.set("Titre", Cell::opt_text(title));
        rr.set("Visites", Cell::opt_int(vc));
        rr.set("Derniere visite", Cell::opt_time(lv.and_then(from_prtime)));
        out.push(rr);
    }
    cats.push(Category::new("Historique (URLs)", &["URL", "Titre", "Visites", "Derniere visite"], out));

    let mut stmt = db
        .conn
        .prepare(
            "SELECT hv.visit_date, p.url, p.title, hv.visit_type, hv.from_visit \
             FROM moz_historyvisits hv LEFT JOIN moz_places p ON p.id=hv.place_id \
             ORDER BY hv.visit_date DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<i64>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (vd, url, title, vt, from) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("Date visite", Cell::opt_time(vd.and_then(from_prtime)));
        rr.set("URL", Cell::opt_text(url));
        rr.set("Titre", Cell::opt_text(title));
        rr.set("Type", Cell::text(ff_visit(vt.unwrap_or(-1))));
        rr.set("Depuis visite #", Cell::opt_int(from.filter(|v| *v != 0)));
        out.push(rr);
    }
    cats.push(Category::new("Visites", &["Date visite", "URL", "Titre", "Type", "Depuis visite #"], out));

    if db.table_names().contains("moz_bookmarks") {
        let mut stmt = db
            .conn
            .prepare(
                "SELECT b.title, p.url, b.dateAdded, b.lastModified FROM moz_bookmarks b \
                 LEFT JOIN moz_places p ON p.id=b.fk WHERE b.type=1 ORDER BY b.dateAdded DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let (title, url, added, modif) = row.map_err(|e| e.to_string())?;
            let mut rr = Row::new();
            rr.set("Titre", Cell::opt_text(title));
            rr.set("URL", Cell::opt_text(url));
            rr.set("Ajoute", Cell::opt_time(added.and_then(from_prtime)));
            rr.set("Modifie", Cell::opt_time(modif.and_then(from_prtime)));
            out.push(rr);
        }
        cats.push(Category::new("Marque-pages", &["Titre", "URL", "Ajoute", "Modifie"], out));
    }
    Ok(cats)
}

pub fn firefox_cookies(db: &ReadOnlyDb) -> R<Vec<Category>> {
    let cc = db.columns("moz_cookies");
    let sql = format!(
        "SELECT host,name,path,value,{} AS ct,{} AS la,expiry,{} AS sec,{} AS http \
         FROM moz_cookies ORDER BY host,name",
        pick(&cc, "creationTime"),
        pick(&cc, "lastAccessed"),
        pick(&cc, "isSecure"),
        pick(&cc, "isHttpOnly"),
    );
    let mut stmt = db.conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<i64>>(8)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (host, name, path, val, ct, la, exp, sec, http) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("Hote", Cell::opt_text(host));
        rr.set("Nom", Cell::opt_text(name));
        rr.set("Chemin", Cell::opt_text(path));
        rr.set("Valeur", Cell::opt_text(val));
        rr.set("Creation", Cell::opt_time(ct.and_then(from_prtime)));
        rr.set("Dernier acces", Cell::opt_time(la.and_then(from_prtime)));
        rr.set("Expiration", Cell::opt_time(exp.and_then(from_unix_s)));
        rr.set("Securise", yes(sec));
        rr.set("HttpOnly", yes(http));
        out.push(rr);
    }
    Ok(vec![Category::new(
        "Cookies",
        &["Hote", "Nom", "Chemin", "Valeur", "Creation", "Dernier acces", "Expiration", "Securise", "HttpOnly"],
        out,
    )])
}

pub fn firefox_formhistory(db: &ReadOnlyDb) -> R<Vec<Category>> {
    let mut stmt = db
        .conn
        .prepare("SELECT fieldname,value,timesUsed,firstUsed,lastUsed FROM moz_formhistory ORDER BY lastUsed DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (field, val, times, first, last) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("Champ", Cell::opt_text(field));
        rr.set("Valeur", Cell::opt_text(val));
        rr.set("Occurrences", Cell::opt_int(times));
        rr.set("Premiere utilisation", Cell::opt_time(first.and_then(from_prtime)));
        rr.set("Derniere utilisation", Cell::opt_time(last.and_then(from_prtime)));
        out.push(rr);
    }
    Ok(vec![Category::new(
        "Historique de formulaires",
        &["Champ", "Valeur", "Occurrences", "Premiere utilisation", "Derniere utilisation"],
        out,
    )])
}

pub fn firefox_favicons(db: &ReadOnlyDb) -> R<Vec<Category>> {
    let mut stmt = db
        .conn
        .prepare(
            "SELECT DISTINCT p.page_url, i.icon_url FROM moz_icons_to_pages m \
             JOIN moz_pages_w_icons p ON p.id=m.page_id JOIN moz_icons i ON i.id=m.icon_id \
             ORDER BY p.page_url",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (page, icon) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("Page visitee", Cell::opt_text(page));
        rr.set("URL icone", Cell::opt_text(icon));
        out.push(rr);
    }
    Ok(vec![Category::new("Favicons (pages visitees)", &["Page visitee", "URL icone"], out)])
}

// ------------------------------- Safari -----------------------------------
pub fn safari_history(db: &ReadOnlyDb) -> R<Vec<Category>> {
    let mut cats = Vec::new();
    let mut stmt = db
        .conn
        .prepare(
            "SELECT hi.url, hi.visit_count, MAX(hv.visit_time) AS lv FROM history_items hi \
             LEFT JOIN history_visits hv ON hv.history_item=hi.id GROUP BY hi.id ORDER BY lv DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, Option<f64>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (url, vc, lv) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("URL", Cell::opt_text(url));
        rr.set("Visites", Cell::opt_int(vc));
        rr.set("Derniere visite", Cell::opt_time(lv.and_then(from_safari)));
        out.push(rr);
    }
    cats.push(Category::new("Historique (URLs)", &["URL", "Visites", "Derniere visite"], out));

    let vc = db.columns("history_visits");
    let load = pick(&vc, "load_successful");
    let sql = format!(
        "SELECT hv.visit_time, hi.url, hv.title, {} AS ok FROM history_visits hv \
         LEFT JOIN history_items hi ON hi.id=hv.history_item ORDER BY hv.visit_time DESC",
        load
    );
    let mut stmt = db.conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<f64>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (vt, url, title, ok) = row.map_err(|e| e.to_string())?;
        let mut rr = Row::new();
        rr.set("Date visite", Cell::opt_time(vt.and_then(from_safari)));
        rr.set("URL", Cell::opt_text(url));
        rr.set("Titre", Cell::opt_text(title));
        rr.set(
            "Charge OK",
            match ok {
                Some(n) if n != 0 => Cell::Text("oui".into()),
                Some(0) => Cell::Text("non".into()),
                _ => Cell::Null,
            },
        );
        out.push(rr);
    }
    cats.push(Category::new("Visites", &["Date visite", "URL", "Titre", "Charge OK"], out));
    Ok(cats)
}

// --------------------------- Fichiers JSON --------------------------------
fn walk_chromium_bm(node: &serde_json::Value, folder: &str, out: &mut Vec<Row>) {
    let obj = match node.as_object() {
        Some(o) => o,
        None => return,
    };
    match obj.get("type").and_then(|v| v.as_str()) {
        Some("url") => {
            let mut rr = Row::new();
            rr.set("Nom", Cell::opt_text(obj.get("name").and_then(|v| v.as_str()).map(String::from)));
            rr.set("URL", Cell::opt_text(obj.get("url").and_then(|v| v.as_str()).map(String::from)));
            rr.set("Dossier", Cell::text(folder));
            let added = obj
                .get("date_added")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok());
            rr.set("Ajoute", Cell::opt_time(added.and_then(from_webkit)));
            out.push(rr);
        }
        Some("folder") => {
            let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let sub = if folder.is_empty() {
                name.to_string()
            } else {
                format!("{} / {}", folder, name)
            };
            if let Some(children) = obj.get("children").and_then(|v| v.as_array()) {
                for ch in children {
                    walk_chromium_bm(ch, &sub, out);
                }
            }
        }
        _ => {}
    }
}

pub fn chromium_bookmarks(path: &std::path::Path) -> R<Vec<Category>> {
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(roots) = data.get("roots").and_then(|v| v.as_object()) {
        for (_k, root) in roots {
            walk_chromium_bm(root, "", &mut out);
        }
    }
    Ok(vec![Category::new("Marque-pages", &["Nom", "URL", "Dossier", "Ajoute"], out)])
}

pub fn chromium_preferences(path: &std::path::Path) -> R<Vec<Category>> {
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut cats = Vec::new();

    let mut acc = Vec::new();
    if let Some(list) = data.get("account_info").and_then(|v| v.as_array()) {
        for a in list {
            let mut rr = Row::new();
            rr.set("Email", Cell::opt_text(a.get("email").and_then(|v| v.as_str()).map(String::from)));
            rr.set("Nom", Cell::opt_text(a.get("full_name").and_then(|v| v.as_str()).map(String::from)));
            rr.set("Gaia ID", Cell::opt_text(a.get("gaia").and_then(|v| v.as_str()).map(String::from)));
            acc.push(rr);
        }
    }
    if !acc.is_empty() {
        cats.push(Category::new("Compte(s) connecte(s)", &["Email", "Nom", "Gaia ID"], acc));
    }

    let mut ext = Vec::new();
    if let Some(settings) = data
        .get("extensions")
        .and_then(|v| v.get("settings"))
        .and_then(|v| v.as_object())
    {
        for (id, info) in settings {
            let man = info.get("manifest");
            let mut rr = Row::new();
            rr.set("Nom", Cell::opt_text(man.and_then(|m| m.get("name")).and_then(|v| v.as_str()).map(String::from)));
            rr.set("Version", Cell::opt_text(man.and_then(|m| m.get("version")).and_then(|v| v.as_str()).map(String::from)));
            rr.set("ID", Cell::text(id.clone()));
            rr.set("Etat", Cell::opt_int(info.get("state").and_then(|v| v.as_i64())));
            rr.set("Chemin", Cell::opt_text(info.get("path").and_then(|v| v.as_str()).map(String::from)));
            let it = info.get("install_time").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok());
            rr.set("Installe", Cell::opt_time(it.and_then(from_webkit)));
            ext.push(rr);
        }
    }
    if !ext.is_empty() {
        cats.push(Category::new("Extensions", &["Nom", "Version", "ID", "Etat", "Chemin", "Installe"], ext));
    }

    let mut conf = Vec::new();
    if let Some(hp) = data.get("homepage").and_then(|v| v.as_str()) {
        let mut rr = Row::new();
        rr.set("Parametre", Cell::text("Page d'accueil"));
        rr.set("Valeur", Cell::text(hp));
        conf.push(rr);
    }
    let startup = data
        .get("session")
        .and_then(|v| v.get("startup_urls"))
        .and_then(|v| v.as_array());
    if let Some(urls) = startup {
        let joined: Vec<String> = urls.iter().filter_map(|u| u.as_str().map(String::from)).collect();
        if !joined.is_empty() {
            let mut rr = Row::new();
            rr.set("Parametre", Cell::text("URLs de demarrage"));
            rr.set("Valeur", Cell::text(joined.join(", ")));
            conf.push(rr);
        }
    }
    if let Some(name) = data.get("profile").and_then(|v| v.get("name")).and_then(|v| v.as_str()) {
        let mut rr = Row::new();
        rr.set("Parametre", Cell::text("Nom du profil"));
        rr.set("Valeur", Cell::text(name));
        conf.push(rr);
    }
    if !conf.is_empty() {
        cats.push(Category::new("Configuration", &["Parametre", "Valeur"], conf));
    }
    Ok(cats)
}

pub fn firefox_logins(path: &std::path::Path) -> R<Vec<Category>> {
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(list) = data.get("logins").and_then(|v| v.as_array()) {
        for l in list {
            let mut rr = Row::new();
            rr.set(
                "Hote",
                Cell::opt_text(
                    l.get("hostname")
                        .or_else(|| l.get("origin"))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                ),
            );
            rr.set("Utilisations", Cell::opt_int(l.get("timesUsed").and_then(|v| v.as_i64())));
            rr.set("Creation", Cell::opt_time(l.get("timeCreated").and_then(|v| v.as_i64()).and_then(from_unix_ms)));
            rr.set("Derniere utilisation", Cell::opt_time(l.get("timeLastUsed").and_then(|v| v.as_i64()).and_then(from_unix_ms)));
            rr.set("Mot de passe modifie", Cell::opt_time(l.get("timePasswordChanged").and_then(|v| v.as_i64()).and_then(from_unix_ms)));
            out.push(rr);
        }
    }
    Ok(vec![Category::new(
        "Identifiants (metadonnees, sans mot de passe)",
        &["Hote", "Utilisations", "Creation", "Derniere utilisation", "Mot de passe modifie"],
        out,
    )])
}

// ----------------------------- Safari plist -------------------------------
fn plist_date_ms(v: &plist::Value) -> Option<i64> {
    if let plist::Value::Date(d) = v {
        let st: std::time::SystemTime = (*d).into();
        let dur = st.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(dur.as_millis() as i64)
    } else {
        None
    }
}

pub fn safari_downloads(path: &std::path::Path) -> R<Vec<Category>> {
    let data = plist::Value::from_file(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(list) = data
        .as_dictionary()
        .and_then(|d| d.get("DownloadHistory"))
        .and_then(|v| v.as_array())
    {
        for e in list {
            let d = match e.as_dictionary() {
                Some(d) => d,
                None => continue,
            };
            let mut rr = Row::new();
            rr.set("Date", Cell::opt_time(d.get("DownloadEntryDateAddedKey").and_then(plist_date_ms)));
            rr.set("URL", Cell::opt_text(d.get("DownloadEntryURL").and_then(|v| v.as_string()).map(String::from)));
            let file = d.get("DownloadEntryPath").and_then(|v| {
                v.as_string().map(String::from).or_else(|| {
                    v.as_array().and_then(|a| a.first()).and_then(|x| x.as_string()).map(String::from)
                })
            });
            rr.set("Fichier", Cell::opt_text(file));
            rr.set("Recu (o)", Cell::opt_int(d.get("DownloadEntryProgressBytesSoFar").and_then(|v| v.as_signed_integer())));
            rr.set("Total (o)", Cell::opt_int(d.get("DownloadEntryProgressTotalToLoad").and_then(|v| v.as_signed_integer())));
            out.push(rr);
        }
    }
    Ok(vec![Category::new("Telechargements", &["Date", "URL", "Fichier", "Recu (o)", "Total (o)"], out)])
}

fn walk_safari_bm(node: &plist::Value, folder: &str, out: &mut Vec<Row>) {
    let d = match node.as_dictionary() {
        Some(d) => d,
        None => return,
    };
    match d.get("WebBookmarkType").and_then(|v| v.as_string()) {
        Some("WebBookmarkTypeLeaf") => {
            let title = d
                .get("URIDictionary")
                .and_then(|v| v.as_dictionary())
                .and_then(|u| u.get("title"))
                .and_then(|v| v.as_string())
                .map(String::from);
            let mut rr = Row::new();
            rr.set("Nom", Cell::opt_text(title));
            rr.set("URL", Cell::opt_text(d.get("URLString").and_then(|v| v.as_string()).map(String::from)));
            rr.set("Dossier", Cell::text(folder));
            out.push(rr);
        }
        _ => {
            let title = d.get("Title").and_then(|v| v.as_string()).unwrap_or("");
            let sub = if !folder.is_empty() && !title.is_empty() {
                format!("{} / {}", folder, title)
            } else if !title.is_empty() {
                title.to_string()
            } else {
                folder.to_string()
            };
            if let Some(children) = d.get("Children").and_then(|v| v.as_array()) {
                for ch in children {
                    walk_safari_bm(ch, &sub, out);
                }
            }
        }
    }
}

pub fn safari_bookmarks(path: &std::path::Path) -> R<Vec<Category>> {
    let data = plist::Value::from_file(path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    walk_safari_bm(&data, "", &mut out);
    Ok(vec![Category::new("Marque-pages", &["Nom", "URL", "Dossier"], out)])
}

/// utilitaire partage : colonnes presentes
pub fn has(cols: &HashSet<String>, name: &str) -> bool {
    cols.contains(name)
}
