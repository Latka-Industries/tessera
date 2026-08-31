use super::*;

#[test]
fn ordered_list_numbering_restarts_per_depth_and_run() {
    let mut n = OrderedListNumbering::default();
    let o1 = TextHeader::list_item(ListKind::Ordered);
    let o2 = TextHeader::list_item_at(ListKind::Ordered, 2);
    let bullet = TextHeader::list_item(ListKind::Bullet);
    let para = TextHeader::paragraph();

    assert_eq!(n.take_for_text(&o1), Some(1));
    assert_eq!(n.take_for_text(&o1), Some(2));
    assert_eq!(n.take_for_text(&o2), Some(1));
    assert_eq!(n.take_for_text(&o2), Some(2));
    assert_eq!(n.take_for_text(&o1), Some(3));
    assert_eq!(n.take_for_text(&bullet), None);
    assert_eq!(n.take_for_text(&o1), Some(1));
    assert_eq!(n.take_for_text(&para), None);
    assert_eq!(n.take_for_text(&o1), Some(1));
    assert_eq!(
        o1.render_markdown_with_links_indexed("alpha", &[], Some(2)),
        "2. alpha"
    );
}

#[test]
fn list_depth_round_trip_and_markdown_indent() {
    let body = "nested item";
    let header = TextHeader::list_item_at(ListKind::Bullet, 2);
    assert_eq!(header.list_depth, Some(2));
    assert!(header.uses_layout_v1_features());
    let bytes = encode_text_payload(&header, body).unwrap();
    let (h2, b2) = decode_text_payload(&bytes).unwrap();
    assert_eq!(h2, header);
    assert_eq!(b2, body);
    assert_eq!(header.render_markdown(body), "  - nested item");
}

#[test]
fn indent_level_round_trip() {
    let mut header = TextHeader::paragraph();
    header.indent = Some(2);
    assert!(header.uses_layout_v1_features());
    assert_eq!(header.indent_or_default(), 2);
    let bytes = encode_text_payload(&header, "skills").unwrap();
    let (h2, _) = decode_text_payload(&bytes).unwrap();
    assert_eq!(h2.indent, Some(2));
}

#[test]
fn underline_span_round_trip_and_markdown() {
    let body = "see noted term here";
    let mut header = TextHeader::paragraph();
    header.spans = vec![InlineSpan {
        start: 4,
        end: 9,
        kind: InlineKind::Underline,
    }];
    let bytes = encode_text_payload(&header, body).unwrap();
    let (h2, b2) = decode_text_payload(&bytes).unwrap();
    assert_eq!(h2, header);
    assert_eq!(b2, body);
    let md = header.render_markdown(body);
    assert!(md.contains("<u>noted</u>"), "{md}");
}

#[test]
fn font_and_link_coextensive_spans_keep_backslash_font() {
    use crate::catalog::link::{LinkKind, OutboundLink};

    let glyph = "\u{f08c}";
    let body = glyph.to_owned();
    let end = u32::try_from(body.len()).unwrap();
    let mut header = TextHeader::paragraph();
    header.spans = vec![
        InlineSpan {
            start: 0,
            end,
            kind: InlineKind::Font {
                font_id: "fab".into(),
            },
        },
        InlineSpan {
            start: 0,
            end,
            kind: InlineKind::Link { link_id: 0 },
        },
    ];
    let entry = OutboundLink {
        start: 0,
        end,
        dest: "https://example.com/".into(),
    }
    .into_entry(0, LinkKind::Wiki)
    .unwrap();
    let md = header.render_markdown_with_links(&body, &[entry]);
    assert!(
        md.contains("\\icon{linkedin}") || md.contains("\\font{fab}{"),
        "expected \\icon or \\font macro, got {md:?}"
    );
    assert!(
        !md.contains('\u{000c}'),
        "must not interpret \\f as form-feed: {md:?}"
    );
    assert!(
        md.starts_with("[\\icon{")
            || md.starts_with("[\\font{fab}{")
            || md.contains("](https://example.com/)"),
        "expected linked font glyph, got {md:?}"
    );
}

#[test]
fn nested_font_inside_link_label_round_trips() {
    use crate::catalog::link::{LinkKind, OutboundLink};

    // "Nefaxer " + github glyph — Font is a sub-range of Link.
    let glyph = "\u{f09b}";
    let body = format!("Nefaxer {glyph}");
    let glyph_start = body.find(glyph).unwrap() as u32;
    let end = body.len() as u32;
    let mut header = TextHeader::paragraph();
    header.spans = vec![
        InlineSpan {
            start: glyph_start,
            end,
            kind: InlineKind::Font {
                font_id: "fab".into(),
            },
        },
        InlineSpan {
            start: 0,
            end,
            kind: InlineKind::Link { link_id: 0 },
        },
    ];
    let entry = OutboundLink {
        start: 0,
        end,
        dest: "https://github.com/thicclatka/nefaxer".into(),
    }
    .into_entry(0, LinkKind::Wiki)
    .unwrap();
    let md = header.render_markdown_with_links(&body, &[entry]);
    assert!(
        md.contains("[Nefaxer \\icon{github}](https://github.com/thicclatka/nefaxer)"),
        "nested icon in link must not split \\icon: {md:?}"
    );
}

