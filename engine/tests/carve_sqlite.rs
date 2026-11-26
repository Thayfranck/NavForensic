use navforensics_engine as nf;
use rusqlite::Connection;

/// Construit une base SQLite type « historique », insère des lignes, en
/// supprime une partie (sans VACUUM), puis vérifie que le carving récupère
/// des URLs supprimées absentes des lignes vivantes.
#[test]
fn recovers_deleted_urls() {
    let dir = std::env::temp_dir().join(format!("nf_carve_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("History");

    {
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "PRAGMA page_size=4096;
             PRAGMA journal_mode=DELETE;
             PRAGMA auto_vacuum=NONE;
             CREATE TABLE urls(
                id INTEGER PRIMARY KEY,
                url TEXT,
                title TEXT,
                visit_count INTEGER,
                typed_count INTEGER,
                last_visit_time INTEGER,
                hidden INTEGER);
             CREATE TABLE visits(
                id INTEGER PRIMARY KEY,
                url INTEGER,
                visit_time INTEGER,
                visit_duration INTEGER,
                transition INTEGER,
                from_visit INTEGER);",
        )
        .unwrap();

        // Horodatage WebKit (us depuis 1601) ~ 2023. Les 220 premières lignes
        // seront effacées (plage contigue, cas d'un effacement d'historique) ;
        // les 80 suivantes restent vivantes.
        let base_webkit: i64 = 13_350_000_000_000_000;
        for i in 0..300 {
            let kind = if i < 220 { "deleted" } else { "live" };
            conn.execute(
                "INSERT INTO urls(url,title,visit_count,typed_count,last_visit_time,hidden) \
                 VALUES(?1,?2,?3,?4,?5,0)",
                rusqlite::params![
                    format!("https://{kind}-site-{i}.example/page{i}"),
                    format!("Titre {kind} {i}"),
                    i % 7,
                    i % 3,
                    base_webkit + (i as i64) * 1_000_000_000,
                ],
            )
            .unwrap();
        }
        // Efface la plage contigue (pas de VACUUM : données en place).
        conn.execute("DELETE FROM urls WHERE url LIKE 'https://deleted-%'", [])
            .unwrap();
    }

    let art = nf::load_artifact(db.to_str().unwrap(), Some("SCELLE-TEST".into())).unwrap();

    let carved = art
        .categories
        .iter()
        .find(|c| c.name.contains("supprim"))
        .expect("catégorie de récupération présente");

    // Collecte les URLs récupérées.
    let urls: Vec<String> = carved
        .rows
        .iter()
        .filter_map(|r| match r.get("URL") {
            Some(nf::model::Cell::Text(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();

    let deleted_found = urls.iter().filter(|u| u.contains("deleted-site")).count();
    assert!(
        deleted_found >= 10,
        "trop peu d'URLs supprimées récupérées : {deleted_found} (total {} lignes)",
        urls.len()
    );

    // Au moins une récupérée NE figure PAS parmi les lignes vivantes.
    assert!(
        urls.iter().any(|u| u.contains("deleted-site")),
        "aucune URL supprimée distincte récupérée"
    );

    // Horodatage reconstitué sur au moins une ligne.
    let has_ts = carved.rows.iter().any(|r| {
        matches!(r.get("Horodatage"), Some(nf::model::Cell::Time(_)))
    });
    assert!(has_ts, "aucun horodatage reconstitué");

    std::fs::remove_dir_all(&dir).ok();
}
