//! `tes` — command-line interface for the Tessera document format.
//!
//! See `docs/cli.md` for the full command surface. This binary ships `info`,
//! `verify`, `export` (including `--pdf`), CommonMark/HTML `import`,
//! vault-aware `link`, loopback `serve` preview, and Tessera Markdown
//! `edit-read` / `edit-write` / `apply` mutation.

use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgGroup, Args, Parser, Subcommand};
use tessera_doc::bib::{BibFormat, BibImportOptions, export_bibliography, import_bibliography};
use tessera_doc::catalog::{
    format_info_human, format_info_json, format_info_quiet, read_summary_v0,
};
use tessera_doc::edit::{
    EditWriteOptions, apply_ops, apply_patch, edit_read, edit_write, parse_ops_json,
};
use tessera_doc::error::TesError;
use tessera_doc::export::{ExportOptions, ExportView, export_view};
use tessera_doc::import::{
    HtmlImportOptions, MarkdownImportOptions, import_html_v0, import_markdown_v0,
};
use tessera_doc::layout::DocKind;
use tessera_doc::pdf::{PdfExportOptions, export_pdf};
use tessera_doc::preview::{ServeOptions, serve_preview};
use tessera_doc::vault::{Vault, parse_target};
use tessera_doc::verify::{
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
    Export(ExportArgs),

    /// Import a foreign document into a sealed .tes file
    Import(ImportArgs),

    /// Resolve and inspect links across a vault directory
    Link {
        /// Vault root containing .tes files
        #[arg(long)]
        vault: PathBuf,
        #[command(subcommand)]
        command: LinkCommands,
    },

    /// Live browser preview on loopback (semantic HTML + template theme)
    Serve(ServeArgs),

    /// Decode a .tes file to Tessera Markdown (Tessprek) for editors
    EditRead(EditReadArgs),

    /// Compile Tessera Markdown and atomically replace a .tes file
    EditWrite(EditWriteArgs),

    /// Apply Tessera Markdown patch or typed JSON ops through the mutation gate
    Apply(ApplyArgs),
}

/// Flags for `tes export`.
#[derive(Debug, Args)]
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
            "pdf",
            "bibliography",
        ])
))]
struct ExportArgs {
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
    /// Print-theme PDF via headless Chromium (requires -o)
    #[arg(long)]
    pdf: bool,
    /// BibTeX or CSL-JSON bibliography from cite chunks
    #[arg(long)]
    bibliography: bool,
    /// Bibliography format: bibtex | csl-json (default: bibtex)
    #[arg(
        long = "bib-format",
        default_value = "bibtex",
        requires = "bibliography"
    )]
    bib_format: String,
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
    /// Write to PATH instead of stdout (required for --pdf)
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
    /// Template pack id for --pdf (default: catalog or minimal)
    #[arg(long, requires = "pdf")]
    template: Option<String>,
    /// Template pack root for --pdf (env: `TES_TEMPLATE_ROOT`)
    #[arg(long = "template-root", requires = "pdf")]
    template_root: Option<PathBuf>,
    /// Pack theme id for --pdf (default: print)
    #[arg(long = "theme-id", requires = "pdf")]
    theme_id: Option<String>,
}

/// Flags for `tes import`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("import_format")
        .required(true)
        .args(["markdown", "html", "bibtex", "csl_json"])
))]
struct ImportArgs {
    /// Parse input as the supported `CommonMark` subset
    #[arg(long)]
    markdown: bool,
    /// Parse semantic HTML
    #[arg(long)]
    html: bool,
    /// Import BibTeX into research cite chunks
    #[arg(long)]
    bibtex: bool,
    /// Import CSL-JSON into research cite chunks
    #[arg(long = "csl-json")]
    csl_json: bool,
    /// Source document
    input: PathBuf,
    /// Destination .tes (must not already exist)
    output: PathBuf,
    /// `note|document|manuscript|research|deck|wiki_page|hub|index`
    #[arg(long, default_value = "document")]
    doc_kind: String,
    /// Override catalog title
    #[arg(long)]
    title: Option<String>,
    /// Stable UUID (generated when omitted)
    #[arg(long)]
    doc_id: Option<String>,
}

/// Flags for `tes serve`.
#[derive(Debug, Args)]
struct ServeArgs {
    /// Path to a .tes file
    path: PathBuf,
    /// Template pack id under --template-root (default: catalog or minimal)
    #[arg(long)]
    template: Option<String>,
    /// Directory containing template packs (env: `TES_TEMPLATE_ROOT`)
    #[arg(long = "template-root")]
    template_root: Option<PathBuf>,
    /// Theme id from the pack: draft or print (default: catalog or draft)
    #[arg(long)]
    theme: Option<String>,
    /// Loopback host (127.0.0.1, localhost, or `::1`)
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
    /// Allow packs that declare `requires_theme_js` (still CSS-only serving)
    #[arg(long = "allow-theme-js")]
    allow_theme_js: bool,
}

