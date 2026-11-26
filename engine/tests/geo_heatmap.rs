use navforensics_engine as nf;
use nf::model::{Artifact, Category, Cell, Row};

fn art(cats: Vec<Category>) -> Artifact {
    Artifact {
        evidence: "S".into(),
        path: "x".into(),
        sha256: "x".into(),
        browser: "Chrome".into(),
        kind: "History".into(),
        categories: cats,
        notes: vec![],
    }
}

fn urow(url: &str, title: &str, ms: i64) -> Row {
    let mut r = Row::new();
    r.set("URL", Cell::text(url));
    r.set("Titre", Cell::text(title));
    r.set("Derniere visite", Cell::Time(ms));
    r
}

#[test]
fn geopoints_extracted() {
    let hist = Category::new(
        "Historique (URLs)",
        &["URL", "Titre", "Derniere visite"],
        vec![
            urow("https://www.google.com/maps/@48.8584,2.2945,17z", "Tour Eiffel", 1_700_000_000_000),
            urow("https://www.openstreetmap.org/#map=12/45.7640/4.8357", "Lyon", 1_700_000_100_000),
            urow("https://news.example/article", "Sans coordonnées", 1_700_000_200_000),
        ],
    );
    let pts = nf::build_geopoints(&[art(vec![hist])]);
    assert_eq!(pts.len(), 2, "deux points attendus");
    let lat0 = match pts[0].get("Latitude") {
        Some(Cell::Text(s)) => s.clone(),
        _ => String::new(),
    };
    assert!(lat0.starts_with("48.858"), "latitude Tour Eiffel: {lat0}");
}

#[test]
fn heatmap_grid() {
    // 1700000000000 ms = 2023-11-14 22:13:20 UTC (mardi, 22h).
    let mk = |url: &str, ms: i64| {
        let mut r = Row::new();
        r.set("URL", Cell::text(url));
        r.set("Titre", Cell::text("x"));
        r.set("Date visite", Cell::Time(ms));
        r
    };
    let hist = Category::new(
        "Visites",
        &["URL", "Titre", "Date visite"],
        vec![mk("https://a.example/", 1_700_000_000_000), mk("https://b.example/", 1_700_000_000_000)],
    );
    let hm = nf::build_heatmap(&[art(vec![hist])], "UTC");
    assert_eq!(hm.len(), 7, "sept lignes (jours)");
    // Mardi = index 1, 22h.
    let mardi = &hm[1];
    let n22 = match mardi.get("22h") {
        Some(Cell::Int(n)) => *n,
        _ => -1,
    };
    assert_eq!(n22, 2, "deux événements mardi 22h");
}
