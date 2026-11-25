use navforensics_engine as nf;
use nf::model::{Artifact, Category, Cell, Row};

fn hrow(url: &str, title: &str, ms: i64) -> Row {
    let mut r = Row::new();
    r.set("URL", Cell::text(url));
    r.set("Titre", Cell::text(title));
    r.set("Derniere visite", Cell::Time(ms));
    r
}

fn artifact() -> Artifact {
    let hist = Category::new(
        "Historique (URLs)",
        &["URL", "Titre", "Visites", "Derniere visite"],
        vec![
            hrow("https://www.google.com/search?q=rust+programmation&hl=fr", "Google", 1_700_000_000_000),
            hrow("https://duckduckgo.com/?q=forensic%20browser", "DDG", 1_700_000_100_000),
            hrow("https://www.bing.com/search?q=cellebrite", "Bing", 1_700_000_200_000),
            hrow("https://www.wikipedia.org/wiki/Rust", "pas une recherche", 1_700_000_300_000),
        ],
    );
    Artifact {
        evidence: "SCELLE-1 / places.sqlite".into(),
        path: "/tmp/places.sqlite".into(),
        sha256: "x".into(),
        browser: "Firefox".into(),
        kind: "places.sqlite".into(),
        categories: vec![hist],
        notes: vec![],
    }
}

#[test]
fn extracts_decoded_search_terms() {
    let arts = vec![artifact()];
    let terms = nf::build_search_terms(&arts);
    assert_eq!(terms.len(), 3, "3 recherches attendues (wikipedia ignore)");

    let get = |r: &Row, c: &str| r.get(c).map(|x| x.as_search()).unwrap_or_default();
    // triees par date decroissante -> Bing (plus recent) en premier
    assert_eq!(get(&terms[0], "Moteur"), "Bing");
    assert_eq!(get(&terms[0], "Terme recherche"), "cellebrite");
    // decodage %20 et '+'
    let ddg = terms.iter().find(|r| get(r, "Moteur") == "DuckDuckGo").unwrap();
    assert_eq!(get(ddg, "Terme recherche"), "forensic browser");
    let goog = terms.iter().find(|r| get(r, "Moteur") == "Google").unwrap();
    assert_eq!(get(goog, "Terme recherche"), "rust programmation");
}

#[test]
fn stats_summary_has_core_rubrics() {
    let arts = vec![artifact()];
    let stats = nf::build_stats(&arts);
    let has = |rub: &str| {
        stats
            .iter()
            .any(|r| r.get("Rubrique").map(|c| c.as_search()).unwrap_or_default() == rub)
    };
    assert!(has("Periode"));
    assert!(has("Volume"));
    assert!(has("Par navigateur"));
    assert!(has("Recherches"));
    assert!(has("Domaines"));
}

fn cookies_artifact() -> Artifact {
    let mut c1 = Row::new();
    c1.set("Hote", Cell::text(".example.com"));
    c1.set("Nom", Cell::text("__utma"));
    c1.set("Valeur", Cell::text("123456789.987654321.1609459200.1612137600.1614556800.5"));
    let mut c2 = Row::new();
    c2.set("Hote", Cell::text(".site.fr"));
    c2.set("Nom", Cell::text("_ga"));
    c2.set("Valeur", Cell::text("GA1.2.1234567890.1609459200"));
    let cat = Category::new(
        "Cookies",
        &["Hote", "Nom", "Chemin", "Valeur", "Creation", "Dernier acces", "Expiration", "Securise", "HttpOnly"],
        vec![c1, c2],
    );
    Artifact { evidence: "SCELLE / cookies.sqlite".into(), path: "x".into(), sha256: "x".into(),
        browser: "Firefox".into(), kind: "cookies.sqlite".into(), categories: vec![cat], notes: vec![] }
}

fn visites_artifact() -> Artifact {
    let mk = |ms: i64| { let mut r = Row::new(); r.set("URL", Cell::text("https://x.fr")); r.set("Date visite", Cell::Time(ms)); r };
    let cat = Category::new("Visites", &["Date visite", "URL", "Titre", "Type", "Depuis visite #"],
        vec![mk(1_609_459_200_000), mk(1_609_462_800_000), mk(1_609_545_600_000)]);
    Artifact { evidence: "SCELLE / places.sqlite".into(), path: "x".into(), sha256: "x".into(),
        browser: "Firefox".into(), kind: "places.sqlite".into(), categories: vec![cat], notes: vec![] }
}

#[test]
fn ga_cookie_timestamps() {
    let arts = vec![cookies_artifact()];
    let rows = nf::build_ga_cookies(&arts);
    // 3 (utma) + 1 (_ga) = 4
    assert_eq!(rows.len(), 4, "lignes GA");
    let roles: Vec<String> = rows.iter()
        .map(|r| r.get("Role").map(|c| c.as_search()).unwrap_or_default()).collect();
    assert!(roles.iter().any(|s| s == "1ere visite"));
    assert!(roles.iter().any(|s| s == "Derniere visite"));
    assert!(roles.iter().any(|s| s == "1ere visite (GA4)"));
    // premier horodatage = 2021-01-01 00:00:00 UTC = 1609459200000 ms
    let has_ts = rows.iter().any(|r| r.get("Horodatage").and_then(|c| c.time_ms()) == Some(1_609_459_200_000));
    assert!(has_ts, "horodatage __utma decode");
}