#[test]
fn text_payload_round_trip() {
    let header = TextHeader::paragraph();
    let body = "We measured …";
    let bytes = encode_text_payload(&header, body).unwrap();
    let (h2, b2) = decode_text_payload(&bytes).unwrap();
    assert_eq!(h2, header);
    assert_eq!(b2, body);
}

#[test]
fn heading_payload_round_trip() {
    let header = TextHeader::heading(2);
    let bytes = encode_text_payload(&header, "Methods").unwrap();
    let (h2, b2) = decode_text_payload(&bytes).unwrap();
    assert_eq!(h2.role, TextRole::Heading);
    assert_eq!(h2.level, Some(2));
    assert_eq!(b2, "Methods");
}

#[test]
fn spans_math_table_round_trip() {
    let body = "alpha beta";
    let mut header = TextHeader::paragraph();
    header.lang = Some("en".into());
    header.align = Some(TextAlign::Start);
    header.spans = vec![InlineSpan {
        start: 0,
        end: 5,
        kind: InlineKind::Emphasis,
    }];
    let bytes = encode_text_payload(&header, body).unwrap();
    let (h2, b2) = decode_text_payload(&bytes).unwrap();
    assert_eq!(h2, header);
    assert_eq!(b2, body);

    let math = TextHeader::math();
    let mbytes = encode_text_payload(&math, "E = mc^2").unwrap();
    let (mh, mb) = decode_text_payload(&mbytes).unwrap();
    assert_eq!(mh.role, TextRole::Math);
    assert_eq!(mb, "E = mc^2");

    let table = TextHeader::table(TableData {
        rows: vec![
            TableRow {
                cells: vec![
                    TableCell {
                        text: "A".into(),
                        spans: Vec::new(),
                        align: None,
                        is_header: true,
                        rowspan: None,
                        colspan: None,
                    },
                    TableCell {
                        text: "B".into(),
                        spans: Vec::new(),
                        align: Some(TextAlign::Center),
                        is_header: true,
                        rowspan: None,
                        colspan: None,
                    },
                ],
            },
            TableRow {
                cells: vec![
                    TableCell {
                        text: "1".into(),
                        spans: Vec::new(),
                        align: None,
                        is_header: false,
                        rowspan: None,
                        colspan: None,
                    },
                    TableCell {
                        text: "2".into(),
                        spans: Vec::new(),
                        align: None,
                        is_header: false,
                        rowspan: None,
                        colspan: None,
                    },
                ],
            },
        ],
    });
    let tbytes = encode_text_payload(&table, "").unwrap();
    let (th, tb) = decode_text_payload(&tbytes).unwrap();
    assert_eq!(th.table.as_ref().unwrap().rows.len(), 2);
    assert!(tb.is_empty());
}

#[test]
fn rejects_out_of_bounds_span() {
    let mut header = TextHeader::paragraph();
    header.spans = vec![InlineSpan {
        start: 0,
        end: 99,
        kind: InlineKind::Strong,
    }];
    assert!(encode_text_payload(&header, "hi").is_err());
}

#[test]
fn caption_round_trip_on_math() {
    let mut header = TextHeader::math();
    header.caption = Some("Einstein".into());
    let bytes = encode_text_payload(&header, "E = mc^2").unwrap();
    let (h, body) = decode_text_payload(&bytes).unwrap();
    assert_eq!(h.caption.as_deref(), Some("Einstein"));
    assert_eq!(body, "E = mc^2");
}

#[test]
fn caption_rejected_on_paragraph() {
    let mut header = TextHeader::paragraph();
    header.caption = Some("nope".into());
    assert!(encode_text_payload(&header, "hi").is_err());
}

#[test]
fn callout_header_round_trip_and_band_title() {
    let header = TextHeader::callout("definition", Some("Trace minimale".into()));
    assert!(header.uses_layout_v1_features());
    assert_eq!(header.callout_band_title(), "Definition (Trace minimale).");
    let bytes = encode_text_payload(&header, "Une trace minimale est une trace.").unwrap();
    let (h2, body) = decode_text_payload(&bytes).unwrap();
    assert_eq!(h2, header);
    assert_eq!(body, "Une trace minimale est une trace.");
    let proof = TextHeader::callout("proof", None);
    assert_eq!(proof.callout_band_title(), "Proof.");
    let note = TextHeader::callout("note", Some("Note".into()));
    assert_eq!(note.callout_band_title(), "Note");
}

#[test]
fn callout_kind_rejected_on_paragraph() {
    let mut header = TextHeader::paragraph();
    header.callout_kind = Some("note".into());
    assert!(encode_text_payload(&header, "hi").is_err());
}

#[test]
fn cite_payload_round_trip() {
    let cite = CitePayload {
        quote: "We measured …".into(),
        target_doc_id: Some("660e8400-e29b-41d4-a716-446655440001".into()),
        target_chunk_id: Some(12),
        target_byte_start: Some(0),
        target_byte_end: Some(42),
        label: Some("Smith2024".into()),
        page: Some(7),
        source: None,
    };
    let decoded = CitePayload::from_bytes(&cite.to_bytes().unwrap()).unwrap();
    assert_eq!(decoded, cite);
}

#[test]
fn cite_rejects_inverted_byte_range() {
    let cite = CitePayload {
        quote: String::new(),
        target_doc_id: None,
        target_chunk_id: None,
        target_byte_start: Some(10),
        target_byte_end: Some(10),
        label: None,
        page: None,
        source: None,
    };
    assert!(cite.validate().is_err());
}
