//! `tes link` (resolve, backlinks, check).

use std::path::PathBuf;
use std::process::ExitCode;

use crate::error::TesError;
use crate::vault::{Vault, parse_target};

use super::super::args::LinkCommands;
use super::super::util::exit_for;

pub(in crate::cli) fn run_link(root: &PathBuf, command: LinkCommands) -> ExitCode {
    let result = (|| -> Result<bool, TesError> {
        let vault = Vault::open(root)?;
        match command {
            LinkCommands::Resolve { target, json } => {
                let (doc_id, chunk_id) = parse_target(&target)?;
                let resolved = vault.resolve(doc_id, chunk_id)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&resolved)?);
                } else {
                    println!(
                        "{}\t{}\t{}",
                        resolved.document.doc_id,
                        resolved.document.title,
                        resolved.document.path.display()
                    );
                    if let Some(text) = resolved.text {
                        println!("{text}");
                    }
                }
                Ok(true)
            }
            LinkCommands::Backlinks { doc_id, json } => {
                let (doc_id, _) = parse_target(&doc_id)?;
                let backlinks = vault.backlinks(doc_id);
                if json {
                    println!("{}", serde_json::to_string_pretty(&backlinks)?);
                } else {
                    for link in &backlinks {
                        println!(
                            "{}\t{}\tchunk={}",
                            link.source_doc_id, link.source_title, link.source_chunk_id
                        );
                    }
                }
                Ok(true)
            }
            LinkCommands::Check { json } => {
                let broken = vault.check()?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&broken)?);
                } else if broken.is_empty() {
                    println!("status=ok\tdocuments={}", vault.documents().count());
                } else {
                    for link in &broken {
                        println!(
                            "missing\tsource={}/{}\ttarget={}/{}\t{}",
                            link.source_doc_id,
                            link.source_chunk_id,
                            link.target_doc_id,
                            link.target_chunk_id,
                            link.message
                        );
                    }
                    println!("status=failed\tbroken={}", broken.len());
                }
                Ok(broken.is_empty())
            }
        }
    })();

    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(err) => {
            eprintln!("error: {err}");
            exit_for(&err)
        }
    }
}
