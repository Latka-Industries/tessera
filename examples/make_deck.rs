//! Write a two-slide demo deck for HTML/PDF smoke tests.
//!
//! ```bash
//! cargo run --example make_deck -- /tmp/deck.tes
//! cargo run -- export /tmp/deck.tes --html --standalone -o /tmp/deck.html \
//!   --theme templates/minimal/themes/draft.css --embed-css
//! cargo run -- export /tmp/deck.tes --pdf -o /tmp/deck.pdf --template-root templates
//! ```

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use tessera_doc::catalog::{DocumentCatalog, SlidePayload, TesWriterSession, TextHeader};
use tessera_doc::layout::DocKind;

fn main() -> ExitCode {
    let out = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("deck.tes"));
    if out.exists() {
        eprintln!("error: {} already exists", out.display());
        return ExitCode::from(1);
    }

    let mut session = TesWriterSession::create(&out, DocKind::Deck);
    if let Err(err) = session.set_catalog(DocumentCatalog::new(
        "990e8400-e29b-41d4-a716-446655440099",
        "Tessera deck smoke",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Deck,
    )) {
        eprintln!("error: {err}");
        return ExitCode::from(1);
    }

    // Chunks 1–2 → slide A; 3–4 → slide B.
    let steps = [
        (TextHeader::heading(1), "Welcome"),
        (
            TextHeader::paragraph(),
            "First slide body — region-based layout.",
        ),
        (TextHeader::heading(1), "Next up"),
        (
            TextHeader::paragraph(),
            "Second slide body — same container as notes.",
        ),
    ];
    for (header, body) in &steps {
        if let Err(err) = session.add_text_chunk(header, body) {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    }

    for slide in [
        SlidePayload::title_body(1, 2),
        SlidePayload::title_body(3, 4),
    ] {
        if let Err(err) = session.add_slide(&slide) {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    }

    if let Err(err) = session.commit() {
        eprintln!("error: {err}");
        return ExitCode::from(1);
    }
    println!("wrote {}", out.display());
    ExitCode::SUCCESS
}