#[test]
fn histogram_shape() {
    let arts = vec![visites_artifact()];
    let h = nf::build_histogram(&arts);
    assert_eq!(h.len(), 24 + 7, "24 heures + 7 jours");
    let total: i64 = h.iter()
        .filter(|r| r.get("Categorie").map(|c| c.as_search()).unwrap_or_default() == "Heure (UTC)")
        .map(|r| match r.get("Evenements") { Some(nf::model::Cell::Int(n)) => *n, _ => 0 })
        .sum();
    assert_eq!(total, 3, "3 evenements repartis sur les heures");
}

fn online_artifact() -> Artifact {
    let hist = Category::new("Historique (URLs)", &["URL","Titre","Visites","Derniere visite"], vec![
        hrow("https://drive.google.com/drive/my-drive", "Drive", 1_700_000_000_000),
        hrow("https://mail.google.com/mail/u/0/", "Gmail", 1_700_000_100_000),
        hrow("https://www.dropbox.com/home", "Dropbox", 1_700_000_200_000),
        hrow("https://www.wikipedia.org/", "neutre", 1_700_000_300_000),
    ]);
    Artifact { evidence: "S/places.sqlite".into(), path: "x".into(), sha256: "x".into(),
        browser: "Firefox".into(), kind: "places.sqlite".into(), categories: vec![hist], notes: vec![] }
}

#[test]
fn online_services_detected() {
    let arts = vec![online_artifact()];
    let rows = nf::build_online_services(&arts);
    assert_eq!(rows.len(), 3, "3 services (wikipedia ignore)");
    let svc: Vec<String> = rows.iter().map(|r| r.get("Service").map(|c| c.as_search()).unwrap_or_default()).collect();
    assert!(svc.iter().any(|s| s == "Google Drive"));
    assert!(svc.iter().any(|s| s == "Gmail"));
    assert!(svc.iter().any(|s| s == "Dropbox"));
}

#[test]
fn keyword_list_search() {
    let arts = vec![artifact()]; // contient "cellebrite" dans une URL Bing
    let kw = vec![
        ("cellebrite".to_string(), "Outils forensiques".to_string()),
        ("forensic".to_string(), "Outils forensiques".to_string()),
    ];
    let hits = nf::search_keywords(&arts, &kw);
    assert!(!hits.is_empty(), "au moins un hit");
    assert!(hits.iter().any(|r| r.get("Mot-cle").map(|c| c.as_search()).unwrap_or_default() == "cellebrite"));
    assert!(hits.iter().all(|r| r.get("Liste").map(|c| c.as_search()).unwrap_or_default() == "Outils forensiques"));
}

fn af_row(champ: &str, valeur: &str) -> Row {
    let mut r = Row::new();
    r.set("Champ", Cell::text(champ));
    r.set("Valeur", Cell::text(valeur));
    r
}
fn identity_artifact() -> Artifact {
    let af = Category::new(
        "Saisie auto (Autofill)",
        &["Champ", "Valeur", "Occurrences", "Premiere utilisation", "Derniere utilisation"],
        vec![
            af_row("email", "jean.dupont@example.fr"),
            af_row("phone", "0612345678"),
            af_row("cc-number", "4111 1111 1111 1111"),
            af_row("iban", "FR7630006000011234567890189"),
            af_row("firstname", "Jean"),
            af_row("adresse", "12 rue des Lilas 75010 Paris"),
            af_row("random", "bonjour"),
        ],
    );
    let dl = Category::new(
        "Telechargements",
        &["Date", "URL", "Fichier", "Recu (o)", "Total (o)"],
        vec![{
            let mut r = Row::new();
            r.set("Date", Cell::Time(1_700_000_000_000));
            r.set("URL", Cell::text("https://x.fr/setup.exe"));
            r.set("Fichier", Cell::text("setup.exe"));
            r.set("Total (o)", Cell::Int(1048576));
            r
        }],
    );
    Artifact { evidence: "S/Web Data".into(), path: "x".into(), sha256: "x".into(),
        browser: "Google Chrome".into(), kind: "Web Data".into(), categories: vec![af, dl], notes: vec![] }
}

#[test]
fn identity_and_downloads() {
    let arts = vec![identity_artifact()];
    let ids = nf::build_identity(&arts);
    let types: Vec<String> = ids.iter().map(|r| r.get("Type").map(|c| c.as_search()).unwrap_or_default()).collect();
    assert!(types.contains(&"Email".to_string()), "email: {:?}", types);
    assert!(types.contains(&"Telephone".to_string()), "tel");
    assert!(types.contains(&"Carte bancaire".to_string()), "CB (Luhn)");
    assert!(types.contains(&"IBAN / banque".to_string()), "IBAN");
    assert!(types.contains(&"Nom / Prenom".to_string()), "nom");
    assert!(types.contains(&"Adresse".to_string()), "adresse");
    // "random/bonjour" ne doit PAS etre classe
    assert!(!ids.iter().any(|r| r.get("Valeur").map(|c| c.as_search()).unwrap_or_default() == "bonjour"));

    let dls = nf::build_downloads(&arts);
    assert_eq!(dls.len(), 1);
    assert_eq!(dls[0].get("Fichier").map(|c| c.as_search()).unwrap_or_default(), "setup.exe");
}
