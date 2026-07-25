//! `tes` — command-line interface for the Tessera document format.
//!
//! See `docs/cli.md` for the full command surface. This binary ships `info`,
//! `verify`, `export`, CommonMark/HTML `import`, vault-aware `link`, and
//! loopback `serve` preview.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgGroup, Parser, Subcommand};
use tessera::catalog::{format_info_human, format_info_json, format_info_quiet, read_summary_v0};
use tessera::error::TesError;
use tessera::export::{ExportOptions, ExportView, export_view};
use tessera::import::{
    HtmlImportOptions, MarkdownImportOptions, import_html_v0, import_markdown_v0,
};
use tessera::layout::DocKind;
use tessera::preview::{ServeOptions, serve_preview};
use tessera::vault::{Vault, parse_target};
use tessera::verify::{
    format_verify_human, format_verify_json, format_verify_quiet, verify_tes_file,
};

#[derive(Debug, Parser)]
#[command(
    name = "tes",
    version,
    about = "Tessera document format CLI (.tes)",
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Summarize a .tes file (catalog + chunk table)
    Info {
        /// Path to a .tes file
        path: PathBuf,
        /// Emit full JSON (superblock, catalog, index rows)
        #[arg(long)]
        json: bool,
        /// One line: title\tchunks=N\tbytes=M
        #[arg(short, long)]
        quiet: bool,
    },

    /// Validate on-disk layout (exit 1 on failure)
    Verify {
        /// Paths to .tes files
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Decode every payload (codec + UTF-8 validation)
        #[arg(long)]
        deep: bool,
        /// Machine-readable report
        #[arg(long)]
        json: bool,
        /// One line per file: status=ok or status=failed
        #[arg(short, long)]
        quiet: bool,
    },

    /// Write a decoded view to stdout or -o PATH
    #[command(group(
        ArgGroup::new("view")
            .required(true)
            .args([
                "raw",
                "linear",
                "ai_text",
                "chunks_jsonl",
                "markdown",
                "html",
            ])
    ))]
    Export {
        /// Path to a .tes file
        path: PathBuf,
        /// Concatenate text chunk bodies
        #[arg(long)]
        raw: bool,
        /// Reading-order prose with light structure markers
        #[arg(long)]
        linear: bool,
        /// LLM-oriented plain text (no exporter markup)
        #[arg(long = "ai-text")]
        ai_text: bool,
        /// One JSON object per reading-order chunk
        #[arg(long = "chunks-jsonl")]
        chunks_jsonl: bool,
        /// Lossy GFM-ish Markdown
        #[arg(long)]
        markdown: bool,
        /// Semantic HTML5 fragment or standalone page
        #[arg(long)]
        html: bool,
        /// Restrict to a single chunk id
        #[arg(long = "chunk")]
        chunk: Option<u64>,
        /// Prefix each --raw chunk with a debug header
        #[arg(long = "include-headers")]
        include_headers: bool,
        /// Prefix each --ai-text chunk with <!-- chunk:N -->
        #[arg(long)]
        annotate: bool,
        /// Include non-text rows in --chunks-jsonl
        #[arg(long = "all-types")]
        all_types: bool,
        /// Omit cite expansion from --ai-text
        #[arg(long = "no-cites")]
        no_cites: bool,
        /// Write to PATH instead of stdout
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
        /// Stylesheet path/href for --html
        #[arg(long)]
        theme: Option<PathBuf>,
        /// Emit a complete HTML document
        #[arg(long)]
        standalone: bool,
        /// Read --theme and embed its CSS
        #[arg(long, requires = "theme")]
        embed_css: bool,
    },

    /// Import a foreign document into a sealed .tes file
    #[command(group(
        ArgGroup::new("import_format")
            .required(true)
            .args(["markdown", "html"])
    ))]
    Import {
        /// Parse input as the supported CommonMark subset
        #[arg(long)]
        markdown: bool,
        /// Parse semantic HTML
        #[arg(long)]
        html: bool,
        /// Source document
        input: PathBuf,
        /// Destination .tes (must not already exist)
        output: PathBuf,
        /// note|document|manuscript|research|deck|wiki_page|hub|index
        #[arg(long, default_value = "document")]
        doc_kind: String,
        /// Override catalog title
        #[arg(long)]
        title: Option<String>,
        /// Stable UUID (generated when omitted)
        #[arg(long)]
        doc_id: Option<String>,
    },

    /// Resolve and inspect links across a vault directory
    Link {
        /// Vault root containing .tes files
        #[arg(long)]
        vault: PathBuf,
        #[command(subcommand)]
        command: LinkCommands,
    },

    /// Live browser preview on loopback (semantic HTML + template theme)
    Serve {
        /// Path to a .tes file
        path: PathBuf,
        /// Template pack id under --template-root (default: catalog or minimal)
        #[arg(long)]
        template: Option<String>,
        /// Directory containing template packs (env: TES_TEMPLATE_ROOT)
        #[arg(long = "template-root")]
        template_root: Option<PathBuf>,
        /// Theme id from the pack: draft or print (default: catalog or draft)
        #[arg(long)]
        theme: Option<String>,
        /// Loopback host (127.0.0.1, localhost, or ::1)
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port (0 = ephemeral)
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// Inject meta-refresh so the browser reloads while editing
        #[arg(long)]
        watch: bool,
        /// Meta-refresh interval in seconds
        #[arg(long = "watch-secs", default_value_t = 2)]
        watch_secs: u64,
        /// Allow packs that declare requires_theme_js (still CSS-only serving)
        #[arg(long = "allow-theme-js")]
        allow_theme_js: bool,
    },
}

