//! Export PDF du rapport d'analyse NavForensics.
//!
//! Utilise `printpdf` avec les polices standard intégrées (Helvetica),
//! donc aucun asset de police à embarquer. La mise en page est
//! paginée automatiquement (format A4). Le contenu reprend fidèlement
//! les sections du rapport HTML (`report::build_html_report`) en
//! honorant les mêmes `ReportOptions`, `identity_pick` et `tags`.

use crate::browser_version_install;
use crate::model::{Artifact, Cell, Row};
use crate::report::{fmt_cell, CaseInfo, ReportOptions};
use crate::taxonomy::{build_flagged, FLAGGED_COLUMNS};
use crate::views::{
    build_downloads, build_geolocation, build_ga_cookies, build_identity, build_online_services,
    build_search_terms, build_stats, build_vpn, DOMAIN_COLUMNS, DOWNLOADS_COLUMNS, GA_COLUMNS,
    GEO_COLUMNS, IDENTITY_COLUMNS, ONLINE_COLUMNS, SEARCH_TERMS_COLUMNS, STATS_COLUMNS,
    TAG_COLUMNS, TIMELINE_COLUMNS, VPN_COLUMNS,
};
use printpdf::{BuiltinFont, IndirectFontRef, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference};
use std::fs::File;
use std::io::BufWriter;

type R<T> = Result<T, String>;

// Format A4 portrait (mm) et marges.
const PW: f32 = 210.0;
const PH: f32 = 297.0;
const ML: f32 = 14.0; // marge gauche
const MT: f32 = 283.0; // y de départ (haut)
const MB: f32 = 14.0; // marge basse
// Garde-fou : un PDF ne dépasse pas ce nombre de lignes rendues
// (une timeline de plusieurs centaines de milliers d'évènements
// produirait sinon un fichier ingérable). Au-delà, la section est
// tronquée avec une mention explicite.
const MAX_LINES: usize = 60_000;
const MAX_CHARS: usize = 150; // largeur utile en caractères à 7.5pt

struct Pdf {
    doc: PdfDocumentReference,
    layer: PdfLayerReference,
    font: IndirectFontRef,
    bold: IndirectFontRef,
    y: f32,
    lines: usize,
    truncated: bool,
}

impl Pdf {
    fn new(title: &str) -> R<Self> {
        let (doc, page, layer) = PdfDocument::new(title, Mm(PW), Mm(PH), "Calque 1");
        let font = doc
            .add_builtin_font(BuiltinFont::Helvetica)
            .map_err(|e| e.to_string())?;
        let bold = doc
            .add_builtin_font(BuiltinFont::HelveticaBold)
            .map_err(|e| e.to_string())?;
        let layer = doc.get_page(page).get_layer(layer);
        Ok(Pdf {
            doc,
            layer,
            font,
            bold,
            y: MT,
            lines: 0,
            truncated: false,
        })
    }

    fn new_page(&mut self) {
        let (page, layer) = self.doc.add_page(Mm(PW), Mm(PH), "Calque");
        self.layer = self.doc.get_page(page).get_layer(layer);
        self.y = MT;
    }

    /// Hauteur de ligne approximative (mm) pour une taille de police (pt).
    fn line_height(size: f32) -> f32 {
        size * 0.40 + 1.3
    }

    fn write(&mut self, text: &str, size: f32, bold: bool, indent: f32) {
        if self.lines >= MAX_LINES {
            self.truncated = true;
            return;
        }
        let lh = Self::line_height(size);
        if self.y - lh < MB {
            self.new_page();
        }
        let t: String = text.chars().take(MAX_CHARS).collect();
        let f = if bold { &self.bold } else { &self.font };
        self.layer
            .use_text(t, size, Mm(ML + indent), Mm(self.y), f);
        self.y -= lh;
        self.lines += 1;
    }

    fn gap(&mut self, mm: f32) {
        self.y -= mm;
        if self.y < MB {
            self.new_page();
        }
    }

    fn save(self, path: &str) -> R<()> {
        let f = File::create(path).map_err(|e| e.to_string())?;
        self.doc
            .save(&mut BufWriter::new(f))
            .map_err(|e| e.to_string())
    }
}

