//! Formatage des cellules et exports (CSV, XLSX, rapport HTML).

use crate::model::{Artifact, Cell, Row};
use crate::taxonomy::{build_flagged, sub_category, FLAGGED_COLUMNS, SUBCATEGORY_ORDER};
use crate::browser_version_install;
use crate::views::{
    build_downloads, build_ga_cookies, build_geolocation, build_histogram, build_identity,
    build_online_services, build_search_terms, build_stats, build_vpn, DOMAIN_COLUMNS,
    DOWNLOADS_COLUMNS, GA_COLUMNS, GEO_COLUMNS, HISTO_COLUMNS, IDENTITY_COLUMNS, ONLINE_COLUMNS,
    SEARCH_TERMS_COLUMNS, STATS_COLUMNS, TAG_COLUMNS, TIMELINE_COLUMNS, VPN_COLUMNS,
};
use chrono::{Local, TimeZone, Utc};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Default)]
pub struct CaseInfo {
    pub numero: String,
    pub examinateur: String,
    pub service: String,
    pub notes: String,
}

/// Options de génération du rapport (sections à inclure, mode « éléments
/// marqués uniquement »).
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ReportOptions {
    pub only_tagged: bool,
    pub include_tags: bool,
    pub include_stats: bool,
    pub include_histogram: bool,
    pub include_timeline: bool,
    pub include_domains: bool,
    pub include_terms: bool,
    pub include_ga: bool,
    pub include_online: bool,
    pub include_flagged: bool,
    pub include_details: bool,
    pub include_identity: bool,
    pub include_downloads: bool,
    pub include_geolocation: bool,
    pub include_vpn: bool,
}

impl Default for ReportOptions {
    fn default() -> Self {
        ReportOptions {
            only_tagged: false,
            include_tags: true,
            include_stats: true,
            include_histogram: true,
            include_timeline: true,
            include_domains: true,
            include_terms: true,
            include_ga: true,
            include_online: true,
            include_flagged: true,
            include_details: true,
            include_identity: true,
            include_downloads: true,
            include_geolocation: true,
            include_vpn: true,
        }
    }
}

pub fn fmt_cell(cell: Option<&Cell>, tz: &str) -> String {
    match cell {
        None | Some(Cell::Null) => String::new(),
        Some(Cell::Text(s)) => s.clone(),
        Some(Cell::Int(n)) => n.to_string(),
        Some(Cell::Time(ms)) => fmt_time(*ms, tz),
    }
}

pub fn fmt_time(ms: i64, tz: &str) -> String {
    match Utc.timestamp_millis_opt(ms).single() {
        None => String::new(),
        Some(dt) => {
            if tz == "Local" {
                dt.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S %Z").to_string()
            } else {
                dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
            }
        }
    }
}

