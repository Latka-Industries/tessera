use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::catalog::history::split_body_and_history;
use crate::catalog::{DocumentCatalog, TesFile, TesWriterSession, TextHeader};
use crate::edit::{TesOp, apply_ops, file_source_hash};
use crate::error::TesError;
use crate::layout::{DocKind, SuperblockV0};
use crate::verify::verify_tes_file;
use tempfile::tempdir;

fn sample(dir: &Path) -> PathBuf {
    let path = dir.join("note.tes");
    let mut s = TesWriterSession::create(&path, DocKind::Note);
    s.set_catalog(DocumentCatalog::new(
        "550e8400-e29b-41d4-a716-446655440000",
        "History note",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Note,
    ))
    .unwrap();
    s.add_text_chunk(&TextHeader::paragraph(), "First body")
        .unwrap();
    s.commit().unwrap();
    path
}

#[test]
fn save_log_diff_round_trip() {
    let dir = tempdir().unwrap();
    let path = sample(dir.path());

    let r1 = save_revision(
        &path,
        &SaveOptions {
            draft: Some("outline".into()),
            message: Some("initial".into()),
            ..SaveOptions::default()
        },
    )
    .unwrap();
    assert!(verify_tes_file(&path, true).unwrap().ok);
    let log = format_log(&path, false).unwrap();
    assert!(log.contains(&r1.revision_id));
    assert!(log.contains("outline"));

    let hash = file_source_hash(&path).unwrap();
    apply_ops(
        &path,
        &[TesOp::SetText {
            chunk_id: 1,
            body: "Second body".into(),
            role: None,
            level: None,
            class: None,
        }],
        &crate::edit::EditWriteOptions::new(hash, false),
    )
    .unwrap();

    let r2 = save_revision(
        &path,
        &SaveOptions {
            draft: Some("outline".into()),
            message: Some("edit".into()),
            ..SaveOptions::default()
        },
    )
    .unwrap();
    assert_ne!(r1.revision_id, r2.revision_id);

    let diff = diff_revisions(&path, &r1.revision_id, &r2.revision_id).unwrap();
    assert!(!diff.entries.is_empty());
    let text = format_diff(&diff);
    assert!(text.contains('~') || text.contains("Second body") || text.contains('-'));
    let changelog = format_changelog(&path, "outline", &r1.revision_id).unwrap();
    assert!(changelog.contains("changelog"));

    // Identical content save must not mint another revision.
    let before = read_history(&path).unwrap().revisions.len();
    let again = save_revision(
        &path,
        &SaveOptions {
            draft: Some("outline".into()),
            message: Some("noop".into()),
            ..SaveOptions::default()
        },
    )
    .unwrap();
    assert_eq!(again.revision_id, r2.revision_id);
    assert_eq!(read_history(&path).unwrap().revisions.len(), before);
}

#[test]
fn export_checkout_textconv_round_trip() {
    let dir = tempdir().unwrap();
    let path = sample(dir.path());

    let r1 = save_revision(
        &path,
        &SaveOptions {
            draft: Some("outline".into()),
            message: Some("initial".into()),
            ..SaveOptions::default()
        },
    )
    .unwrap();

    let bytes_after_r1 = fs::read(&path).unwrap();
    let sb = SuperblockV0::from_bytes(&bytes_after_r1).unwrap();
    let (body_r1, _) = split_body_and_history(&bytes_after_r1, sb.has_history_footer()).unwrap();

    let hash = file_source_hash(&path).unwrap();
    apply_ops(
        &path,
        &[TesOp::SetText {
            chunk_id: 1,
            body: "Second body".into(),
            role: None,
            level: None,
            class: None,
        }],
        &crate::edit::EditWriteOptions::new(hash, false),
    )
    .unwrap();
    let r2 = save_revision(
        &path,
        &SaveOptions {
            draft: Some("outline".into()),
            message: Some("edit".into()),
            ..SaveOptions::default()
        },
    )
    .unwrap();
    assert_ne!(r1.revision_id, r2.revision_id);

    let exported = dir.path().join("old.tes");
    export_revision(&path, &r1.revision_id, &exported, false).unwrap();
    assert_eq!(fs::read(&exported).unwrap(), body_r1);
    assert!(verify_tes_file(&exported, true).unwrap().ok);

    let hist_before = read_history(&path).unwrap();
    let head_before = hist_before.head.clone();
    let drafts_before = hist_before.drafts.clone();
    checkout_revision(&path, &r1.revision_id).unwrap();
    assert!(verify_tes_file(&path, true).unwrap().ok);
    let hist_after = read_history(&path).unwrap();
    assert_eq!(hist_after.head, head_before);
    assert_eq!(hist_after.drafts, drafts_before);
    assert_eq!(hist_after.revisions.len(), hist_before.revisions.len());

    let bytes = fs::read(&path).unwrap();
    let sb = SuperblockV0::from_bytes(&bytes).unwrap();
    let (body_now, _) = split_body_and_history(&bytes, sb.has_history_footer()).unwrap();
    assert_eq!(body_now, body_r1);

    let tessprek = textconv(&path).unwrap();
    assert!(!tessprek.trim().is_empty());
    assert!(tessprek.contains("First body") || tessprek.contains("History note"));
}