#[derive(Debug, Subcommand)]
enum LinkCommands {
    /// Resolve UUID[/chunk] to a document and optional text body
    Resolve {
        /// UUID or UUID/chunk
        target: String,
        /// Emit JSON
        #[arg(long)]
        json: bool,
    },
    /// List documents linking to UUID
    Backlinks {
        /// Target document UUID
        doc_id: String,
        /// Emit JSON
        #[arg(long)]
        json: bool,
    },
    /// Check that every graph target exists
    Check {
        /// Emit JSON
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Info { path, json, quiet } => match run_info(&path, json, quiet) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                exit_for(&err)
            }
        },
        Commands::Verify {
            paths,
            deep,
            json,
            quiet,
        } => run_verify(&paths, deep, json, quiet),
        Commands::Export {
            path,
            raw,
            linear,
            ai_text,
            chunks_jsonl,
            markdown,
            html,
            chunk,
            include_headers,
            annotate,
            all_types,
            no_cites,
            output,
            theme,
            standalone,
            embed_css,
        } => match run_export(
            &path,
            raw,
            linear,
            ai_text,
            chunks_jsonl,
            markdown,
            html,
            chunk,
            include_headers,
            annotate,
            all_types,
            no_cites,
            output.as_ref(),
            theme.as_ref(),
            standalone,
            embed_css,
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                exit_for(&err)
            }
        },
        Commands::Import {
            markdown,
            html,
            input,
            output,
            doc_kind,
            title,
            doc_id,
        } => match run_import(markdown, html, &input, &output, &doc_kind, title, doc_id) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                exit_for(&err)
            }
        },
        Commands::Link { vault, command } => run_link(&vault, command),
        Commands::Serve {
            path,
            template,
            template_root,
            theme,
            host,
            port,
            watch,
            watch_secs,
            allow_theme_js,
        } => match run_serve(
            path,
            template,
            template_root,
            theme,
            host,
            port,
            watch,
            watch_secs,
            allow_theme_js,
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                exit_for(&err)
            }
        },
    }
}

fn run_info(path: &PathBuf, json: bool, quiet: bool) -> Result<(), TesError> {
    let report = read_summary_v0(path)?;
    let out = if json {
        format_info_json(&report)?
    } else if quiet {
        format_info_quiet(&report)
    } else {
        format_info_human(&report)
    };
    print_out(&out)?;
    Ok(())
}

