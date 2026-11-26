use navforensics_engine as nf;

// Test optionnel : ne s'exécute que si NF_WEBCACHE_TEST pointe un WebCacheV01.dat.
#[test]
fn webcache_parse_if_available() {
    let p = match std::env::var("NF_WEBCACHE_TEST") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skip: NF_WEBCACHE_TEST non defini");
            return;
        }
    };
    let art = nf::load_artifact(&p, Some("WebCache".into())).expect("parse webcache");
    assert_eq!(art.browser, "Internet Explorer / Edge Legacy");
    let total: usize = art.categories.iter().map(|c| c.rows.len()).sum();
    assert!(total > 0, "lignes extraites");
    for c in &art.categories {
        eprintln!("  {} : {} lignes", c.name, c.rows.len());
        // au moins une URL non vide dans l'historique
    }
    let hist = art.categories.iter().find(|c| c.name.contains("Historique"));
    if let Some(h) = hist {
        let has_url = h.rows.iter().any(|r| matches!(r.get("URL"), Some(nf::model::Cell::Text(_))));
        assert!(has_url, "URLs dans l'historique");
    }
}
