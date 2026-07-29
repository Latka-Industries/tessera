//! `tes vault` (rebuild / list / membership for optional `vault.tes` TOC).

use std::path::PathBuf;

use crate::error::TesError;
use crate::vault::{
    list_vault_documents, load_registered_members, rebuild_vault_index, register_member,
    unregister_member, vault_index_path,
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
            force_scan,
            json,
        } => {
            let report = list_vault_documents(root, tag.as_deref(), force_scan)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_list_source(root, &report);
                for entry in &report.entries {
                    let tags = if entry.tags.is_empty() {
                        String::new()
                    } else {
                        format!("\ttags={}", entry.tags.join(","))
                    };
                    println!(
                        "{}\t{}\t{}\t{}{tags}",
                        entry.doc_id, entry.title, entry.doc_kind, entry.path
                    );
                }
                println!("documents={}", report.entries.len());
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
