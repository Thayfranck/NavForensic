// Valide l'algorithme de regroupement Navigateur -> sous-categorie (identique a
// celui de src-tauri/src/main.rs::browser_groups) et la detection des liens
// signales, avec les vrais types de l'engine. Compile sans GTK.

use navforensics_engine as nf;
use nf::model::{Artifact, Category, Cell, Row};

fn row(url: &str, title: &str) -> Row {
    let mut r = Row::new();
    r.set("URL", Cell::text(url));
    r.set("Titre", Cell::text(title));
    r
}

fn firefox_artifact() -> Artifact {
    let hist = Category::new(
        "Historique (URLs)",
        &["URL", "Titre", "Visites", "Derniere visite"],
        vec![
            row("https://www.wikipedia.org/wiki/Rust", "Rust"),
            row("http://abcdef0123456789.onion/market", "marche cache"),
            row("https://www.pornhub.com/view", "adulte"),
            row("http://185.220.101.5/payload.exe", "ip directe"),
        ],
    );
    let bm = Category::new(
        "Marque-pages",
        &["Titre", "URL", "Ajoute", "Modifie"],
        vec![row("https://news.ycombinator.com", "HN")],
    );
    Artifact {
        evidence: "SCELLE-1 / places.sqlite".into(),
        path: "/tmp/places.sqlite".into(),
        sha256: "deadbeef".into(),
        browser: "Firefox".into(),
        kind: "places.sqlite".into(),
        categories: vec![hist, bm],
        notes: vec![],
    }
}

// --- structures miroir de main.rs (verifient que les types collent) ----------
struct SubItem {
    label: String,
    count: usize,
    kind: String,
    ai: Option<usize>,
    ci: Option<usize>,
    browser: Option<String>,
}
struct SubGroup {
    name: String,
    items: Vec<SubItem>,
}
struct BrowserGroup {
    browser: String,
    subs: Vec<SubGroup>,
}

fn browser_groups(arts: &[Artifact]) -> Vec<BrowserGroup> {
    let mut browsers: Vec<String> = Vec::new();
    for art in arts.iter() {
        if !browsers.iter().any(|b| b == &art.browser) {
            browsers.push(art.browser.clone());
        }
    }
    let mut groups: Vec<BrowserGroup> = Vec::new();
    for b in &browsers {
        let mut buckets: Vec<(String, Vec<SubItem>)> = nf::SUBCATEGORY_ORDER
            .iter()
            .map(|n| (n.to_string(), Vec::new()))
            .collect();
        for (ai, art) in arts.iter().enumerate() {
            if &art.browser != b {
                continue;
            }
            for (ci, cat) in art.categories.iter().enumerate() {
                let sub = nf::sub_category(&cat.name);
                if let Some(slot) = buckets.iter_mut().find(|(n, _)| n.as_str() == sub) {
                    slot.1.push(SubItem {
                        label: format!("{} — {}", art.evidence, cat.name),
                        count: cat.rows.len(),
                        kind: "cat".to_string(),
                        ai: Some(ai),
                        ci: Some(ci),
                        browser: None,
                    });
                }
            }
        }
        let mut subs: Vec<SubGroup> = buckets
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(name, items)| SubGroup { name, items })
            .collect();
        let flagged_count = nf::build_flagged(arts, Some(b)).len();
        if flagged_count > 0 {
            subs.push(SubGroup {
                name: nf::FLAGGED_SUB.to_string(),
                items: vec![SubItem {
                    label: format!("Liens signales ({})", flagged_count),
                    count: flagged_count,
                    kind: "flagged".to_string(),
                    ai: None,
                    ci: None,
                    browser: Some(b.clone()),
                }],
            });
        }
        groups.push(BrowserGroup {
            browser: b.clone(),
            subs,
        });
    }
    groups
}

#[test]
fn grouping_and_flagging() {
    let arts = vec![firefox_artifact()];
    let groups = browser_groups(&arts);

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].browser, "Firefox");

    let names: Vec<&str> = groups[0].subs.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Historique & navigation"));
    assert!(names.contains(&"Favoris"));
    assert!(names.contains(&nf::FLAGGED_SUB)); // "Liens signales"

    // 3 liens signales attendus : onion, pornhub, IP directe (wikipedia ignore)
    let flagged = nf::build_flagged(&arts, None);
    assert_eq!(flagged.len(), 3, "liens signales: {:?}", flagged.len());
    let sigs: Vec<String> = flagged
        .iter()
        .map(|r| r.get("Signalement").map(|c| c.as_search()).unwrap_or_default())
        .collect();
    assert!(sigs.iter().any(|s| s == "Tor (.onion)"));
    assert!(sigs.iter().any(|s| s == "Adulte"));
    assert!(sigs.iter().any(|s| s == "Suspect"));

    // filtrage par navigateur inexistant -> 0
    assert_eq!(nf::build_flagged(&arts, Some("Safari")).len(), 0);
}

#[test]
fn html_report_structure() {
    let arts = vec![firefox_artifact()];
    let dir = std::env::temp_dir();
    let path = dir.join("nf_report_test.html");
    let case = nf::CaseInfo::default();
    nf::report::build_html_report(path.to_str().unwrap(), &case, &arts, "UTC", None, None, None, None, &nf::ReportOptions::default())
        .expect("report");
    let html = std::fs::read_to_string(&path).unwrap();
    assert!(html.contains("Navigateur : Firefox"), "entete navigateur");
    assert!(html.contains("Liens signales — synthese"), "synthese globale");
    assert!(html.contains(">Favoris</h3>"), "sous-categorie favoris");
    assert!(html.contains(">Historique &amp; navigation</h3>"), "sous-cat historique");
    assert!(html.contains("Avertissement"), "note de prudence");
    assert!(html.contains("class='toc'"), "sommaire present");
    assert!(html.contains("href='#sources'"), "ancre sommaire");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn report_tags_source_columns_and_gate() {
    let arts = vec![firefox_artifact()];
    let path = std::env::temp_dir().join("nf_tags_report.html");
    let case = nf::CaseInfo::default();
    let mut t = Row::new();
    t.set("Marqueur", Cell::text("Preuve"));
    t.set("Navigateur", Cell::text("Firefox"));
    t.set("Source (chemin)", Cell::text("E:\\scelle\\places.sqlite"));
    t.set("Modifie", Cell::Time(1_700_000_000_000));
    t.set("Taille", Cell::Int(524288));
    t.set("Element", Cell::text("http://x.onion"));
    t.set("Note", Cell::text("a verifier"));
    t.set("Date", Cell::text("2026-11-25T14:00:00Z"));
    t.set("_couleur", Cell::text("#ef4444"));
    let tags = vec![t];

    let opt = nf::ReportOptions::default();
    nf::report::build_html_report(path.to_str().unwrap(), &case, &arts, "UTC", None, None, Some(&tags), None, &opt).unwrap();
    let html = std::fs::read_to_string(&path).unwrap();
    assert!(html.contains(">Source (chemin)</th>"), "colonne chemin");
    assert!(html.contains("E:\\scelle\\places.sqlite"), "chemin affiche");
    assert!(html.contains("id='tags'"), "section presente");

    let opt2 = nf::ReportOptions { include_tags: false, ..nf::ReportOptions::default() };
    nf::report::build_html_report(path.to_str().unwrap(), &case, &arts, "UTC", None, None, Some(&tags), None, &opt2).unwrap();
    let html2 = std::fs::read_to_string(&path).unwrap();
    assert!(!html2.contains("id='tags'"), "section tags masquee");

    let _ = std::fs::remove_file(&path);
}
