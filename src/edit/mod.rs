//! Virtual editor mutation protocol (`edit-read` / `edit-write` / `apply`).
//!
//! Safe mutation gate from `docs/security.md`:
//! advisory lock → source-hash recheck → sibling temp compile → deep verify →
//! atomic replace.

mod ops;
pub mod tessprek;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::catalog::chunk::{CitePayload, TextHeader};
use crate::catalog::document::DocumentCatalog;
use crate::catalog::history::attach_footer;
use crate::catalog::index::ChunkType;
use crate::catalog::media::{AttachmentPayload, FigureRef, ImagePayload};
use crate::catalog::slide::SlidePayload;
use crate::catalog::{TesFile, TesWriterSession};
use crate::error::{Result, TesError};
use crate::verify::{TesVerifyReport, verify_bytes, verify_tes_file};

pub use ops::{CatalogPatch, TesOp, apply_ops_to_blocks, parse_ops_json};
pub use tessprek::markers;
pub use tessprek::{
    decode_tessprek, encode_content_blocks, encode_tessprek, normalize_tessprek,
    tessprek_needs_format,
};

/// One reading-order block in a Tessprek projection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // CitePayload carries optional BibEntry.
pub enum ContentBlock {
    /// Text chunk.
    Text {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Semantic header.
        header: TextHeader,
        /// UTF-8 body.
        body: String,
        /// Outbound links over [`Self::Text::body`] (Tessprek / markdown).
        pending_links: Vec<crate::catalog::OutboundLink>,
    },
    /// Figure chunk referencing an image payload.
    Figure {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Figure metadata + alt.
        figure: FigureRef,
    },
    /// Cite chunk.
    Cite {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Cite payload.
        cite: CitePayload,
    },
    /// Slide chunk with named region refs.
    Slide {
        /// Optional stable id from the source projection.
        chunk_id: Option<u64>,
        /// Slide payload.
        slide: SlidePayload,
    },
    /// Inert attachment chunk (metadata in Tessprek; bytes from source or media bag).
    Attachment {
        /// Source chunk id, or a temporary id resolved via [`EditMediaBag`].
        chunk_id: Option<u64>,
        /// Safe basename.
        filename: String,
        /// IANA media type.
        media_type: String,
        /// Optional caption.
        caption: Option<String>,
        /// Declared integrity hash (checked against bytes on write).
        sha256: String,
    },
}

impl ContentBlock {
    /// Projected chunk id, when known.
    #[must_use]
    pub fn chunk_id(&self) -> Option<u64> {
        match self {
            Self::Text { chunk_id, .. }
            | Self::Figure { chunk_id, .. }
            | Self::Cite { chunk_id, .. }
            | Self::Slide { chunk_id, .. }
            | Self::Attachment { chunk_id, .. } => *chunk_id,
        }
    }
}

/// Result of `edit-read`.
#[derive(Debug, Clone)]
pub struct EditReadReport {
    /// Hex SHA-256 of the on-disk `.tes` bytes.
    pub source_hash: String,
    /// Tessera Markdown buffer.
    pub tessprek: String,
}

/// New image / attachment payloads injected during [`edit_write`].
///
/// Tessprek (or Fluid) may reference temporary chunk ids that are not present in
/// the source `.tes`. Those ids are resolved from this bag when compiling.
#[derive(Debug, Clone, Default)]
pub struct EditMediaBag {
    /// Temporary image chunk id → payload (figure `image=` / `media:chunk-N`).
    pub images: Vec<(u64, ImagePayload)>,
    /// Temporary attachment chunk id → payload (attachment `chunk=`).
    pub attachments: Vec<(u64, AttachmentPayload)>,
}

impl EditMediaBag {
    /// Whether the bag carries any payloads.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty() && self.attachments.is_empty()
    }

    fn image_map(&self) -> Result<std::collections::HashMap<u64, &ImagePayload>> {
        unique_id_map(&self.images, "image")
    }

    fn attachment_map(&self) -> Result<std::collections::HashMap<u64, &AttachmentPayload>> {
        unique_id_map(&self.attachments, "attachment")
    }
}