#[test]
fn blame_attributes_separate_paragraph_edits() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("two.tes");
    let mut s = TesWriterSession::create(&path, DocKind::Note);
    s.set_catalog(DocumentCatalog::new(
        "550e8400-e29b-41d4-a716-446655440099",
        "Blame note",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Note,
    ))
    .unwrap();
    s.add_text_chunk(&TextHeader::paragraph(), "Alpha paragraph")
        .unwrap();
    s.add_text_chunk(&TextHeader::paragraph(), "Beta paragraph")
        .unwrap();
    s.commit().unwrap();

    let r1 = save_revision(
        &path,
        &SaveOptions {
            message: Some("initial".into()),
            ..SaveOptions::default()
        },
    )
    .unwrap();

    let hash = file_source_hash(&path).unwrap();
    apply_ops(
        &path,
        &[TesOp::SetText {
            chunk_id: 1,
            body: "Alpha edited".into(),
            role: None,
            level: None,
            class: None,
        }],
        &crate::edit::EditWriteOptions::new(hash, false),
    )
    .unwrap();
    let r2 = save_revision(
        &path,
        &SaveOptions {
            message: Some("edit alpha".into()),
            ..SaveOptions::default()
        },
    )
    .unwrap();

    let hash = file_source_hash(&path).unwrap();
    apply_ops(
        &path,
        &[TesOp::SetText {
            chunk_id: 2,
            body: "Beta edited".into(),
            role: None,
            level: None,
            class: None,
        }],
        &crate::edit::EditWriteOptions::new(hash, false),
    )
    .unwrap();
    let r3 = save_revision(
        &path,
        &SaveOptions {
            message: Some("edit beta".into()),
            ..SaveOptions::default()
        },
    )
    .unwrap();
    assert_ne!(r1.revision_id, r2.revision_id);
    assert_ne!(r2.revision_id, r3.revision_id);

    let report = blame_file(&path, &BlameOptions::default()).unwrap();
    assert_eq!(report.revision_id, r3.revision_id);
    let alpha = report
        .regions
        .iter()
        .find(|r| r.chunk_id == 1)
        .expect("alpha");
    let beta = report
        .regions
        .iter()
        .find(|r| r.chunk_id == 2)
        .expect("beta");
    assert_eq!(alpha.revision_id, r2.revision_id);
    assert_eq!(alpha.text, "Alpha edited");
    assert_eq!(beta.revision_id, r3.revision_id);
    assert_eq!(beta.text, "Beta edited");

    let text = format_blame(&report);
    assert!(text.contains(&r2.revision_id));
    assert!(text.contains(&r3.revision_id));
    assert!(text.contains("Alpha edited"));
    assert!(text.contains("Beta edited"));
}

#[test]
fn pending_suggest_redline_accept_reject() {
    use crate::history::{
        PendingActionOptions, SuggestOptions, accept_pending, format_pending, list_pending,
        pending_redline, reject_pending, suggest_pending,
    };
    use crate::verify::verify_tes_file;

    let dir = tempdir().unwrap();
    let path = sample(dir.path());
    let hash = file_source_hash(&path).unwrap();

    let report = suggest_pending(
        &path,
        r#"[{"op":"set_text","chunk_id":1,"body":"Pending body"}]"#,
        &SuggestOptions {
            source_hash: hash.clone(),
            message: Some("try this".into()),
            ..SuggestOptions::default()
        },
    )
    .unwrap();
    assert_eq!(report.ids.len(), 1);
    assert!(verify_tes_file(&path, true).unwrap().ok);

    // Body unchanged until accept.
    let raw = crate::catalog::TesFile::open(&path).unwrap();
    let entry = raw.chunk_by_id(1).unwrap();
    let decoded = raw.decode_payload(entry).unwrap();
    let (_, body) = crate::catalog::chunk::decode_text_payload(decoded.as_ref()).unwrap();
    assert_eq!(body, "First body");

    let pending = list_pending(&path).unwrap();
    assert_eq!(pending.len(), 1);
    assert!(
        format_pending(&pending).contains("Pending body")
            || format_pending(&pending).contains("set_text")
    );

    // Footer rewrite changes the on-disk source hash.
    let hash = file_source_hash(&path).unwrap();
    let redline = pending_redline(&path, &hash).unwrap();
    assert!(redline.contains("Pending body") || redline.contains('+') || redline.contains('-'));

    // Reject restores empty pending; body still original.
    let rejected = reject_pending(
        &path,
        &PendingActionOptions {
            source_hash: hash,
            ids: report.ids.clone(),
        },
    )
    .unwrap();
    assert_eq!(rejected.pending_count, 0);
    assert!(list_pending(&path).unwrap().is_empty());

    let hash = file_source_hash(&path).unwrap();
    let again = suggest_pending(
        &path,
        r#"[{"op":"set_text","chunk_id":1,"body":"Accepted body"}]"#,
        &SuggestOptions {
            source_hash: hash,
            message: Some("ship it".into()),
            ..SuggestOptions::default()
        },
    )
    .unwrap();
    let hash = file_source_hash(&path).unwrap();
    let accepted = accept_pending(
        &path,
        &PendingActionOptions {
            source_hash: hash,
            ids: again.ids.clone(),
        },
    )
    .unwrap();
    assert_eq!(accepted.pending_count, 0);
    assert!(verify_tes_file(&path, true).unwrap().ok);

    let raw = crate::catalog::TesFile::open(&path).unwrap();
    let entry = raw.chunk_by_id(1).unwrap();
    let decoded = raw.decode_payload(entry).unwrap();
    let (_, body) = crate::catalog::chunk::decode_text_payload(decoded.as_ref()).unwrap();
    assert_eq!(body, "Accepted body");
    assert!(list_pending(&path).unwrap().is_empty());
}

