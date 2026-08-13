//! Dense long-word prose for hyphenation smoke (`hyphen_dense.tes`, THI-394).

use crate::catalog::{TesWriterSession, TextHeader};
use crate::layout::DocKind;

use super::common::catalog;

const LONG: &str = "The internationalization and decentralization of telecommunications \
infrastructure requires thoughtful hyphenation: supercalifragilisticexpialidocious \
vocabulary should break with a hyphen rather than overflowing the measure or \
hard-splitting mid-grapheme. Repeated electroencephalographically challenging \
words force several soft wraps across a deliberately narrow band.";

const WIDOW_PAD: &str = "Widows and orphans: short lead-in. Then enough lines that pagination \
glue matters when the paragraph straddles a page break. Lorem-ish \
filler continues with more internationalization talk so the engine \
has several content lines to keep together at the break. Extra \
electroencephalographically dense wording pads the paragraph.";

fn indented_para(session: &mut TesWriterSession, body: &str) {
    let mut header = TextHeader::paragraph();
    header.indent = Some(5);
    session.add_text_chunk(&header, body).expect("para");
}

/// Dense indented prose for THI-394 hyphen on/off smoke.
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_hyphen_dense() -> Vec<u8> {
    let mut session = TesWriterSession::create("hyphen_dense.tes", DocKind::Note);
    let mut cat = catalog(
        "aa0e8400-e29b-41d4-a716-446655440394",
        "Hyphenation dense prose",
        "2026-08-12T21:00:00Z",
        "2026-08-12T21:00:00Z",
        DocKind::Note,
        &["sample", "hyphen", "browse"],
    );
    cat.language = Some("en".into());
    session.set_catalog(cat).expect("catalog");
    session
        .add_text_chunk(&TextHeader::heading(1), "Hyphenation dense prose")
        .expect("h1");
    for _ in 0..5 {
        indented_para(&mut session, LONG);
    }
    indented_para(&mut session, WIDOW_PAD);
    for _ in 0..3 {
        indented_para(&mut session, LONG);
    }
    session.encode_file().expect("hyphen_dense")
}