fn unique_id_map<'a, T>(
    entries: &'a [(u64, T)],
    kind: &str,
) -> Result<std::collections::HashMap<u64, &'a T>> {
    let mut map = std::collections::HashMap::with_capacity(entries.len());
    for (id, payload) in entries {
        if map.insert(*id, payload).is_some() {
            return Err(TesError::EditOp {
                message: format!("duplicate media bag {kind} id {id}"),
            });
        }
    }
    Ok(map)
}

/// Options for mutation writes.
#[derive(Debug, Clone)]
pub struct EditWriteOptions {
    /// Expected source hash (required).
    pub source_hash: String,
    /// When true, compile + verify but do not replace the original.
    pub dry_run: bool,
    /// New media payloads referenced by temporary chunk ids in Tessprek.
    pub media: EditMediaBag,
}

impl EditWriteOptions {
    /// Build options with an empty media bag.
    #[must_use]
    pub fn new(source_hash: impl Into<String>, dry_run: bool) -> Self {
        Self {
            source_hash: source_hash.into(),
            dry_run,
            media: EditMediaBag::default(),
        }
    }
}

/// Result of a successful (or dry-run) write.
#[derive(Debug, Clone)]
pub struct EditWriteReport {
    /// Path that was mutated (or would be).
    pub path: PathBuf,
    /// Prior source hash that was checked.
    pub source_hash: String,
    /// New source hash after replace (`None` on dry-run).
    pub new_source_hash: Option<String>,
    /// Deep-verify report for the compiled temp file.
    pub verify: TesVerifyReport,
    /// Unified-ish Tessprek diff (before → after) for dry-run / agents.
    pub diff: String,
    /// Whether the original was replaced.
    pub replaced: bool,
}

/// Hex-encoded SHA-256 of file bytes.
///
/// # Errors
///
/// Returns [`TesError::Io`] if the file cannot be read.
pub fn file_source_hash(path: impl AsRef<Path>) -> Result<String> {
    let bytes = fs::read(path.as_ref())?;
    Ok(hash_bytes(&bytes))
}

/// Hex-encoded SHA-256 of an in-memory buffer.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_encode(&digest)
}

/// Decode a `.tes` file to Tessera Markdown for an editor buffer.
///
/// # Errors
///
/// Returns I/O or decode errors from opening/encoding the file.
pub fn edit_read(path: impl AsRef<Path>) -> Result<EditReadReport> {
    let path = path.as_ref();
    let source_hash = file_source_hash(path)?;
    let file = TesFile::open(path)?;
    let tessprek = encode_tessprek(&file, &source_hash)?;
    Ok(EditReadReport {
        source_hash,
        tessprek,
    })
}

/// Compile Tessera Markdown and atomically replace `path` under lock + hash check.
///
/// New image/attachment bytes may be supplied via [`EditWriteOptions::media`]
/// (see [`edit_write_with_media`]).
///
/// # Errors
///
/// Returns lock, hash mismatch, parse, verify, or I/O errors. On failure the
/// original file is left untouched.
pub fn edit_write(
    path: impl AsRef<Path>,
    tessprek: &str,
    options: &EditWriteOptions,
) -> Result<EditWriteReport> {
    edit_write_inner(path, tessprek, options)
}

/// Like [`edit_write`], with an explicit media bag for newly injected payloads.
///
/// Temporary ids referenced by figure `image=` / `media:chunk-N` or attachment
/// `chunk=` are resolved from `media` when absent from the source file.
///
/// # Errors
///
/// Same as [`edit_write`].
pub fn edit_write_with_media(
    path: impl AsRef<Path>,
    tessprek: &str,
    options: &EditWriteOptions,
    media: EditMediaBag,
) -> Result<EditWriteReport> {
    let mut options = options.clone();
    options.media = media;
    edit_write_inner(path, tessprek, &options)
}

