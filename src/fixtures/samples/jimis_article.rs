//! THI-397 jimis-article reseal (`jimis_article.tes`).
//!
//! Witness: `tmp/latex-goldens/jimis-article/main.pdf`. Not a class clone.

use crate::catalog::chunk::{NOTE_MARKER, NoteKind};
use crate::catalog::{InlineKind, InlineSpan, TableData, TableRow, TesWriterSession, TextHeader};
use crate::layout::DocKind;

use super::common::{catalog, cell};

/// French two-column article reseal of the jimis-article witness (THI-397).
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_jimis_article() -> Vec<u8> {
    let mut session = TesWriterSession::create("jimis_article.tes", DocKind::Research);
    let mut cat = catalog(
        "dd0e8400-e29b-41d4-a716-446655440397",
        "Méthodes reproductibles pour l'analyse interdisciplinaire",
        "2026-08-31T00:00:00Z",
        "2026-08-31T00:00:00Z",
        DocKind::Research,
        &["sample", "article", "jimis", "dogfood"],
    );
    cat.language = Some("fr".into());
    cat.template_id = Some("article".into());
    cat.cite_style_id = Some("numeric".into());
    session.set_catalog(cat).expect("catalog");

    session
        .add_text_chunk(
            &TextHeader::heading(1),
            "Méthodes reproductibles pour l'analyse interdisciplinaire",
        )
        .expect("h1");
    session
        .add_text_chunk(&TextHeader::paragraph(), "JIMIS — volume à compléter")
        .expect("volume line");

    add_author_camille(&mut session);
    session
        .add_text_chunk(
            &TextHeader::callout("author", Some("Léa Bernard".into())),
            "",
        )
        .expect("author lea");

    // Full-width maketitle analogue, then 2-col including abstract (jimis
    // `twocolumn` after `\maketitle`).
    session
        .add_text_chunk(&TextHeader::columns_with(2, Some(18)), "")
        .expect("columns open");

    session
        .add_text_chunk(
            &TextHeader::callout("abstract", Some("Résumé".into())),
            "Les recherches interdisciplinaires combinent souvent des données hétérogènes, \
             des méthodes complémentaires et des décisions qui doivent rester lisibles pour \
             plusieurs communautés. Cet article présente un petit protocole de travail qui \
             relie une question, un jeu de données, une analyse et une décision de partage. \
             Un exemple synthétique montre comment documenter les transformations sans \
             alourdir le texte principal.",
        )
        .expect("abstract");
    session
        .add_text_chunk(
            &TextHeader::callout("keywords", Some("Mots-clés".into())),
            "interdisciplinarité ; reproductibilité ; méthodes ; données ouvertes",
        )
        .expect("keywords");

    session
        .add_text_chunk(&TextHeader::heading(2), "Introduction")
        .expect("h2 intro");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Une méthode devient plus facile à discuter lorsqu'elle distingue clairement \
             les observations, les hypothèses et les choix de mise en forme. Cette séparation \
             permet à des lecteurs de disciplines différentes de vérifier les mêmes étapes \
             et de proposer des améliorations localisées.",
        )
        .expect("intro");

    session
        .add_text_chunk(&TextHeader::heading(2), "Protocole")
        .expect("h2 proto");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Nous décrivons chaque analyse par une chaîne de transformations. Si $x_i$ est \
             une observation et $m_i$ son indicateur de qualité, la moyenne pondérée est",
        )
        .expect("proto lead");
    session
        .add_text_chunk(
            &TextHeader::math(),
            r"\bar{x}_w = \frac{\sum_i m_i x_i}{\sum_i m_i}",
        )
        .expect("weighted mean");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Les décisions sont conservées avec la date, la personne responsable et une \
             référence vers le fichier produit.",
        )
        .expect("proto trail");
    session
        .add_text_chunk(
            &TextHeader::callout("definition", Some("Trace minimale".into())),
            "Une trace minimale est l'ensemble des informations qui permet de reconstruire \
             une transformation : entrée, opération, paramètres, sortie et commentaire.",
        )
        .expect("definition");

    session
        .add_text_chunk(&TextHeader::heading(2), "Résultats")
        .expect("h2 results");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Le tableau suivant illustre une synthèse de trois transformations.",
        )
        .expect("results lead");
    add_trace_table(&mut session);

    session
        .add_text_chunk(&TextHeader::heading(2), "Discussion")
        .expect("h2 disc");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Ce format rend les choix visibles sans prétendre qu'une seule méthode convient \
             à toutes les questions. Une version longue peut ajouter des annexes, des \
             figures et les détails du protocole ; la version de soumission doit suivre les \
             consignes éditoriales de JIMIS pour les dates, le DOI et la pagination.",
        )
        .expect("discussion");

    session
        .add_text_chunk(&TextHeader::heading(2), "Conclusion")
        .expect("h2 conc");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Un registre court et partagé suffit souvent à rendre une analyse plus vérifiable. \
             La structure de ce document peut être étendue avec les sections propres au \
             projet et les références définitives.",
        )
        .expect("conclusion");

    add_bibliography(&mut session);

    session
        .add_text_chunk(&TextHeader::columns_end(), "")
        .expect("columns end");

    session.encode_file().expect("jimis_article")
}

fn add_author_camille(session: &mut TesWriterSession) {
    let end = u32::try_from(NOTE_MARKER.len()).expect("marker");
    let mut header = TextHeader::callout("author", Some("Camille Martin".into()));
    header.spans.push(InlineSpan {
        start: 0,
        end,
        kind: InlineKind::Note {
            kind: NoteKind::Footnote,
            body: "camille.martin@example.org".into(),
        },
    });
    session
        .add_text_chunk(&header, NOTE_MARKER)
        .expect("author camille");
}

fn add_trace_table(session: &mut TesWriterSession) {
    let mut table = TextHeader::table(TableData {
        rows: vec![
            TableRow {
                cells: vec![
                    cell("Étape", true),
                    cell("Entrée", true),
                    cell("Sortie", true),
                ],
            },
            TableRow {
                cells: vec![
                    cell("Nettoyage", false),
                    cell("128 lignes", false),
                    cell("121 lignes", false),
                ],
            },
            TableRow {
                cells: vec![
                    cell("Codage", false),
                    cell("121 lignes", false),
                    cell("4 thèmes", false),
                ],
            },
            TableRow {
                cells: vec![
                    cell("Contrôle", false),
                    cell("4 thèmes", false),
                    cell("1 tableau", false),
                ],
            },
        ],
    });
    table.caption = Some("Exemple de registre de transformations.".into());
    session.add_text_chunk(&table, "").expect("trace table");
}

fn add_bibliography(session: &mut TesWriterSession) {
    // jimis has `thebibliography` with no in-text `\cite`. Cite chunks would
    // also dump an English "References" heading after `\endcolumns` (gap note).
    session
        .add_text_chunk(&TextHeader::heading(2), "Références")
        .expect("h2 refs");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "[1] J. W. Creswell and V. L. P. Clark, Designing and Conducting Mixed Methods \
             Research, 3rd ed., Sage, 2018.",
        )
        .expect("creswell");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "[2] M. D. Wilkinson et al., “The FAIR Guiding Principles for scientific data \
             management and stewardship,” Scientific Data, 3, 2016.",
        )
        .expect("wilkinson");
}