/// Flags for `tes edit-read`.
#[derive(Debug, Args)]
struct EditReadArgs {
    /// Path to a .tes file
    path: PathBuf,
    /// Projection format (only `tessprek` today)
    #[arg(long, default_value = "tessprek")]
    format: String,
    /// Write Tessera Markdown to PATH instead of stdout
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
}

/// Flags for `tes edit-write`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("edit_input")
        .required(true)
        .args(["stdin", "input"])
))]
struct EditWriteArgs {
    /// Path to the .tes file to replace
    path: PathBuf,
    /// Projection format (only `tessprek` today)
    #[arg(long, default_value = "tessprek")]
    format: String,
    /// Expected SHA-256 of the current on-disk file
    #[arg(long = "source-hash", required = true)]
    source_hash: String,
    /// Read Tessera Markdown from stdin
    #[arg(long)]
    stdin: bool,
    /// Tessera Markdown input file (alternative to --stdin)
    #[arg(short = 'i', long = "input")]
    input: Option<PathBuf>,
    /// Compile and verify without replacing
    #[arg(long)]
    dry_run: bool,
}

/// Flags for `tes apply`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("apply_source")
        .required(true)
        .args(["ops", "patch"])
))]
struct ApplyArgs {
    /// Path to the .tes file to mutate
    path: PathBuf,
    /// Expected SHA-256 of the current on-disk file
    #[arg(long = "source-hash", required = true)]
    source_hash: String,
    /// JSON array of typed TesOp mutations
    #[arg(long)]
    ops: Option<PathBuf>,
    /// Full Tessera Markdown replacement patch
    #[arg(long)]
    patch: Option<PathBuf>,
    /// Compile and verify without replacing; print a line diff
    #[arg(long)]
    dry_run: bool,
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
    match Cli::parse().command {
        Commands::Info { path, json, quiet } => result_exit(run_info(&path, json, quiet)),
        Commands::Verify {
            paths,
            deep,
            json,
            quiet,
        } => run_verify(&paths, deep, json, quiet),
        Commands::Export(args) => result_exit(run_export(args)),
        Commands::Import(args) => result_exit(run_import(args)),
        Commands::Link { vault, command } => run_link(&vault, command),
        Commands::Serve(args) => result_exit(run_serve(args)),
        Commands::EditRead(args) => result_exit(run_edit_read(args)),
        Commands::EditWrite(args) => result_exit(run_edit_write(args)),
        Commands::Apply(args) => result_exit(run_apply(args)),
    }
}

fn result_exit(result: Result<(), TesError>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            exit_for(&err)
        }
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

fn run_export(args: ExportArgs) -> Result<(), TesError> {
    if args.bibliography {
        let format = BibFormat::parse(&args.bib_format)?;
        let out = export_bibliography(&args.path, format)?;
        if let Some(path) = args.output.as_ref() {
            fs::write(path, out.as_bytes())?;
        } else {
            print_out(&out)?;
        }
        return Ok(());
    }

    if args.pdf {
        let Some(out_path) = args.output.as_ref() else {
            return Err(TesError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--pdf requires -o/--output PATH",
            )));
        };
        let template_root = args
            .template_root
            .or_else(|| env::var_os("TES_TEMPLATE_ROOT").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("templates"));
        return export_pdf(
            &args.path,
            out_path,
            &PdfExportOptions {
                template_root,
                template_id: args.template,
                theme_id: args.theme_id.or_else(|| Some("print".into())),
                chrome_path: env::var_os("TES_CHROME").map(PathBuf::from),
            },
        );
    }

    let view = if args.raw {
        ExportView::Raw
    } else if args.linear {
        ExportView::Linear
    } else if args.ai_text {
        ExportView::AiText
    } else if args.chunks_jsonl {
        ExportView::ChunksJsonl
    } else if args.markdown {
        ExportView::Markdown
    } else if args.html {
        ExportView::Html
    } else {
        return Err(TesError::ExportViewRequired);
    };

    let embedded_css = if args.embed_css {
        args.theme.as_ref().map(fs::read_to_string).transpose()?
    } else {
        None
    };
    let options = ExportOptions {
        chunk_id: args.chunk,
        include_headers: args.include_headers,
        annotate: args.annotate,
        all_types: args.all_types,
        no_cites: args.no_cites,
        theme_href: if args.embed_css {
            None
        } else {
            args.theme.as_ref().map(|path| path.display().to_string())
        },
        standalone: args.standalone,
        embedded_css,
        media_url_prefix: None,
    };
    let out = export_view(&args.path, view, &options)?;
    if let Some(path) = args.output.as_ref() {
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

fn run_import(args: ImportArgs) -> Result<(), TesError> {
    if args.bibtex || args.csl_json {
        let format = if args.bibtex {
            BibFormat::Bibtex
        } else {
            BibFormat::CslJson
        };
        // Clap default is `document`; bibliography imports prefer research.
        let kind = if args.doc_kind == "document" {
            DocKind::Research
        } else {
            parse_doc_kind(&args.doc_kind)?
        };
        import_bibliography(
            &args.input,
            &args.output,
            format,
            &BibImportOptions {
                doc_kind: kind,
                title: args.title,
                doc_id: args.doc_id,
                cite_style_id: Some("numeric".into()),
            },
        )?;
        let summary = read_summary_v0(&args.output)?;
        let doc_id = summary.catalog.as_ref().map_or("", |c| c.doc_id.as_str());
        println!(
            "imported {}\tchunks={}\tdoc_id={}",
            args.output.display(),
            summary.chunks.len(),
            doc_id
        );
        return Ok(());
    }

    let doc_kind = parse_doc_kind(&args.doc_kind)?;
    let (chunk_count, report_doc_id) = if args.markdown {
        let report = import_markdown_v0(
            &args.input,
            &args.output,
            &MarkdownImportOptions {
                doc_kind,
                title: args.title,
                doc_id: args.doc_id,
            },
        )?;
        (report.chunk_count, report.doc_id)
    } else if args.html {
        let report = import_html_v0(
            &args.input,
            &args.output,
            &HtmlImportOptions {
                doc_kind,
                title: args.title,
                doc_id: args.doc_id,
            },
        )?;
        (report.chunk_count, report.doc_id)
    } else {
        return Err(TesError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "select --markdown, --html, --bibtex, or --csl-json",
        )));
    };
    println!(
        "imported {}\tchunks={}\tdoc_id={}",
        args.output.display(),
        chunk_count,
        report_doc_id
    );
    Ok(())
}