fn edit_write_inner(
    path: impl AsRef<Path>,
    tessprek: &str,
    options: &EditWriteOptions,
) -> Result<EditWriteReport> {
    let path = path.as_ref();
    let _lock = AdvisoryLock::acquire(path)?;
    let current_hash = file_source_hash(path)?;
    if current_hash != options.source_hash {
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: current_hash,
        });
    }

    let before = edit_read(path)?;
    let source = TesFile::open(path)?;
    let blocks = decode_tessprek(tessprek)?;
    let compiled = seal_with_history(
        &source,
        compile_blocks_to_bytes(&source, &blocks, None, &options.media)?,
    )?;

    let verify = verify_bytes(path, &compiled, true);
    if !verify.ok {
        let first = verify
            .findings
            .iter()
            .find(|f| matches!(f.severity, crate::verify::Severity::Error))
            .map_or_else(|| "deep verify failed".into(), |f| f.message.clone());
        return Err(TesError::EditVerifyFailed { message: first });
    }

    let after_tessprek = {
        // Encode from compiled bytes via a temp file for the diff projection.
        let tmp_for_diff = sibling_temp_path(path, "diff");
        fs::write(&tmp_for_diff, &compiled)?;
        let report = edit_read(&tmp_for_diff);
        let _ = fs::remove_file(&tmp_for_diff);
        report?.tessprek
    };
    let diff = simple_diff(
        &normalize_tessprek_for_diff(&before.tessprek),
        &normalize_tessprek_for_diff(&after_tessprek),
    );

    if options.dry_run {
        return Ok(EditWriteReport {
            path: path.to_path_buf(),
            source_hash: current_hash,
            new_source_hash: None,
            verify,
            diff,
            replaced: false,
        });
    }

    let tmp = sibling_temp_path(path, "tmp");
    {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(&compiled)?;
        file.sync_all()?;
    }
    // Re-check after compile (another writer could have raced before we locked
    // in pathological cases; lock covers the common editor path).
    let recheck = file_source_hash(path)?;
    if recheck != options.source_hash {
        let _ = fs::remove_file(&tmp);
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: recheck,
        });
    }
    fs::rename(&tmp, path)?;
    let new_source_hash = file_source_hash(path)?;
    // Confirm the replaced file still verifies on disk.
    let on_disk = verify_tes_file(path, true)?;
    if !on_disk.ok {
        return Err(TesError::EditVerifyFailed {
            message: "post-replace deep verify failed".into(),
        });
    }

    Ok(EditWriteReport {
        path: path.to_path_buf(),
        source_hash: current_hash,
        new_source_hash: Some(new_source_hash),
        verify: on_disk,
        diff,
        replaced: true,
    })
}

/// Apply a Tessprek patch file through the same mutation gate as [`edit_write`].
///
/// # Errors
///
/// Same as [`edit_write`].
pub fn apply_patch(
    path: impl AsRef<Path>,
    patch_tessprek: &str,
    options: &EditWriteOptions,
) -> Result<EditWriteReport> {
    edit_write(path, patch_tessprek, options)
}

