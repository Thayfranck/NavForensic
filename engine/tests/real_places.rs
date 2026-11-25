use navforensics_engine as nf;
use rusqlite::Connection;

#[test]
fn realistic_places_does_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("places.sqlite");
    let c = Connection::open(&p).unwrap();
    // Schema proche de Firefox reel (colonnes supplementaires, types varies)
    c.execute_batch(
        "CREATE TABLE moz_places(id INTEGER PRIMARY KEY, url TEXT, title TEXT, rev_host TEXT, visit_count INTEGER DEFAULT 0, hidden INTEGER DEFAULT 0, typed INTEGER DEFAULT 0, frecency INTEGER DEFAULT -1, last_visit_date INTEGER, guid TEXT, foreign_count INTEGER DEFAULT 0, url_hash INTEGER DEFAULT 0, description TEXT, preview_image_url TEXT, origin_id INTEGER);\
         CREATE TABLE moz_historyvisits(id INTEGER PRIMARY KEY, from_visit INTEGER, place_id INTEGER, visit_date INTEGER, visit_type INTEGER, session INTEGER);\
         CREATE TABLE moz_bookmarks(id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, parent INTEGER, position INTEGER, title TEXT, keyword_id INTEGER, folder_type TEXT, dateAdded INTEGER, lastModified INTEGER, guid TEXT);",
    ).unwrap();
    let tx = c.unchecked_transaction().unwrap();
    for i in 0..20000i64 {
        // valeurs variees : NULL titre, visit_date parfois 0 ou NULL, url longue
        let title: Option<String> = if i % 7 == 0 { None } else { Some(format!("Titre éàü {}", i)) };
        let lvd: Option<i64> = match i % 5 { 0 => None, 1 => Some(0), _ => Some(1_700_000_000_000_000 + i * 1_000_000) };
        c.execute(
            "INSERT INTO moz_places(id,url,title,visit_count,last_visit_date,frecency) VALUES(?1,?2,?3,?4,?5,?6)",
            rusqlite::params![i, format!("https://site{}.example/path/{}", i % 50, i), title, (i % 30) as i64, lvd, 100 + i],
        ).unwrap();
        let vd: Option<i64> = if i % 11 == 0 { None } else { Some(1_700_000_000_000_000 + i * 1_000_000) };
        let vt: Option<i64> = if i % 13 == 0 { None } else { Some((i % 9 + 1) as i64) };
        c.execute(
            "INSERT INTO moz_historyvisits(id,from_visit,place_id,visit_date,visit_type,session) VALUES(?1,?2,?3,?4,?5,0)",
            rusqlite::params![i, if i == 0 { 0 } else { i - 1 }, i, vd, vt],
        ).unwrap();
    }
    c.execute("INSERT INTO moz_bookmarks(id,type,fk,title,dateAdded,lastModified) VALUES(1,1,5,'BM',1700000000000000,1700000000000000)", []).unwrap();
    tx.commit().unwrap();
    drop(c);

    assert_eq!(nf::detect(&p), Some("firefox_places"));
    let art = nf::load_artifact(p.to_str().unwrap(), Some("SC / places.sqlite".into())).unwrap();
    let total: usize = art.categories.iter().map(|c| c.rows.len()).sum();
    println!("categories: {:?}, total rows: {}", art.categories.iter().map(|c| c.name.clone()).collect::<Vec<_>>(), total);
    assert!(total > 30000);

    // vues transversales
    let core: std::collections::BTreeSet<String> = nf::TIMELINE_CORE.iter().map(|s| s.to_string()).collect();
    let tl = nf::build_timeline(&[art.clone()], &core);
    let dom = nf::build_domain_summary(&[art.clone()], &core);
    println!("timeline: {}, domaines: {}", tl.len(), dom.len());
    assert!(!tl.is_empty());
}
