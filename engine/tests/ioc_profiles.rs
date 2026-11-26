use navforensics_engine as nf;
use nf::model::{Artifact, Category, Cell, Row};

fn hrow(url: &str, title: &str) -> Row {
    let mut r = Row::new();
    r.set("URL", Cell::text(url));
    r.set("Titre", Cell::text(title));
    r
}

fn art_with(path: &str, browser: &str, cats: Vec<Category>) -> Artifact {
    Artifact {
        evidence: "S".into(),
        path: path.into(),
        sha256: "x".into(),
        browser: browser.into(),
        kind: "History".into(),
        categories: cats,
        notes: vec![],
    }
}

#[test]
fn ioc_parse_and_match() {
    let list = "# liste de surveillance\nevil-domain.ru\nsuspect@mail.com\n0x52908400098527886E0F7030069857D2E4169EE7\nrendez-vous secret | Mot-clé\n";
    let iocs = nf::parse_iocs(list);
    assert_eq!(iocs.len(), 4);
    assert_eq!(iocs[0].kind, "Domaine");
    assert_eq!(iocs[1].kind, "Email");
    assert_eq!(iocs[2].kind, "Adresse crypto");
    assert_eq!(iocs[3].kind, "Mot-clé");

    let hist = Category::new(
        "Historique (URLs)",
        &["URL", "Titre"],
        vec![
            hrow("https://evil-domain.ru/panel", "Panel"),
            hrow("https://news.example/politique", "Actu"),
            hrow("https://mail.google.com/", "contact suspect@mail.com"),
        ],
    );
    let m = nf::build_ioc_matches(&[art_with("x", "Chrome", vec![hist])], &iocs);
    let vals: Vec<String> = m
        .iter()
        .filter_map(|r| match r.get("Indicateur") {
            Some(Cell::Text(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert!(vals.iter().any(|v| v == "evil-domain.ru"), "domaine non trouvé");
    assert!(vals.iter().any(|v| v == "suspect@mail.com"), "email non trouvé");
    // Pas de correspondance pour l'URL d'actualité.
    assert!(!m.iter().any(|r| matches!(r.get("Valeur"), Some(Cell::Text(s)) if s.contains("politique"))));
}

#[test]
fn sync_accounts_and_extensions() {
    let dir = std::env::temp_dir().join(format!("nf_profile_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Preferences Chromium avec un compte.
    std::fs::write(
        dir.join("Preferences"),
        r#"{"account_info":[{"email":"jean.dupont@gmail.com","full_name":"Jean Dupont"}]}"#,
    )
    .unwrap();

    // Une extension avec manifest.
    let ext = dir.join("Extensions").join("abcdefghij").join("1.2.0");
    std::fs::create_dir_all(&ext).unwrap();
    std::fs::write(
        ext.join("manifest.json"),
        r#"{"name":"MetaMask","version":"1.2.0","permissions":["tabs","storage"],"host_permissions":["<all_urls>"]}"#,
    )
    .unwrap();

    let db = dir.join("History");
    std::fs::write(&db, b"placeholder").unwrap();
    let arts = vec![art_with(db.to_str().unwrap(), "Chrome", vec![])];

    let sync = nf::build_sync_accounts(&arts);
    assert!(
        sync.iter().any(|r| matches!(r.get("Compte"), Some(Cell::Text(s)) if s == "jean.dupont@gmail.com")),
        "compte synchronisé non détecté"
    );

    let exts = nf::build_extensions(&arts);
    let ext_row = exts
        .iter()
        .find(|r| matches!(r.get("Extension"), Some(Cell::Text(s)) if s == "MetaMask"))
        .expect("extension non détectée");
    assert!(matches!(ext_row.get("Permissions"), Some(Cell::Text(s)) if s.contains("tabs") && s.contains("all_urls")));

    std::fs::remove_dir_all(&dir).ok();
}