fn run_verify(paths: &[PathBuf], deep: bool, json: bool, quiet: bool) -> ExitCode {
    let mut failed = false;
    for path in paths {
        match verify_tes_file(path, deep) {
            Ok(report) => {
                if !report.ok {
                    failed = true;
                }
                let out = if json {
                    match format_verify_json(&report) {
                        Ok(s) => s,
                        Err(err) => {
                            eprintln!("error: {err}");
                            return ExitCode::from(2);
                        }
                    }
                } else if quiet {
                    format!("{}\t{}", path.display(), format_verify_quiet(&report))
                } else {
                    format_verify_human(&report)
                };
                println!("{}", out.trim_end());
            }
            Err(err) => {
                eprintln!("error: {}: {err}", path.display());
                return exit_for(&err);
            }
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#[allow(clippy::too_many_arguments)]
fn run_export(
    path: &PathBuf,
    raw: bool,
    linear: bool,
    ai_text: bool,
    chunks_jsonl: bool,
    markdown: bool,
    html: bool,
    chunk: Option<u64>,
    include_headers: bool,
    annotate: bool,
    all_types: bool,
    no_cites: bool,
    output: Option<&PathBuf>,
    theme: Option<&PathBuf>,
    standalone: bool,
    embed_css: bool,
) -> Result<(), TesError> {
    let view = if raw {
        ExportView::Raw
    } else if linear {
        ExportView::Linear
    } else if ai_text {
        ExportView::AiText
    } else if chunks_jsonl {
        ExportView::ChunksJsonl
    } else if markdown {
        ExportView::Markdown
    } else if html {
        ExportView::Html
    } else {
        return Err(TesError::ExportViewRequired);
    };

    let embedded_css = if embed_css {
        theme.map(fs::read_to_string).transpose()?
    } else {
        None
    };
    let options = ExportOptions {
        chunk_id: chunk,
        include_headers,
        annotate,
        all_types,
        no_cites,
        theme_href: if embed_css {
            None
        } else {
            theme.map(|path| path.display().to_string())
        },
        standalone,
        embedded_css,
    };
    let out = export_view(path, view, &options)?;
    if let Some(path) = output {
        fs::write(path, out.as_bytes())?;
    } else {
        print_out(&out)?;
    }
    Ok(())
}

fn print_out(out: &str) -> Result<(), TesError> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(out.as_bytes())?;
    if !out.is_empty() && !out.ends_with('\n') {
        stdout.write_all(b"\n")?;
    }
    Ok(())
}

fn run_import(
    markdown: bool,
    html: bool,
    input: &PathBuf,
    output: &PathBuf,
    doc_kind: &str,
    title: Option<String>,
    doc_id: Option<String>,
) -> Result<(), TesError> {
    let doc_kind = parse_doc_kind(doc_kind)?;
    let (chunk_count, report_doc_id) = if markdown {
        let report = import_markdown_v0(
            input,
            output,
            &MarkdownImportOptions {
                doc_kind,
                title,
                doc_id,
            },
        )?;
        (report.chunk_count, report.doc_id)
    } else if html {
        let report = import_html_v0(
            input,
            output,
            &HtmlImportOptions {
                doc_kind,
                title,
                doc_id,
            },
        )?;
        (report.chunk_count, report.doc_id)
    } else {
        return Err(TesError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "select --markdown or --html",
        )));
    };
    println!(
        "imported {}\tchunks={}\tdoc_id={}",
        output.display(),
        chunk_count,
        report_doc_id
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_serve(
    path: PathBuf,
    template: Option<String>,
    template_root: Option<PathBuf>,
    theme: Option<String>,
    host: String,
    port: u16,
    watch: bool,
    watch_secs: u64,
    allow_theme_js: bool,
) -> Result<(), TesError> {
    let template_root = template_root
        .or_else(|| env::var_os("TES_TEMPLATE_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("templates"));
    let options = ServeOptions {
        path,
        template_root,
        template_id: template,
        theme_id: theme,
        host,
        port,
        watch,
        watch_secs,
        allow_theme_js,
    };
    serve_preview(options, None)
}

fn parse_doc_kind(value: &str) -> Result<DocKind, TesError> {
    match value {
        "note" => Ok(DocKind::Note),
        "document" => Ok(DocKind::Document),
        "manuscript" => Ok(DocKind::Manuscript),
        "research" => Ok(DocKind::Research),
        "deck" => Ok(DocKind::Deck),
        "wiki_page" => Ok(DocKind::WikiPage),
        "hub" => Ok(DocKind::Hub),
        "index" => Ok(DocKind::Index),
        _ => Err(TesError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown doc kind '{value}'"),
        ))),
    }
}

fn run_link(root: &PathBuf, command: LinkCommands) -> ExitCode {
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

fn exit_for(err: &TesError) -> ExitCode {
    match err {
        TesError::Io(_) => ExitCode::from(2),
        _ => ExitCode::from(1),
    }
}