fn two_chunk_sample(dir: &Path) -> PathBuf {
    let path = dir.join("two.tes");
    let mut s = TesWriterSession::create(&path, DocKind::Note);
    s.set_catalog(DocumentCatalog::new(
        "550e8400-e29b-41d4-a716-446655440099",
        "Merge note",
        "2026-07-28T00:00:00Z",
        "2026-07-28T00:00:00Z",
        DocKind::Note,
    ))
    .unwrap();
    s.add_text_chunk(&TextHeader::paragraph(), "Alpha").unwrap();
    s.add_text_chunk(&TextHeader::paragraph(), "Beta").unwrap();
    s.commit().unwrap();
    path
}

#[test]
fn merge_non_overlapping_chunk_edits() {
    use crate::history::merge_files;

    let dir = tempdir().unwrap();
    let base = two_chunk_sample(dir.path());
    let ours = dir.path().join("ours.tes");
    let theirs = dir.path().join("theirs.tes");
    fs::copy(&base, &ours).unwrap();
    fs::copy(&base, &theirs).unwrap();

    let hash = file_source_hash(&ours).unwrap();
    apply_ops(
        &ours,
        &[TesOp::SetText {
            chunk_id: 1,
            body: "Alpha ours".into(),
            role: None,
            level: None,
            class: None,
        }],
        &crate::edit::EditWriteOptions::new(hash, false),
    )
    .unwrap();

    let hash = file_source_hash(&theirs).unwrap();
    apply_ops(
        &theirs,
        &[TesOp::SetText {
            chunk_id: 2,
            body: "Beta theirs".into(),
            role: None,
            level: None,
            class: None,
        }],
        &crate::edit::EditWriteOptions::new(hash, false),
    )
    .unwrap();

    let report = merge_files(&base, &ours, &theirs).unwrap();
    assert_eq!(report.from_ours, vec![1]);
    assert_eq!(report.from_theirs, vec![2]);
    assert!(verify_tes_file(&ours, true).unwrap().ok);

    let file = TesFile::open(&ours).unwrap();
    let e1 = file.chunk_by_id(1).unwrap();
    let e2 = file.chunk_by_id(2).unwrap();
    let (_, b1) =
        crate::catalog::chunk::decode_text_payload(file.decode_payload(e1).unwrap().as_ref())
            .unwrap();
    let (_, b2) =
        crate::catalog::chunk::decode_text_payload(file.decode_payload(e2).unwrap().as_ref())
            .unwrap();
    assert_eq!(b1, "Alpha ours");
    assert_eq!(b2, "Beta theirs");
}

#[test]
fn merge_overlapping_chunk_edits_conflict() {
    use crate::history::merge_files;

    let dir = tempdir().unwrap();
    let base = two_chunk_sample(dir.path());
    let ours = dir.path().join("ours.tes");
    let theirs = dir.path().join("theirs.tes");
    fs::copy(&base, &ours).unwrap();
    fs::copy(&base, &theirs).unwrap();

    let hash = file_source_hash(&ours).unwrap();
    apply_ops(
        &ours,
        &[TesOp::SetText {
            chunk_id: 1,
            body: "Alpha ours".into(),
            role: None,
            level: None,
            class: None,
        }],
        &crate::edit::EditWriteOptions::new(hash, false),
    )
    .unwrap();

    let hash = file_source_hash(&theirs).unwrap();
    apply_ops(
        &theirs,
        &[TesOp::SetText {
            chunk_id: 1,
            body: "Alpha theirs".into(),
            role: None,
            level: None,
            class: None,
        }],
        &crate::edit::EditWriteOptions::new(hash, false),
    )
    .unwrap();

    let before = fs::read(&ours).unwrap();
    let err = merge_files(&base, &ours, &theirs).unwrap_err();
    assert!(matches!(err, TesError::MergeConflict { .. }), "{err}");
    assert_eq!(fs::read(&ours).unwrap(), before, "ours must be untouched");
}
