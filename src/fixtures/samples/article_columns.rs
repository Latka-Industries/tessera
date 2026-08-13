//! Multi-column article body for THI-391 (`article_columns.tes`).

use crate::catalog::{TesWriterSession, TextHeader};
use crate::layout::DocKind;

use super::common::catalog;

const LOREM_A: &str = "\
Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor \
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis \
nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. \
Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore \
eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt \
in culpa qui officia deserunt mollit anim id est laborum.";

const LOREM_B: &str = "\
Curabitur pretium tincidunt lacus. Nulla gravida orci a odio. Nullam varius, \
turpis et commodo pharetra, est eros bibendum elit, nec luctus magna felis \
sollicitudin mauris. Integer in mauris eu nibh euismod gravida. Duis ac tellus \
et risus vulputate vehicula. Donec lobortis risus a elit. Etiam tempor. Ut \
ullamcorper, ligula eu tempor congue, eros est euismod turpis, id tincidunt \
sapien risus a quam. Maecenas fermentum consequat mi.";

const LOREM_C: &str = "\
Pellentesque habitant morbi tristique senectus et netus et malesuada fames ac \
turpis egestas. Vestibulum tortor quam, feugiat vitae, ultricies eget, tempor \
sit amet, ante. Donec eu libero sit amet quam egestas semper. Aenean ultricies \
mi vitae est. Mauris placerat eleifend leo. Quisque sit amet est et sapien \
ullamcorper pharetra. Vestibulum erat wisi, condimentum sed, commodo vitae, \
ornare sit amet, wisi. Aenean fermentum, elit eget tincidunt condimentum, eros \
ipsum rutrum orci, sagittis tempus lacus enim ac dui.";

const LOREM_D: &str = "\
Nam dui ligula, fringilla a, euismod sodales, sollicitudin vel, wisi. Morbi \
auctor lorem non justo. Nam lacus libero, pretium at, lobortis vitae, \
ultricies et, tellus. Donec aliquet, tortor sed accumsan bibendum, erat ligula \
aliquet magna, vitae ornare odio metus a mi. Morbi ac orci et nisl hendrerit \
mollis. Suspendisse ut massa. Cras nec ante. Pellentesque a nulla. Cum sociis \
natoque penatibus et magnis dis parturient montes, nascetur ridiculus mus.";

const LOREM_E: &str = "\
Proin tincidunt, velit id porta ornare, arcu lorem sollicitudin mi, quis \
porttitor magna nisl vel risus. Integer nonummy. Cras dapibus. Vivamus \
elementum semper nisi. Aenean vulputate eleifend tellus. Aenean leo ligula, \
porttitor eu, consequat vitae, eleifend ac, enim. Aliquam lorem ante, dapibus \
in, viverra quis, feugiat a, tellus. Phasellus viverra nulla ut metus varius \
laoreet. Quisque rutrum. Aenean imperdiet.";

const LOREM_F: &str = "\
Etiam ultricies nisi vel augue. Curabitur ullamcorper ultricies nisi. Nam eget \
dui. Etiam rhoncus. Maecenas tempus, tellus eget condimentum rhoncus, sem quam \
semper libero, sit amet adipiscing sem neque sed ipsum. Nam quam nunc, blandit \
vel, luctus pulvinar, hendrerit id, lorem. Maecenas nec odio et ante tincidunt \
tempus. Donec vitae sapien ut libero venenatis faucibus. Nullam quis ante.";

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
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Lead stays full measure. Below: a two-column region, a spanning mid heading, \
             then a three-column region — enough lorem to fill bands so the split is obvious.",
        )
        .expect("lead");

    // --- 2 columns ---
    session
        .add_text_chunk(&TextHeader::columns_with(2, Some(16)), "")
        .expect("columns-2 open");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_A)
        .expect("2col a");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_B)
        .expect("2col b");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_C)
        .expect("2col c");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_D)
        .expect("2col d");
    session
        .add_text_chunk(&TextHeader::heading(2), "Mid heading spans full width")
        .expect("mid h2");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_E)
        .expect("2col after span");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_F)
        .expect("2col after span 2");
    session
        .add_text_chunk(&TextHeader::columns_end(), "")
        .expect("columns-2 end");

    session
        .add_text_chunk(
            &TextHeader::heading(2),
            "Three-column band",
        )
        .expect("h2 three");
    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Full-measure bridge before the three-column region opens.",
        )
        .expect("bridge");

    // --- 3 columns ---
    session
        .add_text_chunk(&TextHeader::columns_with(3, Some(12)), "")
        .expect("columns-3 open");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_A)
        .expect("3col a");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_B)
        .expect("3col b");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_C)
        .expect("3col c");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_D)
        .expect("3col d");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_E)
        .expect("3col e");
    session
        .add_text_chunk(&TextHeader::paragraph(), LOREM_F)
        .expect("3col f");
    session
        .add_text_chunk(&TextHeader::columns_end(), "")
        .expect("columns-3 end");

    session
        .add_text_chunk(
            &TextHeader::paragraph(),
            "Closing full-measure paragraph after the last column region.",
        )
        .expect("closing");

    session.encode_file().expect("article_columns")
}