/// Apply typed JSON ops: project → mutate → compile → verify → replace.
///
/// # Errors
///
/// Returns op/parse/verify/I/O errors. Original untouched on failure.
pub fn apply_ops(
    path: impl AsRef<Path>,
    ops: &[TesOp],
    options: &EditWriteOptions,
) -> Result<EditWriteReport> {
    let path = path.as_ref();
    let _lock = AdvisoryLock::acquire(path)?;
    let current_hash = file_source_hash(path)?;
    if current_hash != options.source_hash {
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: current_hash,
        });
    }

    let before = edit_read(path)?;
    let source = TesFile::open(path)?;
    let mut blocks = decode_tessprek(&before.tessprek)?;
    let mut catalog = CatalogPatch::from_catalog(source.catalog());
    apply_ops_to_blocks(&mut blocks, &mut catalog, ops)?;
    let compiled = seal_with_history(
        &source,
        compile_blocks_to_bytes(&source, &blocks, Some(&catalog), &EditMediaBag::default())?,
    )?;

    let verify = verify_bytes(path, &compiled, true);
    if !verify.ok {
        let first = verify
            .findings
            .iter()
            .find(|f| matches!(f.severity, crate::verify::Severity::Error))
            .map_or_else(|| "deep verify failed".into(), |f| f.message.clone());
        return Err(TesError::EditVerifyFailed { message: first });
    }

    let after_tessprek = {
        let tmp_for_diff = sibling_temp_path(path, "diff");
        fs::write(&tmp_for_diff, &compiled)?;
        let report = edit_read(&tmp_for_diff);
        let _ = fs::remove_file(&tmp_for_diff);
        report?.tessprek
    };
    let diff = simple_diff(
        &normalize_tessprek_for_diff(&before.tessprek),
        &normalize_tessprek_for_diff(&after_tessprek),
    );

    if options.dry_run {
        return Ok(EditWriteReport {
            path: path.to_path_buf(),
            source_hash: current_hash,
            new_source_hash: None,
            verify,
            diff,
            replaced: false,
        });
    }

    let tmp = sibling_temp_path(path, "tmp");
    {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(&compiled)?;
        file.sync_all()?;
    }
    let recheck = file_source_hash(path)?;
    if recheck != options.source_hash {
        let _ = fs::remove_file(&tmp);
        return Err(TesError::SourceHashMismatch {
            expected: options.source_hash.clone(),
            found: recheck,
        });
    }
    fs::rename(&tmp, path)?;
    let new_source_hash = file_source_hash(path)?;
    let on_disk = verify_tes_file(path, true)?;
    if !on_disk.ok {
        return Err(TesError::EditVerifyFailed {
            message: "post-replace deep verify failed".into(),
        });
    }

    Ok(EditWriteReport {
        path: path.to_path_buf(),
        source_hash: current_hash,
        new_source_hash: Some(new_source_hash),
        verify: on_disk,
        diff,
        replaced: true,
    })
}

/// Re-attach an existing THST footer so edits do not drop revision history.
fn seal_with_history(source: &TesFile, body: Vec<u8>) -> Result<Vec<u8>> {
    match source.history()? {
        Some(history) => attach_footer(body, &history),
        None => Ok(body),
    }
}

fn compile_blocks_to_bytes(
    source: &TesFile,
    blocks: &[ContentBlock],
    catalog_patch: Option<&CatalogPatch>,
    media: &EditMediaBag,
) -> Result<Vec<u8>> {
    let catalog = catalog_for_compile(source, catalog_patch);
    let bag_images = media.image_map()?;
    let bag_attachments = media.attachment_map()?;
    let image_payloads = load_referenced_images(source, blocks, &bag_images)?;

    // Build into an ephemeral session path (encode_file only; no commit).
    let phantom = PathBuf::from("__tessera_edit_encode__.tes");
    let mut session = TesWriterSession::create(&phantom, source.superblock().doc_kind);
    session.set_catalog(catalog)?;

    let mut image_id_map = std::collections::HashMap::new();
    for (old_id, payload) in &image_payloads {
        let new_id = session.add_image_chunk(payload)?;
        image_id_map.insert(*old_id, new_id);
    }

    for block in blocks {
        write_compiled_block(&mut session, source, block, &image_id_map, &bag_attachments)?;
    }
    session.encode_file()
}

fn catalog_for_compile(source: &TesFile, patch: Option<&CatalogPatch>) -> DocumentCatalog {
    let doc_kind = source.superblock().doc_kind;
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());
    let mut catalog = source.catalog().cloned().unwrap_or_else(|| {
        DocumentCatalog::new(
            uuid::Uuid::new_v4().to_string(),
            "Untitled",
            now.clone(),
            now.clone(),
            doc_kind,
        )
    });
    if let Some(patch) = patch {
        patch.apply_to(&mut catalog);
    }
    catalog.modified = now;
    catalog
}

