use std::path::Path;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use super::{
    MarkdownBlock, MarkdownImportOptions, MarkdownImportReport, parse_front_matter,
    parse_markdown_blocks, rewrite_wikilinks,
};
use crate::catalog::{DocumentCatalog, TesFile, TesWriterSession, TextRole, doc_id_from_seed};
use crate::error::{Result, TesError};

/// Import a Markdown file and seal a `.tes` document.
///
/// # Errors
///
/// Returns [`TesError::Io`] if the source cannot be read or the `.tes` cannot be written,
/// [`TesError::InvalidDocId`] if `options.doc_id` is not a UUID, or catalog/session
/// errors from [`TesWriterSession`].
pub fn import_markdown_v0(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    options: &MarkdownImportOptions,
) -> Result<MarkdownImportReport> {
    let input = input.as_ref();
    let output = output.as_ref();
    let source = std::fs::read_to_string(input)?;
    let (front, markdown) = parse_front_matter(&source);
    let markdown = if let Some(resolver) = options.wikilink_resolver.as_ref() {
        rewrite_wikilinks(markdown, resolver.as_ref())
    } else {
        markdown.to_owned()
    };
    let blocks = parse_markdown_blocks(&markdown);

    let title = options
        .title
        .clone()
        .or(front.title.clone())
        .or_else(|| first_heading(&blocks))
        .or_else(|| {
            input
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Untitled".to_owned());

    let seed = options
        .doc_id_seed
        .clone()
        .or_else(|| front.id.clone())
        .or_else(|| {
            input
                .file_name()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
        });
    let doc_id = resolve_import_doc_id(output, options.doc_id.as_deref(), seed.as_deref())?;

    let mut tags = front.tags.clone();
    extend_unique(&mut tags, &options.tags);
    let mut aliases = front.aliases.clone();
    extend_unique(&mut aliases, &options.aliases);
    let slug = if options.slug_override {
        options.slug.clone()
    } else {
        options.slug.clone().or(front.id.clone())
    };

    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|err| std::io::Error::other(format!("format import timestamp: {err}")))?;

    let mut catalog = DocumentCatalog::new(&doc_id, &title, &now, &now, options.doc_kind);
    catalog.tags = tags;
    catalog.category.clone_from(&options.category);
    catalog.section.clone_from(&options.section);
    catalog.aliases = aliases;
    catalog.slug.clone_from(&slug);

    let _ = std::fs::remove_file(output);
    let mut session = TesWriterSession::create(output, options.doc_kind);
    session.set_catalog(catalog)?;
    seal_text_blocks(&mut session, &blocks)?;
    session.commit()?;

    Ok(MarkdownImportReport {
        input: input.to_path_buf(),
        output: output.to_path_buf(),
        doc_id,
        title,
        chunk_count: blocks.len(),
        slug,
    })
}

/// Resolve `doc_id` for import: explicit option, else keep existing output catalog
/// (D2), else `UUIDv5` from seed, else random.
///
/// # Errors
///
/// Returns [`TesError::InvalidDocId`] when an explicit id is not a UUID.
pub fn resolve_import_doc_id(
    output: &Path,
    explicit: Option<&str>,
    seed: Option<&str>,
) -> Result<String> {
    if let Some(value) = explicit {
        return Ok(Uuid::parse_str(value)
            .map_err(|_| TesError::InvalidDocId {
                value: value.to_owned(),
            })?
            .to_string());
    }
    if output.is_file()
        && let Ok(file) = TesFile::open(output)
        && let Some(catalog) = file.catalog()
    {
        return Ok(catalog.doc_id.clone());
    }
    if let Some(seed) = seed {
        return Ok(doc_id_from_seed(seed).to_string());
    }
    Ok(Uuid::new_v4().to_string())
}

/// Append text blocks and materialize pending outbound links into `TLNK`.
///
/// # Errors
///
/// Returns session / link validation errors.
pub fn seal_text_blocks(session: &mut TesWriterSession, blocks: &[MarkdownBlock]) -> Result<()> {
    for block in blocks {
        session.add_text_with_outbound_links(
            block.header.clone(),
            &block.body,
            &block.pending_links,
        )?;
    }
    Ok(())
}

fn first_heading(blocks: &[MarkdownBlock]) -> Option<String> {
    blocks
        .iter()
        .find(|b| b.header.role == TextRole::Heading)
        .map(|b| b.body.clone())
}

fn extend_unique(dst: &mut Vec<String>, extras: &[String]) {
    for item in extras {
        if !dst.iter().any(|existing| existing == item) {
            dst.push(item.clone());
        }
    }
}
