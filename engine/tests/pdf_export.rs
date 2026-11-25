use navforensics_engine as nf;
use nf::model::{Artifact, Category, Cell, Row};
use nf::report::{CaseInfo, ReportOptions};

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
            hrow("https://www.google.com/search?q=rust", "Google", 1_700_000_000_000),
            hrow("https://nordvpn.com/fr/", "NordVPN", 1_700_000_100_000),
        ],
    );
    let mut dl = Row::new();
    dl.set("Date", Cell::Time(1_700_000_050_000));
    dl.set("URL", Cell::text("https://ex.fr/setup.exe"));
    dl.set("Fichier cible", Cell::text("C:/Users/J/Downloads/setup.exe"));
    let downloads = Category::new(
        "Telechargements",
        &["Date", "URL", "Fichier cible", "Recu", "Total"],
        vec![dl],
    );
    Artifact {
        evidence: "SCELLE-1 / History".into(),
        path: "/tmp/does-not-exist".into(),
        sha256: "abc123".into(),
        browser: "Chrome".into(),
        kind: "History".into(),
        categories: vec![hist, downloads],
        notes: vec![],
    }
}

#[test]
fn produces_valid_pdf() {
    let tmp = std::env::temp_dir().join("navforensics_test_report.pdf");
    let path = tmp.to_string_lossy().to_string();
    let arts = vec![artifact()];
    let case = CaseInfo {
        numero: "2026/00123".into(),
        examinateur: "B/C Franck SCHMITT".into(),
        service: "OFAC 51 — Reims".into(),
        notes: "Test export PDF".into(),
    };
    let opts = ReportOptions::default();
    nf::pdf::build_pdf_report(&path, &case, &arts, "Europe/Paris", None, None, None, None, &opts)
        .expect("PDF build ok");

    let bytes = std::fs::read(&path).expect("read pdf");
    assert!(bytes.len() > 500, "pdf trop petit: {} octets", bytes.len());
    assert_eq!(&bytes[0..5], b"%PDF-", "pas un en-tete PDF");
    std::fs::remove_file(&path).ok();
}