fn load_referenced_images(
    source: &TesFile,
    blocks: &[ContentBlock],
    bag_images: &std::collections::HashMap<u64, &ImagePayload>,
) -> Result<Vec<(u64, ImagePayload)>> {
    let mut needed_images: Vec<u64> = blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Figure { figure, .. } => Some(figure.image_chunk_id),
            _ => None,
        })
        .collect();
    needed_images.sort_unstable();
    needed_images.dedup();

    let mut image_payloads = Vec::with_capacity(needed_images.len());
    for old_id in needed_images {
        if let Some(payload) = bag_images.get(&old_id) {
            payload.validate()?;
            image_payloads.push((old_id, (*payload).clone()));
            continue;
        }
        let raw = source_payload_bytes(source, old_id, ChunkType::Image, "image")?;
        image_payloads.push((old_id, ImagePayload::from_bytes(raw.as_ref())?));
    }
    Ok(image_payloads)
}

fn source_payload_bytes<'a>(
    source: &'a TesFile,
    chunk_id: u64,
    expected: ChunkType,
    kind: &str,
) -> Result<std::borrow::Cow<'a, [u8]>> {
    let entry = source.chunk_by_id(chunk_id).map_err(|_| TesError::EditOp {
        message: format!("{kind} chunk {chunk_id} missing from source and media bag"),
    })?;
    if entry.chunk_type != expected {
        return Err(TesError::EditOp {
            message: format!("chunk {chunk_id} is not a {kind}"),
        });
    }
    source.decode_payload(entry)
}

fn write_compiled_block(
    session: &mut TesWriterSession,
    source: &TesFile,
    block: &ContentBlock,
    image_id_map: &std::collections::HashMap<u64, u64>,
    bag_attachments: &std::collections::HashMap<u64, &AttachmentPayload>,
) -> Result<()> {
    match block {
        ContentBlock::Text {
            header,
            body,
            pending_links,
            ..
        } => {
            let outbound = text_outbound_links(source, header, pending_links);
            session.add_text_with_outbound_links(header.clone(), body, &outbound)?;
        }
        ContentBlock::Figure { figure, .. } => {
            let mut figure = figure.clone();
            let Some(&new_id) = image_id_map.get(&figure.image_chunk_id) else {
                return Err(TesError::EditOp {
                    message: format!("missing image payload for chunk {}", figure.image_chunk_id),
                });
            };
            figure.image_chunk_id = new_id;
            session.add_figure(&figure)?;
        }
        ContentBlock::Cite { cite, .. } => write_cite_block(session, source, block, cite)?,
        ContentBlock::Slide { slide, .. } => {
            session.add_slide(slide)?;
        }
        ContentBlock::Attachment { .. } => {
            write_attachment_block(session, source, bag_attachments, block)?;
        }
    }
    Ok(())
}

fn text_outbound_links(
    source: &TesFile,
    header: &TextHeader,
    pending_links: &[crate::catalog::OutboundLink],
) -> Vec<crate::catalog::OutboundLink> {
    if !pending_links.is_empty() {
        return pending_links.to_vec();
    }
    // Remap existing Link spans from the source TLNK.
    header
        .spans
        .iter()
        .filter_map(|span| {
            let crate::catalog::InlineKind::Link { link_id } = &span.kind else {
                return None;
            };
            let entry = source.links().get(*link_id as usize)?;
            Some(crate::catalog::OutboundLink {
                start: span.start,
                end: span.end,
                dest: entry.target.markdown_destination(),
            })
        })
        .collect()
}

fn write_cite_block(
    session: &mut TesWriterSession,
    source: &TesFile,
    block: &ContentBlock,
    cite: &CitePayload,
) -> Result<()> {
    // Prefer full cite payload from source when id matches (keeps `source` bib).
    if let Some(id) = block.chunk_id()
        && let Ok(entry) = source.chunk_by_id(id)
        && entry.chunk_type == ChunkType::Cite
    {
        let raw = source.decode_payload(entry)?;
        let mut full = CitePayload::from_bytes(raw.as_ref())?;
        full.quote.clone_from(&cite.quote);
        if cite.label.is_some() {
            full.label.clone_from(&cite.label);
        }
        if cite.target_doc_id.is_some() {
            full.target_doc_id.clone_from(&cite.target_doc_id);
        }
        if cite.target_chunk_id.is_some() {
            full.target_chunk_id = cite.target_chunk_id;
        }
        if cite.page.is_some() {
            full.page = cite.page;
        }
        session.add_cite_chunk(&full)?;
        return Ok(());
    }
    session.add_cite_chunk(cite)?;
    Ok(())
}