fn row_line(columns: &[&str], r: &Row, tz: &str) -> String {
    columns
        .iter()
        .map(|c| fmt_cell(r.get(c), tz))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Rend une section tabulaire : titre + en-tête de colonnes + lignes.
/// Ne rend rien si la section est vide.
fn section(pdf: &mut Pdf, title: &str, columns: &[&str], rows: &[Row], tz: &str) {
    if rows.is_empty() {
        return;
    }
    pdf.gap(2.5);
    pdf.write(&format!("{} ({})", title, rows.len()), 12.0, true, 0.0);
    pdf.write(&columns.join(" | "), 7.5, true, 0.0);
    for r in rows {
        pdf.write(&row_line(columns, r, tz), 7.5, false, 0.0);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_pdf_report(
    path: &str,
    case: &CaseInfo,
    arts: &[Artifact],
    tz: &str,
    timeline: Option<&[Row]>,
    domains: Option<&[Row]>,
    tags: Option<&[Row]>,
    identity_pick: Option<&[Row]>,
    options: &ReportOptions,
) -> R<()> {
    let mut pdf = Pdf::new("Rapport NavForensics")?;

    // --- En-tête ---
    pdf.write(
        &format!(
            "Rapport d'analyse des navigateurs — NavForensics v{}",
            env!("CARGO_PKG_VERSION")
        ),
        16.0,
        true,
        0.0,
    );
    pdf.gap(2.0);
    if !case.numero.is_empty() {
        pdf.write(&format!("Dossier : {}", case.numero), 9.5, false, 0.0);
    }
    if !case.examinateur.is_empty() {
        pdf.write(&format!("Examinateur : {}", case.examinateur), 9.5, false, 0.0);
    }
    if !case.service.is_empty() {
        pdf.write(&format!("Service : {}", case.service), 9.5, false, 0.0);
    }
    if !case.notes.is_empty() {
        pdf.write(&format!("Observations : {}", case.notes), 9.5, false, 0.0);
    }

    // --- Sources & empreintes ---
    pdf.gap(2.5);
    pdf.write("Sources analysées & empreintes", 12.0, true, 0.0);
    pdf.write(
        "Scellé | Navigateur | Version | Installé | Base | SHA-256",
        7.5,
        true,
        0.0,
    );
    for a in arts {
        let (ver, inst) = browser_version_install(&a.path);
        let inst_txt = inst
            .map(|ms| fmt_cell(Some(&Cell::Time(ms)), tz))
            .unwrap_or_else(|| "—".into());
        pdf.write(
            &format!(
                "{} | {} | {} | {} | {} | {}",
                a.evidence,
                a.browser,
                ver.as_deref().unwrap_or("—"),
                inst_txt,
                a.kind,
                a.sha256
            ),
            7.0,
            false,
            0.0,
        );
    }

    if options.only_tagged {
        let tg = tags.unwrap_or(&[]);
        section(&mut pdf, "Éléments marqués", TAG_COLUMNS, tg, tz);
    } else {
        if options.include_stats {
            section(&mut pdf, "Statistiques", STATS_COLUMNS, &build_stats(arts), tz);
        }
        if options.include_tags {
            if let Some(tg) = tags {
                section(&mut pdf, "Éléments marqués", TAG_COLUMNS, tg, tz);
            }
        }
        if let Some(idp) = identity_pick {
            section(&mut pdf, "Éléments d'identité retenus", IDENTITY_COLUMNS, idp, tz);
        }
        if options.include_identity {
            section(
                &mut pdf,
                "Éléments d'identité (autofill)",
                IDENTITY_COLUMNS,
                &build_identity(arts),
                tz,
            );
        }
        if options.include_downloads {
            section(
                &mut pdf,
                "Téléchargements",
                DOWNLOADS_COLUMNS,
                &build_downloads(arts),
                tz,
            );
        }
        if options.include_geolocation {
            section(
                &mut pdf,
                "Géolocalisations",
                GEO_COLUMNS,
                &build_geolocation(arts),
                tz,
            );
        }
        if options.include_vpn {
            section(&mut pdf, "VPN", VPN_COLUMNS, &build_vpn(arts), tz);
        }
        if options.include_terms {
            section(
                &mut pdf,
                "Termes recherchés",
                SEARCH_TERMS_COLUMNS,
                &build_search_terms(arts),
                tz,
            );
        }
        if options.include_online {
            section(
                &mut pdf,
                "Services en ligne",
                ONLINE_COLUMNS,
                &build_online_services(arts),
                tz,
            );
        }
        if options.include_ga {
            section(
                &mut pdf,
                "Cookies Google Analytics",
                GA_COLUMNS,
                &build_ga_cookies(arts),
                tz,
            );
        }
        if options.include_flagged {
            section(
                &mut pdf,
                "Liens signalés",
                FLAGGED_COLUMNS,
                &build_flagged(arts, None),
                tz,
            );
        }
        if options.include_timeline {
            if let Some(t) = timeline {
                section(&mut pdf, "Chronologie", TIMELINE_COLUMNS, t, tz);
            }
        }
        if options.include_domains {
            if let Some(d) = domains {
                section(&mut pdf, "Domaines", DOMAIN_COLUMNS, d, tz);
            }
        }
    }

    if pdf.truncated {
        pdf.gap(3.0);
        pdf.write(
            &format!(
                "[Rapport tronqué à {} lignes — exporter en CSV/XLSX pour l'intégralité]",
                MAX_LINES
            ),
            8.0,
            true,
            0.0,
        );
    }

    pdf.save(path)
}
