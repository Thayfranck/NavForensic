use chrono::{TimeZone, Utc};
use navforensics_engine as nf;
use nf::model::Cell;
use rusqlite::Connection;
use std::collections::BTreeSet;

fn ms(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap().timestamp_millis()
}
fn webkit(target_ms: i64) -> i64 {
    (target_ms + 11_644_473_600_000) * 1000
}
fn prtime(target_ms: i64) -> i64 {
    target_ms * 1000
}
fn cf(target_ms: i64) -> f64 {
    (target_ms / 1000 - 978_307_200) as f64
}

fn cell_text(c: Option<&Cell>) -> String {
    match c {
        Some(Cell::Text(s)) => s.clone(),
        _ => String::new(),
    }
}
fn cell_ms(c: Option<&Cell>) -> Option<i64> {
    match c {
        Some(Cell::Time(m)) => Some(*m),
        _ => None,
    }
}

#[test]
fn chromium_history_and_conversions() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("History");
    let c = Connection::open(&p).unwrap();
    c.execute_batch(
        "CREATE TABLE urls(id INTEGER PRIMARY KEY,url TEXT,title TEXT,visit_count INT,typed_count INT,last_visit_time INT,hidden INT);\
         CREATE TABLE visits(id INTEGER PRIMARY KEY,url INT,visit_time INT,from_visit INT,transition INT,visit_duration INT);\
         CREATE TABLE downloads(id INTEGER PRIMARY KEY,target_path TEXT,tab_url TEXT,start_time INT,end_time INT,received_bytes INT,total_bytes INT,state INT,danger_type INT,opened INT,mime_type TEXT);\
         CREATE TABLE keyword_search_terms(keyword_id INT,url_id INT,term TEXT);",
    ).unwrap();
    let lv = ms(2026, 3, 15, 16, 0, 0);
    c.execute("INSERT INTO urls VALUES(1,'https://example.com/','Exemple',5,2,?1,0)", [webkit(lv)]).unwrap();
    c.execute("INSERT INTO visits VALUES(1,1,?1,0,1,2500000)", [webkit(lv)]).unwrap();
    c.execute("INSERT INTO downloads VALUES(1,'C:/x/secret.zip','https://secret.local/leak',?1,?2,1,1,1,0,0,'application/zip')",
        rusqlite::params![webkit(ms(2026,3,15,15,1,0)), webkit(ms(2026,3,15,15,1,30))]).unwrap();
    c.execute("INSERT INTO keyword_search_terms VALUES(1,1,'mot de passe admin')", []).unwrap();
    drop(c);

    assert_eq!(nf::detect(&p), Some("chromium_history"));
    let art = nf::load_artifact(p.to_str().unwrap(), Some("SC-001".into())).unwrap();
    assert!(art.browser.starts_with("Chromium"));
    let names: Vec<&str> = art.categories.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Historique (URLs)"));
    assert!(names.contains(&"Telechargements"));
    assert!(names.contains(&"Termes de recherche"));

    let hist = art.categories.iter().find(|c| c.name == "Historique (URLs)").unwrap();
    assert_eq!(cell_ms(hist.rows[0].get("Derniere visite")), Some(lv));
    let dl = art.categories.iter().find(|c| c.name == "Telechargements").unwrap();
    assert_eq!(cell_ms(dl.rows[0].get("Debut")), Some(ms(2026, 3, 15, 15, 1, 0)));
    let vis = art.categories.iter().find(|c| c.name == "Visites").unwrap();
    assert_eq!(cell_text(vis.rows[0].get("Transition")), "Saisie URL");
}