fn write_attachment_block(
    session: &mut TesWriterSession,
    source: &TesFile,
    bag_attachments: &std::collections::HashMap<u64, &AttachmentPayload>,
    block: &ContentBlock,
) -> Result<()> {
    let ContentBlock::Attachment {
        chunk_id,
        filename,
        media_type,
        caption,
        sha256,
    } = block
    else {
        return Err(TesError::EditOp {
            message: "internal: write_attachment_block expected Attachment".into(),
        });
    };
    let Some(id) = *chunk_id else {
        return Err(TesError::EditOp {
            message: "attachment directives require a chunk id (source or media bag)".into(),
        });
    };

    let mut payload = if let Some(bag) = bag_attachments.get(&id) {
        (*bag).clone()
    } else {
        let raw = source_payload_bytes(source, id, ChunkType::Attachment, "attachment")?;
        AttachmentPayload::from_bytes(raw.as_ref())?
    };

    // Allow Tessprek metadata edits that keep the same bytes.
    if payload.sha256 != *sha256 {
        return Err(TesError::EditOp {
            message: format!(
                "attachment chunk {id} sha256 mismatch: tessprek={sha256}, payload={}",
                payload.sha256
            ),
        });
    }
    payload.filename.clone_from(filename);
    payload.media_type.clone_from(media_type);
    payload.caption.clone_from(caption);
    payload.validate()?;
    session.add_attachment_chunk(&payload)?;
    Ok(())
}

fn sibling_temp_path(path: &Path, tag: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("document.tes");
    let pid = std::process::id();
    parent.join(format!(".{stem}.{tag}.{pid}"))
}

fn advisory_lock_path(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("document.tes");
    parent.join(format!(".{name}.lock"))
}

/// Advisory per-file lock via exclusive lock-file create.
struct AdvisoryLock {
    path: PathBuf,
}

impl AdvisoryLock {
    fn acquire(target: &Path) -> Result<Self> {
        let path = advisory_lock_path(target);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let _ = writeln!(file, "{}", std::process::id());
                let _ = file.sync_all();
                Ok(Self { path })
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(TesError::EditLocked {
                    path: path.display().to_string(),
                })
            }
            Err(err) => Err(err.into()),
        }
    }
}

impl Drop for AdvisoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn normalize_tessprek_for_diff(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.starts_with(markers::HEADER_PREFIX) && line.contains("source-hash=") {
                // Ignore hash churn from re-encoding into a temp file.
                format!(
                    "{} format={} version={} source-hash=<hash>{}",
                    markers::HEADER_PREFIX,
                    markers::FORMAT,
                    markers::VERSION,
                    markers::COMMENT_SUFFIX,
                )
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn simple_diff(before: &str, after: &str) -> String {
    if before == after {
        return String::from("(no changes)\n");
    }
    let mut out = String::from("--- before\n+++ after\n");
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let max = before_lines.len().max(after_lines.len());
    for i in 0..max {
        let a = before_lines.get(i).copied();
        let b = after_lines.get(i).copied();
        match (a, b) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                let _ = writeln!(out, "-{a}");
                let _ = writeln!(out, "+{b}");
            }
            (Some(a), None) => {
                let _ = writeln!(out, "-{a}");
            }
            (None, Some(b)) => {
                let _ = writeln!(out, "+{b}");
            }
            (None, None) => {}
        }
    }
    out
}

