use navforensics_engine as nf;
use nf::model::{Artifact, Category, Cell, Row};

fn hrow(url: &str, title: &str) -> Row {
    let mut r = Row::new();
    r.set("URL", Cell::text(url));
    r.set("Titre", Cell::text(title));
    r
}

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

#[test]
fn extracts_addresses_platforms_and_seed() {
    let hist = Category::new(
        "Historique (URLs)",
        &["URL", "Titre"],
        vec![
            hrow(
                "https://etherscan.io/address/0x52908400098527886E0F7030069857D2E4169EE7",
                "Etherscan",
            ),
            hrow("https://www.binance.com/fr/trade", "Binance"),
            hrow(
                "https://example.org/pay?to=1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
                "Don BTC",
            ),
            hrow("https://news.example/articles/politique", "Rien d'ici"),
        ],
    );
    let autofill = Category::new(
        "Saisie auto (Autofill)",
        &["Champ", "Valeur"],
        vec![{
            let mut r = Row::new();
            r.set("Champ", Cell::text("backup"));
            r.set(
                "Valeur",
                Cell::text("legal winner thank year wave sausage worth useful legal winner thank yellow"),
            );
            r
        }],
    );

    let rows = nf::build_crypto(&[art(vec![hist, autofill])]);
    let has = |typ: &str| rows.iter().any(|r| matches!(r.get("Type"), Some(Cell::Text(s)) if s == typ));

    assert!(has("Adresse Ethereum (EVM)"), "ETH non détectée");
    assert!(has("Adresse Bitcoin"), "BTC non détectée");
    assert!(has("Exchange"), "plateforme exchange non détectée");
    assert!(has("Explorateur"), "explorateur non détecté");
    assert!(has("Phrase mnémonique possible"), "seed non détectée");

    // Pas de faux positif grossier : l'URL d'actualité ne produit rien.
    let vals: Vec<String> = rows
        .iter()
        .filter_map(|r| match r.get("Valeur") {
            Some(Cell::Text(s)) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert!(!vals.iter().any(|v| v.contains("politique")));

    // Dédoublonnage : une adresse vue deux fois n'apparaît qu'une fois.
    let eth_count = rows
        .iter()
        .filter(|r| matches!(r.get("Type"), Some(Cell::Text(s)) if s == "Adresse Ethereum (EVM)"))
        .count();
    assert_eq!(eth_count, 1);
}