#[test]
fn firefox_and_safari_timestamps() {
    let dir = tempfile::tempdir().unwrap();
    // Firefox
    let fp = dir.path().join("places.sqlite");
    let c = Connection::open(&fp).unwrap();
    c.execute_batch(
        "CREATE TABLE moz_places(id INTEGER PRIMARY KEY,url TEXT,title TEXT,visit_count INT,last_visit_date INT);\
         CREATE TABLE moz_historyvisits(id INTEGER PRIMARY KEY,place_id INT,visit_date INT,visit_type INT,from_visit INT);",
    ).unwrap();
    let t = ms(2026, 4, 1, 9, 0, 0);
    c.execute("INSERT INTO moz_places VALUES(1,'https://mozilla.org/','Moz',3,?1)", [prtime(t)]).unwrap();
    c.execute("INSERT INTO moz_historyvisits VALUES(1,1,?1,2,0)", [prtime(t)]).unwrap();
    drop(c);
    let art = nf::load_artifact(fp.to_str().unwrap(), None).unwrap();
    let h = art.categories.iter().find(|c| c.name == "Historique (URLs)").unwrap();
    assert_eq!(cell_ms(h.rows[0].get("Derniere visite")), Some(t));

    // Safari
    let sp = dir.path().join("History.db");
    let c = Connection::open(&sp).unwrap();
    c.execute_batch(
        "CREATE TABLE history_items(id INTEGER PRIMARY KEY,url TEXT,visit_count INT);\
         CREATE TABLE history_visits(id INTEGER PRIMARY KEY,history_item INT,visit_time REAL,title TEXT,load_successful INT);",
    ).unwrap();
    let t2 = ms(2026, 6, 1, 10, 0, 0);
    c.execute("INSERT INTO history_items VALUES(1,'https://apple.com/',3)", []).unwrap();
    c.execute("INSERT INTO history_visits VALUES(1,1,?1,'Apple',1)", [cf(t2)]).unwrap();
    drop(c);
    assert_eq!(nf::detect(&sp), Some("safari_history"));
    let art = nf::load_artifact(sp.to_str().unwrap(), None).unwrap();
    assert_eq!(art.browser, "Safari");
    let h = art.categories.iter().find(|c| c.name == "Historique (URLs)").unwrap();
    assert_eq!(cell_ms(h.rows[0].get("Derniere visite")), Some(t2));
}

#[test]
fn cookies_encrypted_and_logins_no_password() {
    let dir = tempfile::tempdir().unwrap();
    let cp = dir.path().join("Cookies");
    let c = Connection::open(&cp).unwrap();
    c.execute_batch("CREATE TABLE cookies(host_key TEXT,name TEXT,path TEXT,creation_utc INT,expires_utc INT,last_access_utc INT,is_secure INT,is_httponly INT,value TEXT,encrypted_value BLOB);").unwrap();
    c.execute("INSERT INTO cookies VALUES('.example.com','sid','/',?1,?2,?3,1,1,'',?4)",
        rusqlite::params![webkit(ms(2026,3,1,0,0,0)), webkit(ms(2027,3,1,0,0,0)), webkit(ms(2026,3,2,0,0,0)), vec![1u8,2,3,4,5]]).unwrap();
    drop(c);
    let art = nf::load_artifact(cp.to_str().unwrap(), None).unwrap();
    let ck = &art.categories[0].rows[0];
    assert!(cell_text(ck.get("Valeur / chiffrement")).contains("chiffre"));

    let lp = dir.path().join("Login Data");
    let c = Connection::open(&lp).unwrap();
    c.execute_batch("CREATE TABLE logins(origin_url TEXT,signon_realm TEXT,username_value TEXT,password_value BLOB,times_used INT,date_created INT,date_last_used INT,blacklisted_by_user INT);").unwrap();
    c.execute("INSERT INTO logins VALUES('https://admin.local/','https://admin.local/','administrateur',?1,9,?2,?3,0)",
        rusqlite::params![vec![0u8,1,2], webkit(ms(2026,1,5,0,0,0)), webkit(ms(2026,3,10,0,0,0))]).unwrap();
    drop(c);
    let art = nf::load_artifact(lp.to_str().unwrap(), None).unwrap();
    let cols = art.categories[0].columns.join(",").to_lowercase();
    assert!(!cols.contains("password") && !cols.contains("mot de passe"));
    assert_eq!(cell_text(art.categories[0].rows[0].get("Nom utilisateur")), "administrateur");
}