use std::fmt::Write as _;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::chunk::TextHeader;
    use crate::catalog::index::ChunkType;
    use crate::layout::DocKind;
    use tempfile::tempdir;

    fn sample_note(dir: &Path) -> PathBuf {
        let path = dir.join("note.tes");
        let mut session = TesWriterSession::create(&path, DocKind::Note);
        session
            .set_catalog(DocumentCatalog::new(
                "550e8400-e29b-41d4-a716-446655440000",
                "Meeting notes",
                "2026-07-27T00:00:00Z",
                "2026-07-27T00:00:00Z",
                DocKind::Note,
            ))
            .unwrap();
        session
            .add_text_chunk(&TextHeader::heading(1), "Agenda")
            .unwrap();
        session
            .add_text_chunk(&TextHeader::paragraph(), "Ship Tessprek")
            .unwrap();
        session.commit().unwrap();
        path
    }

    fn apply_json(path: &Path, ops_json: &str, dry_run: bool) -> EditWriteReport {
        let read = edit_read(path).unwrap();
        apply_ops(
            path,
            &parse_ops_json(ops_json).unwrap(),
            &EditWriteOptions::new(read.source_hash, dry_run),
        )
        .unwrap()
    }

    fn assert_meta(
        aliases: &[&str],
        slug: Option<&str>,
        category: Option<&str>,
        got_aliases: &[String],
        got_slug: &Option<String>,
        got_category: &Option<String>,
    ) {
        assert_eq!(got_aliases, aliases);
        assert_eq!(got_slug.as_deref(), slug);
        assert_eq!(got_category.as_deref(), category);
    }

    #[test]
    fn edit_read_write_round_trip() {
        let dir = tempdir().unwrap();
        let path = sample_note(dir.path());
        let read = edit_read(&path).unwrap();
        assert!(read.tessprek.contains("Agenda"));
        assert_eq!(read.source_hash.len(), 64);

        let edited = read.tessprek.replace("Ship Tessprek", "Ship edit protocol");
        // Keep directives intact; only body changed.
        let report = edit_write(
            &path,
            &edited,
            &EditWriteOptions::new(read.source_hash.clone(), false),
        )
        .unwrap();
        assert!(report.replaced);
        let again = edit_read(&path).unwrap();
        assert!(again.tessprek.contains("Ship edit protocol"));
        assert_ne!(again.source_hash, read.source_hash);
    }

    #[test]
    fn source_hash_mismatch_rejects() {
        let dir = tempdir().unwrap();
        let path = sample_note(dir.path());
        let read = edit_read(&path).unwrap();
        let err = edit_write(
            &path,
            &read.tessprek,
            &EditWriteOptions::new("deadbeef", false),
        )
        .unwrap_err();
        assert!(matches!(err, TesError::SourceHashMismatch { .. }));
    }

    #[test]
    fn apply_ops_set_text_dry_run() {
        let dir = tempdir().unwrap();
        let path = sample_note(dir.path());
        let report = apply_json(
            &path,
            r#"[{"op":"set_text","chunk_id":2,"body":"Updated body"},{"op":"set_title","title":"Renamed"}]"#,
            true,
        );
        assert!(!report.replaced);
        assert!(report.diff.contains("Updated body") || report.diff.contains('+'));
        // Original unchanged.
        let file = TesFile::open(&path).unwrap();
        assert_eq!(file.catalog().unwrap().title, "Meeting notes");
    }

    #[test]
    fn apply_ops_catalog_aliases_slug_category_round_trip() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        let path = sample_note(vault);
        let report = apply_json(
            &path,
            r#"[
              {"op":"set_aliases","aliases":["American Fiction","Percival"]},
              {"op":"set_slug","slug":"Erasure"},
              {"op":"set_category","category":"Literature"}
            ]"#,
            false,
        );
        assert!(report.replaced);

        let file = TesFile::open(&path).unwrap();
        let cat = file.catalog().unwrap();
        assert_meta(
            &["American Fiction", "Percival"],
            Some("Erasure"),
            Some("Literature"),
            &cat.aliases,
            &cat.slug,
            &cat.category,
        );

        crate::vault::rebuild_vault_index(vault).unwrap();
        let index = crate::vault::load_vault_index(vault).unwrap().unwrap();
        let entry = index
            .entries
            .iter()
            .find(|e| e.doc_id == cat.doc_id)
            .expect("note in vault.tes");
        assert_meta(
            &["American Fiction", "Percival"],
            Some("Erasure"),
            Some("Literature"),
            &entry.aliases,
            &entry.slug,
            &entry.category,
        );
    }

    #[test]
    fn edit_write_with_media_injects_image_and_attachment() {
        let dir = tempdir().unwrap();
        let path = sample_note(dir.path());
        let read = edit_read(&path).unwrap();

        let image = ImagePayload {
            media_type: "image/png".into(),
            width_px: 2,
            height_px: 2,
            data: vec![0x89, 0x50, 0x4e, 0x47],
        };
        let att = AttachmentPayload::new(
            "application/pdf",
            "notes.pdf",
            b"%PDF-1.4 inject-test".to_vec(),
            Some("Handout".into()),
        )
        .unwrap();

        let tessprek = format!(
            "{}\n<!-- tes chunk=10 type=figure image=900001 placement=flow caption=\"Shot\" -->\n![Injected](media:chunk-900001)\n\n<!-- tes chunk=11 type=attachment filename=\"notes.pdf\" media_type=application/pdf sha256={} caption=\"Handout\" -->\n",
            read.tessprek.trim_end(),
            att.sha256,
        );
        let media = EditMediaBag {
            images: vec![(900_001, image.clone())],
            attachments: vec![(11, att.clone())],
        };
        let report = edit_write_with_media(
            &path,
            &tessprek,
            &EditWriteOptions::new(read.source_hash, false),
            media,
        )
        .unwrap();
        assert!(report.replaced);

        let file = TesFile::open(&path).unwrap();
        let mut saw_image = false;
        let mut saw_figure = false;
        let mut saw_attachment = false;
        for entry in file.chunks() {
            match entry.chunk_type {
                ChunkType::Image => {
                    let raw = file.decode_payload(entry).unwrap();
                    let payload = ImagePayload::from_bytes(raw.as_ref()).unwrap();
                    assert_eq!(payload.data, image.data);
                    saw_image = true;
                }
                ChunkType::Figure => {
                    let raw = file.decode_payload(entry).unwrap();
                    let figure = FigureRef::from_bytes(raw.as_ref()).unwrap();
                    assert_eq!(figure.alt_text, "Injected");
                    assert_eq!(figure.caption.as_deref(), Some("Shot"));
                    saw_figure = true;
                }
                ChunkType::Attachment => {
                    let raw = file.decode_payload(entry).unwrap();
                    let payload = AttachmentPayload::from_bytes(raw.as_ref()).unwrap();
                    assert_eq!(payload.filename, "notes.pdf");
                    assert_eq!(payload.data, att.data);
                    assert_eq!(payload.sha256, att.sha256);
                    saw_attachment = true;
                }
                _ => {}
            }
        }
        assert!(saw_image && saw_figure && saw_attachment);
    }

    #[test]
    fn edit_write_with_media_missing_bag_image_errors() {
        let dir = tempdir().unwrap();
        let path = sample_note(dir.path());
        let read = edit_read(&path).unwrap();
        let tessprek = format!(
            "{}\n<!-- tes chunk=10 type=figure image=900001 placement=flow -->\n![Missing](media:chunk-900001)\n",
            read.tessprek.trim_end(),
        );
        let err = edit_write_with_media(
            &path,
            &tessprek,
            &EditWriteOptions::new(read.source_hash, false),
            EditMediaBag::default(),
        )
        .unwrap_err();
        assert!(matches!(err, TesError::EditOp { .. }));
    }

    #[test]
    fn apply_ops_catalog_clear_fields_dry_run_unchanged() {
        let dir = tempdir().unwrap();
        let path = sample_note(dir.path());
        apply_json(
            &path,
            r#"[{"op":"set_aliases","aliases":["Keep"]},{"op":"set_slug","slug":"keep-slug"},{"op":"set_category","category":"KeepCat"}]"#,
            false,
        );
        let report = apply_json(
            &path,
            r#"[{"op":"set_aliases","aliases":[]},{"op":"set_slug","slug":null},{"op":"set_category","category":null}]"#,
            true,
        );
        assert!(!report.replaced);
        let cat = TesFile::open(&path).unwrap().catalog().unwrap().clone();
        assert_meta(
            &["Keep"],
            Some("keep-slug"),
            Some("KeepCat"),
            &cat.aliases,
            &cat.slug,
            &cat.category,
        );
    }
}
