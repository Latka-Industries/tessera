//! `tes vault` (rebuild / list / import / membership for optional `vault.tes` TOC).

use std::fmt::Write as _;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use crate::error::TesError;
use crate::vault::{
    VaultIndexEntry, VaultMarkdownImportOptions, VaultSearchHit, VaultSearchMode,
    VaultSearchOptions, import_markdown_vault, list_vault_documents_filtered,
    load_registered_members, rebuild_vault_fts, rebuild_vault_index, register_member, search_vault,
    unregister_member, vault_fts_path, vault_index_path,
};

use super::super::args::{VaultCommands, VaultListArgs, VaultSearchArgs};

pub(in crate::cli) fn run_vault(root: &PathBuf, command: VaultCommands) -> Result<(), TesError> {
    match command {
        VaultCommands::Rebuild => {
            let path = rebuild_vault_index(root)?;
            println!("wrote\t{}", path.display());
        }
        VaultCommands::List(args) => run_vault_list(root, &args)?,
        VaultCommands::Import { source, json } => run_vault_import(root, &source, json)?,
        VaultCommands::Add { path } => {
            let member = register_member(root, &path)?;
            println!("added\t{}\t{}", member.kind.as_str(), member.path);
        }
        VaultCommands::Remove { path } => {
            unregister_member(root, &path)?;
            println!("removed\t{}", path.display());
        }
        VaultCommands::Members { json } => run_vault_members(root, json)?,
        VaultCommands::Search(args) => run_vault_search(root, &args)?,
    }
    Ok(())
}

fn run_vault_list(root: &PathBuf, args: &VaultListArgs) -> Result<(), TesError> {
    let report = list_vault_documents_filtered(
        root,
        args.tag.as_deref(),
        args.category.as_deref(),
        args.section.as_deref(),
        args.force_scan,
    )?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    print_list_source(root, &report);
    let use_table = if args.table {
        true
    } else if args.tsv {
        false
    } else {
        io::stdout().is_terminal()
    };
    if use_table {
        print_list_table(&report.entries);
    } else {
        print_list_tsv(&report.entries);
    }
    println!("documents={}", report.entries.len());
    Ok(())
}

fn run_vault_import(root: &PathBuf, source: &PathBuf, json: bool) -> Result<(), TesError> {
    let report = import_markdown_vault(source, root, &VaultMarkdownImportOptions::default())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    for entry in &report.imported {
        println!(
            "imported\t{}\t→\t{}\tdoc_id={}\tkind={}",
            entry.source, entry.output, entry.doc_id, entry.doc_kind
        );
    }
    for warn in &report.slug_collisions {
        eprintln!("warning: {warn}");
    }
    for warn in &report.doc_id_collisions {
        eprintln!("warning: {warn}");
    }
    if !report.unresolved_wikilinks.is_empty() {
        eprintln!(
            "warning: unresolved wikilinks={}",
            report.unresolved_wikilinks.len()
        );
        for name in &report.unresolved_wikilinks {
            eprintln!("  [[{name}]]");
        }
    }
    println!("documents={}", report.imported.len());
    println!("vault_index={}", report.vault_index.display());
    Ok(())
}

fn run_vault_members(root: &PathBuf, json: bool) -> Result<(), TesError> {
    let members = load_registered_members(root)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&members)?);
    } else if members.is_empty() {
        println!("members=0");
    } else {
        for member in &members {
            println!("{}\t{}", member.kind.as_str(), member.path);
        }
        println!("members={}", members.len());
    }
    Ok(())
}

fn run_vault_search(root: &PathBuf, args: &VaultSearchArgs) -> Result<(), TesError> {
    let query = args
        .query
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(query) = query else {
        return rebuild_only(root, args);
    };
    let report = search_vault(root, query, search_options(args))?;
    print_search_source(root, &report);
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_search_tsv(&report.hits);
        println!("hits={}", report.hits.len());
    }
    Ok(())
}

fn rebuild_only(root: &PathBuf, args: &VaultSearchArgs) -> Result<(), TesError> {
    if !args.rebuild && !args.force_rebuild {
        return Err(TesError::VaultFts {
            message: "search requires a query, or --rebuild / --force-rebuild".into(),
        });
    }
    let path = rebuild_vault_fts(root)?;
    if args.json {
        println!(
            "{}",
            serde_json::json!({
                "rebuilt": true,
                "mode": "index",
                "path": path,
            })
        );
    } else {
        println!("wrote\t{}", path.display());
    }
    Ok(())
}

