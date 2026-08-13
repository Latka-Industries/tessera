//! Multi-column article body for THI-391 (`article_columns.tes`).

use crate::catalog::{TesWriterSession, TextHeader};
use crate::layout::DocKind;

use super::common::catalog;

const LOREM: &[&str] = &[
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor \
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis \
nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. \
Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore \
eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt \
in culpa qui officia deserunt mollit anim id est laborum.",
    "Curabitur pretium tincidunt lacus. Nulla gravida orci a odio. Nullam varius, \
turpis et commodo pharetra, est eros bibendum elit, nec luctus magna felis \
sollicitudin mauris. Integer in mauris eu nibh euismod gravida. Duis ac tellus \
et risus vulputate vehicula. Donec lobortis risus a elit. Etiam tempor. Ut \
ullamcorper, ligula eu tempor congue, eros est euismod turpis, id tincidunt \
sapien risus a quam. Maecenas fermentum consequat mi.",
    "Pellentesque habitant morbi tristique senectus et netus et malesuada fames ac \
turpis egestas. Vestibulum tortor quam, feugiat vitae, ultricies eget, tempor \
sit amet, ante. Donec eu libero sit amet quam egestas semper. Aenean ultricies \
mi vitae est. Mauris placerat eleifend leo. Quisque sit amet est et sapien \
ullamcorper pharetra. Vestibulum erat wisi, condimentum sed, commodo vitae, \
ornare sit amet, wisi. Aenean fermentum, elit eget tincidunt condimentum, eros \
ipsum rutrum orci, sagittis tempus lacus enim ac dui.",
    "Nam dui ligula, fringilla a, euismod sodales, sollicitudin vel, wisi. Morbi \
auctor lorem non justo. Nam lacus libero, pretium at, lobortis vitae, \
ultricies et, tellus. Donec aliquet, tortor sed accumsan bibendum, erat ligula \
aliquet magna, vitae ornare odio metus a mi. Morbi ac orci et nisl hendrerit \
mollis. Suspendisse ut massa. Cras nec ante. Pellentesque a nulla. Cum sociis \
natoque penatibus et magnis dis parturient montes, nascetur ridiculus mus.",
    "Proin tincidunt, velit id porta ornare, arcu lorem sollicitudin mi, quis \
porttitor magna nisl vel risus. Integer nonummy. Cras dapibus. Vivamus \
elementum semper nisi. Aenean vulputate eleifend tellus. Aenean leo ligula, \
porttitor eu, consequat vitae, eleifend ac, enim. Aliquam lorem ante, dapibus \
in, viverra quis, feugiat a, tellus. Phasellus viverra nulla ut metus varius \
laoreet. Quisque rutrum. Aenean imperdiet.",
    "Etiam ultricies nisi vel augue. Curabitur ullamcorper ultricies nisi. Nam eget \
dui. Etiam rhoncus. Maecenas tempus, tellus eget condimentum rhoncus, sem quam \
semper libero, sit amet adipiscing sem neque sed ipsum. Nam quam nunc, blandit \
vel, luctus pulvinar, hendrerit id, lorem. Maecenas nec odio et ante tincidunt \
tempus. Donec vitae sapien ut libero venenatis faucibus. Nullam quis ante.",
];

fn add_paragraphs(session: &mut TesWriterSession, texts: &[&str]) {
    for text in texts {
        session
            .add_text_chunk(&TextHeader::paragraph(), text)
            .expect("paragraph");
    }
}

fn columns_region(session: &mut TesWriterSession, count: u8, gap: u16, body: &[&str]) {
    session
        .add_text_chunk(&TextHeader::columns_with(count, Some(gap)), "")
        .expect("columns open");
    add_paragraphs(session, body);
    session
        .add_text_chunk(&TextHeader::columns_end(), "")
        .expect("columns end");
}

/// Dense 2- then 3-column article so the newspaper band is obvious on PDF.
///
/// # Panics
///
/// Panics if catalog setup or encoding fails.
#[must_use]
pub fn encode_article_columns() -> Vec<u8> {
    let mut session = TesWriterSession::create("article_columns.tes", DocKind::Document);
    let mut cat = catalog(
        "cc0e8400-e29b-41d4-a716-446655440301",
        "Harbor column smoke",
        "2026-08-12T00:00:00Z",
        "2026-08-12T00:00:00Z",
        DocKind::Document,
        &["sample", "columns", "print"],
    );
    cat.language = Some("en".into());
    session.set_catalog(cat).expect("catalog");

    session
        .add_text_chunk(&TextHeader::heading(1), "Harbor column smoke")
        .expect("h1");
    add_paragraphs(
        &mut session,
        &[
            "Lead stays full measure. Below: a two-column region, a spanning mid heading, \
             then a three-column region — enough lorem to fill bands so the split is obvious. \
             Pair with pack `columns_justify` for justified column text (vs `columns_left`).",
        ],
    );

    // 2-col: four lorems, spanning H2, then two more.
    session
        .add_text_chunk(&TextHeader::columns_with(2, Some(16)), "")
        .expect("columns-2 open");
    add_paragraphs(&mut session, &LOREM[..4]);
    session
        .add_text_chunk(&TextHeader::heading(2), "Mid heading spans full width")
        .expect("mid h2");
    add_paragraphs(&mut session, &LOREM[4..]);
    session
        .add_text_chunk(&TextHeader::columns_end(), "")
        .expect("columns-2 end");

    session
        .add_text_chunk(&TextHeader::heading(2), "Three-column band")
        .expect("h2 three");
    add_paragraphs(
        &mut session,
        &["Full-measure bridge before the three-column region opens."],
    );
    columns_region(&mut session, 3, 12, LOREM);

    add_paragraphs(
        &mut session,
        &["Closing full-measure paragraph after the last column region."],
    );

    session.encode_file().expect("article_columns")
}