#[test]
fn sessions_and_cache_carving() {
    let dir = tempfile::tempdir().unwrap();
    // SNSS
    let sp = dir.path().join("Current Session");
    let mut blob: Vec<u8> = b"SNSS".to_vec();
    blob.extend_from_slice(&1i32.to_le_bytes());
    blob.extend_from_slice(b"\x00 https://ascii-tab.local/path?x=1 \x00");
    for ch in "https://utf16-tab.local/open".encode_utf16() {
        blob.extend_from_slice(&ch.to_le_bytes());
    }
    std::fs::write(&sp, &blob).unwrap();
    assert_eq!(nf::detect(&sp), Some("chromium_session"));
    let art = nf::load_artifact(sp.to_str().unwrap(), None).unwrap();
    let urls: Vec<String> = art.categories[0].rows.iter().map(|r| cell_text(r.get("URL"))).collect();
    assert!(urls.iter().any(|u| u.contains("ascii-tab.local")));
    assert!(urls.iter().any(|u| u.contains("utf16-tab.local")));

    // cache folder
    let cdir = dir.path().join("Cache_Data");
    std::fs::create_dir(&cdir).unwrap();
    std::fs::write(cdir.join("data_1"), b"\x00GET https://cache-hit.local/img.png HTTP\x00").unwrap();
    let art = nf::carve_cache_urls(&cdir, Some("SC-cache".into()));
    assert!(art.categories[0].rows.iter().any(|r| cell_text(r.get("URL")).contains("cache-hit.local")));
}

#[test]
fn timeline_domains_and_search() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("History");
    let c = Connection::open(&p).unwrap();
    c.execute_batch(
        "CREATE TABLE urls(id INTEGER PRIMARY KEY,url TEXT,title TEXT,visit_count INT,typed_count INT,last_visit_time INT,hidden INT);\
         CREATE TABLE visits(id INTEGER PRIMARY KEY,url INT,visit_time INT,from_visit INT,transition INT,visit_duration INT);",
    ).unwrap();
    c.execute("INSERT INTO urls VALUES(1,'https://secret.local/a','Fuite admin',1,0,?1,0)", [webkit(ms(2026,3,1,8,0,0))]).unwrap();
    c.execute("INSERT INTO urls VALUES(2,'https://secret.local/b','B',1,0,?1,0)", [webkit(ms(2026,3,2,9,0,0))]).unwrap();
    c.execute("INSERT INTO visits VALUES(1,1,?1,0,1,0)", [webkit(ms(2026,3,1,8,0,0))]).unwrap();
    c.execute("INSERT INTO visits VALUES(2,2,?1,0,1,0)", [webkit(ms(2026,3,2,9,0,0))]).unwrap();
    drop(c);
    let art = nf::load_artifact(p.to_str().unwrap(), Some("SC-001".into())).unwrap();
    let arts = vec![art];

    let core: BTreeSet<String> = nf::TIMELINE_CORE.iter().map(|s| s.to_string()).collect();
    let tl = nf::build_timeline(&arts, &core);
    assert!(!tl.is_empty());
    let dates: Vec<i64> = tl.iter().filter_map(|r| cell_ms(r.get("Date"))).collect();
    let mut sorted = dates.clone();
    sorted.sort();
    assert_eq!(dates, sorted);

    let dom = nf::build_domain_summary(&arts, &core);
    assert_eq!(cell_text(dom[0].get("Domaine")), "secret.local");

    let res = nf::search_all(&arts, "admin");
    assert!(res.iter().any(|r| cell_text(r.get("Valeur")) == "Fuite admin"));
}