fn csv_field(s: &str) -> String {
    if s.contains(';') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn export_csv(path: &str, columns: &[String], rows: &[Row], tz: &str) -> Result<(), String> {
    let mut s = String::new();
    s.push('\u{feff}');
    s.push_str(&columns.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(";"));
    s.push_str("\r\n");
    for r in rows {
        let line: Vec<String> = columns
            .iter()
            .map(|c| csv_field(&fmt_cell(r.get(c), tz)))
            .collect();
        s.push_str(&line.join(";"));
        s.push_str("\r\n");
    }
    std::fs::write(path, s).map_err(|e| e.to_string())
}

pub fn export_xlsx(
    path: &str,
    columns: &[String],
    rows: &[Row],
    tz: &str,
    sheet: &str,
) -> Result<(), String> {
    use rust_xlsxwriter::{Format, Workbook};
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    let title: String = sheet.chars().take(31).collect();
    ws.set_name(if title.is_empty() { "Donnees" } else { &title })
        .map_err(|e| e.to_string())?;
    let head = Format::new().set_bold();
    for (i, c) in columns.iter().enumerate() {
        ws.write_string_with_format(0, i as u16, c, &head)
            .map_err(|e| e.to_string())?;
    }
    for (ri, r) in rows.iter().enumerate() {
        for (ci, c) in columns.iter().enumerate() {
            ws.write_string((ri + 1) as u32, ci as u16, fmt_cell(r.get(c), tz))
                .map_err(|e| e.to_string())?;
        }
    }
    ws.set_freeze_panes(1, 0).map_err(|e| e.to_string())?;
    wb.save(path).map_err(|e| e.to_string())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

const CSS: &str = "body{font-family:Segoe UI,Arial,sans-serif;margin:24px;color:#111;background:#fff;font-size:13px}h1{font-size:20px;margin:0 0 4px}h2{font-size:16px;margin:26px 0 6px;border-bottom:2px solid #1F2937;padding-bottom:3px}h3{font-size:14px;margin:16px 0 4px;color:#1F2937;border-bottom:1px solid #cbd5e1;padding-bottom:2px}h4{font-size:12.5px;margin:10px 0 3px;color:#475569;font-weight:600}.meta{border-collapse:collapse;margin:8px 0}.meta td{border:1px solid #cbd5e1;padding:4px 8px;vertical-align:top}.meta td.k{background:#f1f5f9;font-weight:bold;white-space:nowrap}table.data{border-collapse:collapse;width:100%;margin:6px 0;table-layout:fixed}table.data th{background:#1F2937;color:#fff;text-align:left;padding:4px 6px;font-size:12px}table.data td{border:1px solid #e2e8f0;padding:3px 6px;font-size:12px;word-break:break-word;vertical-align:top}table.data tr:nth-child(even) td{background:#f8fafc}.hash{font-family:Consolas,monospace;font-size:11px;word-break:break-all}.count{color:#475569}.note{background:#fef3c7;border:1px solid #f59e0b;border-radius:4px;padding:7px 10px;margin:6px 0;font-size:11.5px;color:#7c2d12}.browser{background:#eff6ff;border-left:4px solid #1F2937;padding:4px 10px;margin:30px 0 4px;font-size:17px;font-weight:700}.foot{margin-top:30px;color:#64748b;font-size:11px;border-top:1px solid #cbd5e1;padding-top:8px}.layout{display:flex;align-items:flex-start;gap:20px}.toc{position:sticky;top:0;flex:0 0 240px;max-height:100vh;overflow:auto;font-size:12px;border-right:1px solid #cbd5e1;padding:4px 10px 10px 0}.toc .tochdr{font-weight:700;color:#1F2937;margin:0 0 6px}.toc a{display:block;color:#1d4ed8;text-decoration:none;padding:2px 0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.toc a:hover{text-decoration:underline}.toc a.l2{padding-left:14px;color:#475569;font-size:11px}.content{flex:1;min-width:0}.totop{position:fixed;right:18px;bottom:18px;background:#1F2937;color:#fff;border-radius:50%;width:40px;height:40px;line-height:40px;text-align:center;text-decoration:none;font-size:18px;box-shadow:0 2px 6px rgba(0,0,0,.3)}@media print{.toc,.totop{display:none}.layout{display:block}}";

const FLAGGED_NOTE: &str = "<div class='note'><b>Avertissement :</b> signalements automatiques fondes sur des motifs d'URL (Tor .onion, domaines et mots-cles a caractere adulte, indicateurs suspects). Ce sont des pistes de tri a VERIFIER, et non une preuve d'infraction. L'identification de contenus illegaux, en particulier impliquant des mineurs, releve d'outils certifies (correspondance d'empreintes) et du cadre judiciaire.</div>";

fn html_table(out: &mut String, columns: &[&str], rows: &[Row], tz: &str) {
    out.push_str("<table class='data'><thead><tr>");
    for c in columns {
        out.push_str(&format!("<th>{}</th>", esc(c)));
    }
    out.push_str("</tr></thead><tbody>");
    for r in rows {
        out.push_str("<tr>");
        for c in columns {
            out.push_str(&format!("<td>{}</td>", esc(&fmt_cell(r.get(*c), tz))));
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody></table>");
}

/// Variante de html_table pour les elements marques : colore la cellule
/// "Marqueur" avec la couleur de la balise (valeur cachee "_couleur").
fn html_tags_table(out: &mut String, columns: &[&str], rows: &[Row], tz: &str) {
    out.push_str("<table class='data'><thead><tr>");
    for c in columns {
        out.push_str(&format!("<th>{}</th>", esc(c)));
    }
    out.push_str("</tr></thead><tbody>");
    for r in rows {
        let color = r.get("_couleur").map(|c| c.as_search()).unwrap_or_default();
        out.push_str("<tr>");
        for c in columns {
            let val = esc(&fmt_cell(r.get(*c), tz));
            if *c == "Marqueur" && !color.is_empty() {
                out.push_str(&format!(
                    "<td><span style='display:inline-block;width:10px;height:10px;border-radius:2px;background:{};margin-right:6px;vertical-align:middle'></span>{}</td>",
                    esc(&color), val
                ));
            } else {
                out.push_str(&format!("<td>{}</td>", val));
            }
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody></table>");
}

#[allow(clippy::too_many_arguments)]
pub fn build_html_report(
    path: &str,
    case: &CaseInfo,
    arts: &[Artifact],
    tz: &str,
    timeline: Option<&[Row]>,
    domains: Option<&[Row]>,
    tags: Option<&[Row]>,
    identity_pick: Option<&[Row]>,
    options: &ReportOptions,
) -> Result<(), String> {
    let gen = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();

    // Entrees du sommaire : (niveau 1|2, id d'ancre, libelle)
    let mut toc: Vec<(u8, String, String)> = Vec::new();
    // Corps principal (a droite du sommaire)
    let mut c = String::new();

    toc.push((1, "sources".into(), "Sources & empreintes".into()));
    c.push_str("<h2 id='sources'>Sources analysees &amp; empreintes</h2>");
    c.push_str("<table class='meta'><tr><td class='k'>Scelle / libelle</td><td class='k'>Navigateur</td><td class='k'>Version</td><td class='k'>Installe</td><td class='k'>Base</td><td class='k'>SHA-256</td></tr>");
    for a in arts {
        let (ver, inst) = browser_version_install(&a.path);
        let inst_txt = inst
            .map(|ms| fmt_cell(Some(&Cell::Time(ms)), tz))
            .unwrap_or_else(|| "—".into());
        c.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class='hash'>{}</td></tr>",
            esc(&a.evidence),
            esc(&a.browser),
            esc(ver.as_deref().unwrap_or("—")),
            esc(&inst_txt),
            esc(&a.kind),
            esc(&a.sha256)
        ));
    }
    c.push_str("</table>");

    // Mode « éléments marqués uniquement » : rapport restreint aux tags,
    // regroupés par contexte (leur catégorie d'origine), + données du dossier.
    if options.only_tagged {
        let tg = tags.unwrap_or(&[]);
        toc.push((1, "tags".into(), "Elements marques".into()));
        c.push_str(&format!(
            "<h2 id='tags'>Elements marques <span class='count'>({} entrees)</span></h2>",
            tg.len()
        ));
        if tg.is_empty() {
            c.push_str("<div class='note'>Aucun element marque.</div>");
        } else {
            let mut contexts: Vec<String> = Vec::new();
            for r in tg {
                let ctx = r.get("Contexte").map(|x| x.as_search()).unwrap_or_default();
                if !contexts.iter().any(|x| x == &ctx) {
                    contexts.push(ctx);
                }
            }
            for ctx in &contexts {
                let subset: Vec<Row> = tg
                    .iter()
                    .filter(|r| r.get("Contexte").map(|x| x.as_search()).unwrap_or_default() == *ctx)
                    .cloned()
                    .collect();
                let label = if ctx.is_empty() { "(sans contexte)" } else { ctx.as_str() };
                c.push_str(&format!("<h3>{}</h3>", esc(label)));
                html_tags_table(&mut c, TAG_COLUMNS, &subset, tz);
            }
        }
    } else {
        // --- Statistiques (synthese) ---
        if options.include_stats {
            let stats = build_stats(arts);
            if !stats.is_empty() {
                toc.push((1, "stats".into(), "Statistiques".into()));
                c.push_str("<h2 id='stats'>Statistiques</h2>");
                html_table(&mut c, STATS_COLUMNS, &stats, tz);
            }
        }

        // --- Elements marques (tagging d'investigation) ---
        if options.include_tags {
            if let Some(tg) = tags {
                if !tg.is_empty() {
                    toc.push((1, "tags".into(), "Elements marques".into()));
                    c.push_str(&format!(
                        "<h2 id='tags'>Elements marques <span class='count'>({} entrees)</span></h2>",
                        tg.len()
                    ));
                    html_tags_table(&mut c, TAG_COLUMNS, tg, tz);
                }
            }
        }

        // --- Histogramme d'activite (heure / jour) ---
        if options.include_histogram {
            let histo = build_histogram(arts);
            let histo_any = histo
                .iter()
                .any(|r| matches!(r.get("Evenements"), Some(Cell::Int(n)) if *n > 0));
            if histo_any {
                toc.push((1, "histo".into(), "Histogramme d'activite".into()));
                c.push_str("<h2 id='histo'>Histogramme d'activite <span class='count'>(par heure / jour, UTC)</span></h2>");
                html_table(&mut c, HISTO_COLUMNS, &histo, tz);
            }
        }

        if options.include_timeline {
            if let Some(rows) = timeline {
                if !rows.is_empty() {
                    toc.push((1, "timeline".into(), "Timeline unifiee".into()));
                    c.push_str(&format!(
                        "<h2 id='timeline'>Timeline unifiee <span class='count'>({} evenements)</span></h2>",
                        rows.len()
                    ));
                    html_table(&mut c, TIMELINE_COLUMNS, rows, tz);
                }
            }
        }
        if options.include_domains {
            if let Some(rows) = domains {
                if !rows.is_empty() {
                    toc.push((1, "domains".into(), "Regroupement par domaine".into()));
                    c.push_str(&format!(
                        "<h2 id='domains'>Regroupement par domaine <span class='count'>({} domaines)</span></h2>",
                        rows.len()
                    ));
                    html_table(&mut c, DOMAIN_COLUMNS, rows, tz);
                }
            }
        }

        // --- Termes de recherche extraits ---
        if options.include_terms {
            let terms = build_search_terms(arts);
            if !terms.is_empty() {
                toc.push((1, "terms".into(), "Termes recherches".into()));
                c.push_str(&format!(
                    "<h2 id='terms'>Termes recherches <span class='count'>({} termes)</span></h2>",
                    terms.len()
                ));
                html_table(&mut c, SEARCH_TERMS_COLUMNS, &terms, tz);
            }
        }

        // --- Services en ligne (cloud, webmail, reseaux) ---
        if options.include_online {
            let online = build_online_services(arts);
            if !online.is_empty() {
                toc.push((1, "online".into(), "Services en ligne".into()));
                c.push_str(&format!(
                    "<h2 id='online'>Services en ligne <span class='count'>({} acces)</span></h2>",
                    online.len()
                ));
                html_table(&mut c, ONLINE_COLUMNS, &online, tz);
            }
        }

        // --- Elements d'identite retenus (selection de l'enqueteur) ---
        if let Some(idp) = identity_pick {
            if !idp.is_empty() {
                toc.push((1, "idpick".into(), "Identite retenue".into()));
                c.push_str(&format!(
                    "<h2 id='idpick'>Elements d'identite retenus <span class='count'>({} elements)</span></h2>",
                    idp.len()
                ));
                html_table(&mut c, IDENTITY_COLUMNS, idp, tz);
            }
        }
        // --- Elements d'identite (complet, autofill) ---
        if options.include_identity {
            let ids = build_identity(arts);
            if !ids.is_empty() {
                toc.push((1, "identity".into(), "Identite (autofill)".into()));
                c.push_str(&format!(
                    "<h2 id='identity'>Elements d'identite (autofill) <span class='count'>({} elements)</span></h2>",
                    ids.len()
                ));
                html_table(&mut c, IDENTITY_COLUMNS, &ids, tz);
            }
        }
        // --- Telechargements ---
        if options.include_downloads {
            let dls = build_downloads(arts);
            if !dls.is_empty() {
                toc.push((1, "downloads".into(), "Telechargements".into()));
                c.push_str(&format!(
                    "<h2 id='downloads'>Telechargements <span class='count'>({} entrees)</span></h2>",
                    dls.len()
                ));
                html_table(&mut c, DOWNLOADS_COLUMNS, &dls, tz);
            }
        }
        // --- Geolocalisations ---
        if options.include_geolocation {
            let geo = build_geolocation(arts);
            if !geo.is_empty() {
                toc.push((1, "geo".into(), "Geolocalisations".into()));
                c.push_str(&format!(
                    "<h2 id='geo'>Geolocalisations / adresses recherchees <span class='count'>({} entrees)</span></h2>",
                    geo.len()
                ));
                html_table(&mut c, GEO_COLUMNS, &geo, tz);
            }
        }
        // --- VPN / anonymiseurs ---
        if options.include_vpn {
            let vpns = build_vpn(arts);
            if !vpns.is_empty() {
                toc.push((1, "vpn".into(), "VPN".into()));
                c.push_str(&format!(
                    "<h2 id='vpn'>VPN / anonymiseurs <span class='count'>({} services)</span></h2>",
                    vpns.len()
                ));
                html_table(&mut c, VPN_COLUMNS, &vpns, tz);
            }
        }

        // --- Cookies Google Analytics (horodatages decodes) ---
        if options.include_ga {
            let ga = build_ga_cookies(arts);
            if !ga.is_empty() {
                toc.push((1, "ga".into(), "Cookies Google Analytics".into()));
                c.push_str(&format!(
                    "<h2 id='ga'>Cookies Google Analytics <span class='count'>({} horodatages)</span></h2>",
                    ga.len()
                ));
                html_table(&mut c, GA_COLUMNS, &ga, tz);
            }
        }

        // --- Synthese globale des liens signales (tous navigateurs) ---
        if options.include_flagged {
            let flagged_all = build_flagged(arts, None);
            if !flagged_all.is_empty() {
                toc.push((1, "flagged".into(), "Liens signales (synthese)".into()));
                c.push_str(&format!(
                    "<h2 id='flagged'>Liens signales — synthese <span class='count'>({} liens, tous navigateurs)</span></h2>",
                    flagged_all.len()
                ));
                c.push_str(FLAGGED_NOTE);
                html_table(&mut c, FLAGGED_COLUMNS, &flagged_all, tz);
            }
        }

        // --- Detail par navigateur, puis par sous-categorie ---
        if options.include_details {
            let mut browsers: Vec<&str> = Vec::new();
            for a in arts {
                if !browsers.iter().any(|b| *b == a.browser.as_str()) {
                    browsers.push(a.browser.as_str());
                }
            }
            for (bi, b) in browsers.iter().copied().enumerate() {
                let bid = format!("b{}", bi);
                toc.push((1, bid.clone(), format!("Navigateur : {}", b)));
                c.push_str(&format!(
                    "<div class='browser' id='{}'>Navigateur : {}</div>",
                    bid,
                    esc(b)
                ));
                let mut subidx = 0;
                for sub in SUBCATEGORY_ORDER {
                    let mut shown = false;
                    for a in arts {
                        if a.browser.as_str() != b {
                            continue;
                        }
                        for cat in &a.categories {
                            if sub_category(&cat.name) != *sub {
                                continue;
                            }
                            if !shown {
                                let sid = format!("{}-{}", bid, subidx);
                                subidx += 1;
                                toc.push((2, sid.clone(), sub.to_string()));
                                c.push_str(&format!("<h3 id='{}'>{}</h3>", sid, esc(sub)));
                                shown = true;
                            }
                            c.push_str(&format!(
                                "<h4>{} — {} <span class='count'>({} entrees)</span></h4>",
                                esc(&a.evidence),
                                esc(&cat.name),
                                cat.rows.len()
                            ));
                            let cols: Vec<&str> = cat.columns.iter().map(|s| s.as_str()).collect();
                            html_table(&mut c, &cols, &cat.rows, tz);
                        }
                    }
                }
                let flagged_b = build_flagged(arts, Some(b));
                if !flagged_b.is_empty() {
                    let sid = format!("{}-{}", bid, subidx);
                    toc.push((2, sid.clone(), "Liens signales".to_string()));
                    c.push_str(&format!(
                        "<h3 id='{}'>Liens signales <span class='count'>({} liens)</span></h3>",
                        sid,
                        flagged_b.len()
                    ));
                    html_table(&mut c, FLAGGED_COLUMNS, &flagged_b, tz);
                }
            }
        }
    }

    // --- Assemblage final : en-tete + (sommaire | contenu) ---
    let mut o = String::new();
    o.push_str("<!DOCTYPE html><html lang='fr'><head><meta charset='utf-8'><title>Rapport NavForensics</title><style>");
    o.push_str(CSS);
    o.push_str("</style></head><body id='top'>");
    o.push_str("<h1>Rapport d'analyse des navigateurs</h1>");
    o.push_str(&format!(
        "<div class='count'>NavForensics v{} — genere le {} — fuseau : {}</div>",
        env!("CARGO_PKG_VERSION"),
        esc(&gen),
        esc(tz)
    ));
    o.push_str("<table class='meta'>");
    for (k, v) in [
        ("Numero de dossier", &case.numero),
        ("Examinateur", &case.examinateur),
        ("Service", &case.service),
        ("Observations", &case.notes),
    ] {
        o.push_str(&format!("<tr><td class='k'>{}</td><td>{}</td></tr>", esc(k), esc(v)));
    }
    o.push_str("</table>");

    o.push_str("<div class='layout'><nav class='toc'><div class='tochdr'>Sommaire</div>");
    for (level, id, label) in &toc {
        o.push_str(&format!(
            "<a class='{}' href='#{}'>{}</a>",
            if *level == 2 { "l2" } else { "l1" },
            id,
            esc(label)
        ));
    }
    o.push_str("</nav><div class='content'>");
    o.push_str(&c);
    o.push_str("<div class='foot'>Rapport genere par NavForensics. Bases ouvertes en lecture seule sur copie (-wal/-shm inclus). Aucun secret dechiffre. Horodatages convertis depuis les referentiels propres a chaque navigateur. Les liens signales sont des indicateurs automatiques a verifier.</div>");
    o.push_str("</div></div>");
    o.push_str("<a class='totop' href='#top' title='Haut de page'>&#8593;</a>");
    o.push_str("</body></html>");
    std::fs::write(path, o).map_err(|e| e.to_string())
}
