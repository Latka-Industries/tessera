//! `tes vault` (rebuild / list / import / membership for optional `vault.tes` TOC).

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::error::TesError;
use crate::vault::{
    VaultMarkdownImportOptions, import_markdown_vault, list_vault_documents_filtered,
    load_registered_members, rebuild_vault_index, register_member, unregister_member,
    vault_index_path,
};

use super::super::args::VaultCommands;

pub(in crate::cli) fn run_vault(root: &PathBuf, command: VaultCommands) -> Result<(), TesError> {
    match command {
        VaultCommands::Rebuild => {
            let path = rebuild_vault_index(root)?;
            println!("wrote\t{}", path.display());
        }
        VaultCommands::List {
            tag,
            category,
            force_scan,
            json,
        } => {
            let report = list_vault_documents_filtered(
                root,
                tag.as_deref(),
                category.as_deref(),
                force_scan,
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_list_source(root, &report);
                for entry in &report.entries {
                    let mut extras = String::new();
                    if let Some(category) = &entry.category {
                        let _ = write!(extras, "\tcategory={category}");
                    }
                    if !entry.tags.is_empty() {
                        let _ = write!(extras, "\ttags={}", entry.tags.join(","));
                    }
                    if let Some(slug) = &entry.slug {
                        let _ = write!(extras, "\tslug={slug}");
                    }
                    println!(
                        "{}\t{}\t{}\t{}{extras}",
                        entry.doc_id, entry.title, entry.doc_kind, entry.path
                    );
                }
                println!("documents={}", report.entries.len());
            }
        }
        VaultCommands::Import { source, json } => {
            let report =
                import_markdown_vault(&source, root, &VaultMarkdownImportOptions::default())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
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
            }
        }
        VaultCommands::Add { path } => {
            let member = register_member(root, &path)?;
            println!("added\t{}\t{}", member.kind.as_str(), member.path);
        }
        VaultCommands::Remove { path } => {
            unregister_member(root, &path)?;
            println!("removed\t{}", path.display());
        }
        VaultCommands::Members { json } => {
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
        }
    }
    Ok(())
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