fn run_serve(args: ServeArgs) -> Result<(), TesError> {
    let template_root = args
        .template_root
        .or_else(|| env::var_os("TES_TEMPLATE_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("templates"));
    let options = ServeOptions {
        path: args.path,
        template_root,
        template_id: args.template,
        theme_id: args.theme,
        host: args.host,
        port: args.port,
        watch: args.watch,
        watch_secs: args.watch_secs,
        allow_theme_js: args.allow_theme_js,
    };
    serve_preview(&options, None)
}

fn run_edit_read(args: EditReadArgs) -> Result<(), TesError> {
    require_tessprek(&args.format)?;
    let report = edit_read(&args.path)?;
    eprintln!("source-hash={}", report.source_hash);
    if let Some(path) = args.output.as_ref() {
        fs::write(path, report.tessprek.as_bytes())?;
    } else {
        print_out(&report.tessprek)?;
    }
    Ok(())
}

fn run_edit_write(args: EditWriteArgs) -> Result<(), TesError> {
    require_tessprek(&args.format)?;
    let tessprek = read_edit_input(args.stdin, args.input.as_ref())?;
    let report = edit_write(
        &args.path,
        &tessprek,
        &EditWriteOptions {
            source_hash: args.source_hash,
            dry_run: args.dry_run,
        },
    )?;
    print_edit_write_report(&report);
    Ok(())
}

fn run_apply(args: ApplyArgs) -> Result<(), TesError> {
    let options = EditWriteOptions {
        source_hash: args.source_hash,
        dry_run: args.dry_run,
    };
    let report = if let Some(ops_path) = args.ops.as_ref() {
        let json = fs::read_to_string(ops_path)?;
        let ops = parse_ops_json(&json)?;
        apply_ops(&args.path, &ops, &options)?
    } else if let Some(patch_path) = args.patch.as_ref() {
        let patch = fs::read_to_string(patch_path)?;
        apply_patch(&args.path, &patch, &options)?
    } else {
        return Err(TesError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "select --ops or --patch",
        )));
    };
    print_edit_write_report(&report);
    Ok(())
}

fn require_tessprek(format: &str) -> Result<(), TesError> {
    if format == "tessprek" || format == "tessera-markdown" {
        Ok(())
    } else {
        Err(TesError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported edit format '{format}' (use tessprek)"),
        )))
    }
}

fn read_edit_input(stdin: bool, input: Option<&PathBuf>) -> Result<String, TesError> {
    if stdin {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else if let Some(path) = input {
        Ok(fs::read_to_string(path)?)
    } else {
        Err(TesError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provide --stdin or -i/--input",
        )))
    }
}

fn print_edit_write_report(report: &tessera_doc::edit::EditWriteReport) {
    if report.replaced {
        if let Some(hash) = report.new_source_hash.as_ref() {
            eprintln!("replaced\tnew-source-hash={hash}");
        } else {
            eprintln!("replaced");
        }
    } else {
        eprintln!("dry-run\t(no replace)");
        print!("{}", report.diff);
    }
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
        TesError::Io(_) | TesError::PdfEngine { .. } => ExitCode::from(2),
        _ => ExitCode::from(1),
    }
}