fn search_options(args: &VaultSearchArgs) -> VaultSearchOptions {
    VaultSearchOptions {
        limit: args.limit,
        force_rebuild: args.force_rebuild || args.rebuild,
        force_index: args.index || args.rebuild || args.force_rebuild,
        force_scan: args.scan,
    }
}

fn print_search_source(root: &PathBuf, report: &crate::vault::VaultSearchReport) {
    match report.mode {
        VaultSearchMode::Scan => eprintln!("source=scan documents={}", report.documents),
        VaultSearchMode::Index if report.was_stale && report.rebuilt => {
            eprintln!(
                "warning: {} was stale; rebuilt Tantivy index",
                vault_fts_path(root).display()
            );
        }
        VaultSearchMode::Index if report.rebuilt => {
            eprintln!("source=rebuilt {}", vault_fts_path(root).display());
        }
        VaultSearchMode::Index => {
            eprintln!("source={}", vault_fts_path(root).display());
        }
    }
}

fn print_search_tsv(hits: &[VaultSearchHit]) {
    for hit in hits {
        println!(
            "{}\t{}\t{}\t{}",
            &hit.doc_id[..hit.doc_id.len().min(8)],
            hit.title,
            hit.path,
            hit.snippet.replace(['\t', '\n'], " ")
        );
    }
}

fn print_list_source(root: &PathBuf, report: &crate::vault::VaultListReport) {
    if report.index_stale {
        eprintln!(
            "warning: {} is stale; listing from catalog scan",
            vault_index_path(root).display()
        );
    } else if report.used_index {
        eprintln!("source=vault.tes");
    } else {
        eprintln!("source=catalog-scan");
    }
}

/// Machine-friendly rows (also the non-TTY default).
fn print_list_tsv(entries: &[VaultIndexEntry]) {
    for entry in entries {
        let mut extras = String::new();
        append_tsv_opt(&mut extras, "category", entry.category.as_deref());
        append_tsv_opt(&mut extras, "section", entry.section.as_deref());
        if !entry.tags.is_empty() {
            let _ = write!(extras, "\ttags={}", entry.tags.join(","));
        }
        append_tsv_opt(&mut extras, "slug", entry.slug.as_deref());
        println!(
            "{}\t{}\t{}\t{}{extras}",
            entry.doc_id, entry.title, entry.doc_kind, entry.path
        );
    }
}

fn append_tsv_opt(extras: &mut String, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        let _ = write!(extras, "\t{key}={value}");
    }
}

const LIST_TABLE_HEADERS: [&str; 7] = [
    "KIND", "TITLE", "PATH", "CATEGORY", "SECTION", "SLUG", "DOC_ID",
];

/// Aligned columns for interactive terminals.
fn print_list_table(entries: &[VaultIndexEntry]) {
    if entries.is_empty() {
        return;
    }

    let rows: Vec<[String; 7]> = entries
        .iter()
        .map(|e| {
            [
                e.doc_kind.clone(),
                e.title.clone(),
                e.path.clone(),
                e.category.clone().unwrap_or_default(),
                e.section.clone().unwrap_or_default(),
                e.slug.clone().unwrap_or_default(),
                short_doc_id(&e.doc_id),
            ]
        })
        .collect();

    let mut widths = LIST_TABLE_HEADERS.map(|h| h.chars().count());
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }

    println!("{}", format_table_row(LIST_TABLE_HEADERS, &widths));
    let separators = widths.map(|w| "-".repeat(w));
    println!(
        "{}",
        format_table_row(separators.each_ref().map(String::as_str), &widths)
    );
    for row in &rows {
        println!(
            "{}",
            format_table_row(row.each_ref().map(String::as_str), &widths)
        );
    }
}

fn format_table_row(cells: [&str; 7], widths: &[usize; 7]) -> String {
    cells
        .iter()
        .zip(widths.iter())
        .map(|(cell, width)| format!("{cell:<width$}"))
        .collect::<Vec<_>>()
        .join("  ")
}

fn short_doc_id(doc_id: &str) -> String {
    doc_id.get(..8).unwrap_or(doc_id).to_owned()
}
