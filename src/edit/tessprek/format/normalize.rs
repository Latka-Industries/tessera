use std::collections::BTreeMap;

use crate::error::Result;

use super::super::markers::parse_brace_command;
use super::super::{
    TessprekDocMeta, TessprekMediaEntry, encode_content_blocks, parse_attrs, parse_media_header,
    scan_tessprek_preamble, set_chunk_id, take_leading_tessera_header,
};
use super::build::build_content_blocks;
use crate::edit::ContentBlock;

/// Normalize a Tessprek buffer: infer text roles from Markdown shape, split
/// multi-block bodies, allocate/reuse `\ids{}` positionally, and re-emit
/// canonical Tessprek.
///
/// Free Markdown (no preceding `\text{}`) is accepted. Brace-command
/// Brace-command directives (`\figure{}` / `\cite{}` / `\quote{}` / `\ref{}` /
/// `\slide{}` / `\attach{}`) are preserved. Bibliography `\cite` stubs are
/// moved to the end of the document (after ids are bound, so attach/figure
/// identities stay stable across the reorder).
///
/// # Errors
///
/// Returns [`TesError::EditParse`] for malformed directives.
pub fn normalize_tessprek(input: &str) -> Result<String> {
    let lines: Vec<&str> = input.lines().collect();
    let declared_ids = extract_declared_ids(&lines);
    let mut blocks = build_content_blocks(&lines)?;
    assign_normalize_ids(&mut blocks, &declared_ids);
    park_biblio_cites_at_end(&mut blocks);

    let meta = extract_doc_meta(&lines);
    let media = extract_media_entries(&lines, &blocks);
    Ok(encode_content_blocks(&meta, &blocks, &[], &media))
}

/// Explicit `\attach{chunk=N}` (and any pre-set `chunk_id`) first; then positional
/// `\ids{}` / fresh ids. Parking must happen *after* so identities stay stable.
fn assign_normalize_ids(blocks: &mut [ContentBlock], declared_ids: &[u64]) {
    let mut ids = IdAllocator::new(declared_ids.iter().copied().collect());
    for block in blocks.iter_mut() {
        if let Some(explicit) = block.chunk_id() {
            set_chunk_id(block, ids.alloc(Some(explicit)));
        }
    }
    for (idx, block) in blocks.iter_mut().enumerate() {
        if block.chunk_id().is_none() {
            set_chunk_id(block, ids.alloc(declared_ids.get(idx).copied()));
        }
    }
}

/// Move bibliography `\cite` stubs after all other blocks (stable within each group).
fn park_biblio_cites_at_end(blocks: &mut Vec<ContentBlock>) {
    let (biblio, rest): (Vec<_>, Vec<_>) = std::mem::take(blocks).into_iter().partition(|b| {
        matches!(
            b,
            ContentBlock::Cite { cite, .. } if crate::io::cite::is_biblio_cite(cite)
        )
    });
    blocks.extend(rest);
    blocks.extend(biblio);
}

/// True when `normalize_tessprek(input)` would change the buffer (ignoring a
/// single trailing newline difference).
///
/// # Errors
///
/// Propagates normalize / parse errors.
pub fn tessprek_needs_format(input: &str) -> Result<bool> {
    let normalized = normalize_tessprek(input)?;
    Ok(normalize_newlines(&normalized) != normalize_newlines(input))
}

/// Reuses declared `\ids{}` values positionally; falls back to fresh ids
/// beyond the max declared value.
struct IdAllocator {
    reserved: std::collections::BTreeSet<u64>,
    emitted: std::collections::BTreeSet<u64>,
    next_fresh: u64,
}

impl IdAllocator {
    fn new(reserved: std::collections::BTreeSet<u64>) -> Self {
        let max = reserved.iter().next_back().copied().unwrap_or(0);
        Self {
            reserved,
            emitted: std::collections::BTreeSet::new(),
            next_fresh: max.saturating_add(1).max(1),
        }
    }

    fn alloc(&mut self, preferred: Option<u64>) -> u64 {
        if let Some(id) = preferred {
            if self.reserved.remove(&id) {
                self.emitted.insert(id);
                self.bump_fresh();
                return id;
            }
            if !self.emitted.contains(&id) {
                self.emitted.insert(id);
                self.bump_fresh();
                return id;
            }
        }
        loop {
            let id = self.next_fresh;
            self.next_fresh = self.next_fresh.saturating_add(1);
            if !self.emitted.contains(&id) && !self.reserved.contains(&id) {
                self.emitted.insert(id);
                return id;
            }
        }
    }

    fn bump_fresh(&mut self) {
        while self.emitted.contains(&self.next_fresh) || self.reserved.contains(&self.next_fresh) {
            self.next_fresh = self.next_fresh.saturating_add(1);
        }
    }
}

fn extract_doc_meta(lines: &[&str]) -> TessprekDocMeta {
    let Ok((attrs, _, _)) = take_leading_tessera_header(lines) else {
        return TessprekDocMeta::default();
    };
    let Ok(map) = parse_attrs(&attrs, 1) else {
        return TessprekDocMeta::default();
    };
    TessprekDocMeta::from_attrs(&map)
}

/// Lenient scan for the first `\ids{…}` list anywhere in the buffer.
fn extract_declared_ids(lines: &[&str]) -> Vec<u64> {
    for line in lines {
        let Some(("ids", inner)) = parse_brace_command(line.trim(), true) else {
            continue;
        };
        return inner
            .split(',')
            .filter_map(|s| s.trim().parse::<u64>().ok())
            .collect();
    }
    Vec::new()
}

/// Preserve `\media{…}` attrs across `tes format` when the sealed `.tes` is closed.
fn extract_media_entries(lines: &[&str], blocks: &[ContentBlock]) -> Vec<TessprekMediaEntry> {
    let mut declared = BTreeMap::new();
    if let Some(inner) = scan_tessprek_preamble(lines, 0).media_inner {
        for entry in parse_media_header(&inner) {
            declared.insert(entry.chunk_id, entry);
        }
    }
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for block in blocks {
        if let ContentBlock::Figure { figure, .. } = block {
            let id = figure.image_chunk_id;
            if id == 0 || !seen.insert(id) {
                continue;
            }
            out.push(declared.remove(&id).unwrap_or(TessprekMediaEntry {
                chunk_id: id,
                ..TessprekMediaEntry::default()
            }));
        }
    }
    out
}

pub(crate) fn normalize_newlines(s: &str) -> String {
    let mut t = s.replace("\r\n", "\n");
    if t.ends_with('\n') {
        t.pop();
    }
    t
}
