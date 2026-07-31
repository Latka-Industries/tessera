//! Clap argument types for the `tes` CLI (`docs/cli.md`).

use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "tes",
    version,
    about = "Tessera document format CLI (.tes)",
    propagate_version = true
)]
pub(super) struct Cli {
    #[command(subcommand)]
    pub(super) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(super) enum Commands {
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

    /// Vault catalog (`vault.tes` TOC) rebuild / list
    Vault {
        /// Vault root containing .tes files
        #[arg(long)]
        vault: PathBuf,
        #[command(subcommand)]
        command: VaultCommands,
    },

    /// Live browser preview on loopback (semantic HTML + template theme)
    Serve(ServeArgs),

    /// Decode a .tes file to Tessera Markdown (Tessprek) for editors
    EditRead(EditReadArgs),

    /// Normalize Tessprek: infer roles / split blocks from Markdown shape
    Format(FormatArgs),

    /// Compile Tessera Markdown and atomically replace a .tes file
    EditWrite(EditWriteArgs),

    /// Apply Tessera Markdown patch or typed JSON ops through the mutation gate
    Apply(ApplyArgs),

    /// Snapshot the sealed body into a content-addressed history revision
    Save(SaveArgs),

    /// List revisions / drafts from the THST footer
    Log(LogArgs),

    /// Structural diff between two revisions or draft names
    Diff(DiffArgs),

    /// Changelog summary between two revisions or draft names
    Changelog(ChangelogArgs),

    /// Materialize a revision to a new .tes file
    #[command(name = "export-revs")]
    ExportRevs(ExportRevsArgs),

    /// Replace the live sealed body with a revision (keep current THST)
    Checkout(CheckoutArgs),

    /// Attribute current text to the revision that last introduced it
    Blame(BlameArgs),

    /// Pending-ops redline: suggest / list / redline / accept / reject
    Pending {
        /// Path to a .tes file
        path: PathBuf,
        #[command(subcommand)]
        command: PendingCommands,
    },

    /// Emit Tessprek on stdout for git textconv (no source-hash banner)
    Textconv(TextconvArgs),

    /// Verified 3-way merge for git merge drivers (`%O %A %B`)
    MergeFile(MergeFileArgs),
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
            "attachment",
        ])
))]
#[allow(clippy::struct_excessive_bools)]
pub(super) struct ExportArgs {
    /// Path to a .tes file
    pub(super) path: PathBuf,
    /// Concatenate text chunk bodies
    #[arg(long)]
    pub(super) raw: bool,
    /// Reading-order prose with light structure markers
    #[arg(long)]
    pub(super) linear: bool,
    /// LLM-oriented plain text (no exporter markup)
    #[arg(long = "ai-text")]
    pub(super) ai_text: bool,
    /// One JSON object per reading-order chunk
    #[arg(long = "chunks-jsonl")]
    pub(super) chunks_jsonl: bool,
    /// Lossy GFM-ish Markdown
    #[arg(long)]
    pub(super) markdown: bool,
    /// Semantic HTML5 fragment or standalone page
    #[arg(long)]
    pub(super) html: bool,
    /// Print-theme PDF via headless Chromium (requires -o)
    #[arg(long)]
    pub(super) pdf: bool,
    /// BibTeX or CSL-JSON bibliography from cite chunks
    #[arg(long)]
    pub(super) bibliography: bool,
    /// Write opaque attachment chunk bytes (requires --chunk and -o)
    #[arg(long)]
    pub(super) attachment: bool,
    /// Bibliography format: bibtex | csl-json (default: bibtex)
    #[arg(
        long = "bib-format",
        default_value = "bibtex",
        requires = "bibliography"
    )]
    pub(super) bib_format: String,
    /// Restrict to a single chunk id
    #[arg(long = "chunk", conflicts_with = "chapter")]
    pub(super) chunk: Option<u64>,
    /// Restrict to the Nth chapter (1-based; H1 boundaries)
    #[arg(long = "chapter", conflicts_with = "chunk")]
    pub(super) chapter: Option<u32>,
    /// Prefix each --raw chunk with a debug header
    #[arg(long = "include-headers")]
    pub(super) include_headers: bool,
    /// Prefix each --ai-text chunk with <!-- chunk:N -->
    #[arg(long)]
    pub(super) annotate: bool,
    /// Include non-text rows in --chunks-jsonl
    #[arg(long = "all-types")]
    pub(super) all_types: bool,
    /// Omit cite expansion from --ai-text
    #[arg(long = "no-cites")]
    pub(super) no_cites: bool,
    /// Write to PATH instead of stdout (required for --pdf)
    #[arg(short = 'o', long = "output")]
    pub(super) output: Option<PathBuf>,
    /// Stylesheet path/href for --html
    #[arg(long)]
    pub(super) theme: Option<PathBuf>,
    /// Emit a complete HTML document
    #[arg(long)]
    pub(super) standalone: bool,
    /// Read --theme and embed its CSS
    #[arg(long, requires = "theme")]
    pub(super) embed_css: bool,
    /// Template pack id for --pdf (default: catalog or minimal)
    #[arg(long, requires = "pdf")]
    pub(super) template: Option<String>,
    /// Template pack root for --pdf (env: `TES_TEMPLATE_ROOT`)
    #[arg(long = "template-root", requires = "pdf")]
    pub(super) template_root: Option<PathBuf>,
    /// Pack theme id for --pdf (default: print, or manuscript for `doc_kind=manuscript`)
    #[arg(long = "theme-id", requires = "pdf")]
    pub(super) theme_id: Option<String>,
}

/// Flags for `tes import`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("import_format")
        .required(true)
        .args(["markdown", "html", "bibtex", "csl_json"])
))]
#[allow(clippy::struct_excessive_bools)]
pub(super) struct ImportArgs {
    /// Parse input as the supported `CommonMark` subset
    #[arg(long)]
    pub(super) markdown: bool,
    /// Parse semantic HTML
    #[arg(long)]
    pub(super) html: bool,
    /// Import BibTeX into research cite chunks
    #[arg(long)]
    pub(super) bibtex: bool,
    /// Import CSL-JSON into research cite chunks
    #[arg(long = "csl-json")]
    pub(super) csl_json: bool,
    /// Source document
    pub(super) input: PathBuf,
    /// Destination .tes (must not already exist)
    pub(super) output: PathBuf,
    /// `note|document|manuscript|research|deck|wiki_page|hub|index`
    #[arg(long, default_value = "document")]
    pub(super) doc_kind: String,
    /// Override catalog title
    #[arg(long)]
    pub(super) title: Option<String>,
    /// Stable UUID (generated when omitted)
    #[arg(long)]
    pub(super) doc_id: Option<String>,
}

/// Flags for `tes serve`.
#[derive(Debug, Args)]
pub(super) struct ServeArgs {
    /// Path to a .tes file
    pub(super) path: PathBuf,
    /// Template pack id under --template-root (default: catalog or minimal)
    #[arg(long)]
    pub(super) template: Option<String>,
    /// Directory containing template packs (env: `TES_TEMPLATE_ROOT`)
    #[arg(long = "template-root")]
    pub(super) template_root: Option<PathBuf>,
    /// Theme id from the pack: draft or print (default: catalog or draft)
    #[arg(long)]
    pub(super) theme: Option<String>,
    /// Loopback host (127.0.0.1, localhost, or `::1`)
    #[arg(long, default_value = "127.0.0.1")]
    pub(super) host: String,
    /// Port (0 = ephemeral)
    #[arg(long, default_value_t = 7878)]
    pub(super) port: u16,
    /// Inject meta-refresh so the browser reloads while editing
    #[arg(long)]
    pub(super) watch: bool,
    /// Meta-refresh interval in seconds
    #[arg(long = "watch-secs", default_value_t = 2)]
    pub(super) watch_secs: u64,
    /// Allow packs that declare `requires_theme_js` (still CSS-only serving)
    #[arg(long = "allow-theme-js")]
    pub(super) allow_theme_js: bool,
}

/// Flags for `tes edit-read`.
#[derive(Debug, Args)]
pub(super) struct EditReadArgs {
    /// Path to a .tes file
    pub(super) path: PathBuf,
    /// Projection format (only `tessprek` today)
    #[arg(long, default_value = "tessprek")]
    pub(super) format: String,
    /// Write Tessera Markdown to PATH instead of stdout
    #[arg(short = 'o', long = "output")]
    pub(super) output: Option<PathBuf>,
}

/// Flags for `tes format`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("format_input")
        .required(true)
        .args(["stdin", "input"])
))]
pub(super) struct FormatArgs {
    /// Projection format (only `tessprek` today)
    #[arg(long, default_value = "tessprek")]
    pub(super) format: String,
    /// Read Tessprek from stdin
    #[arg(long)]
    pub(super) stdin: bool,
    /// Read Tessprek from PATH
    #[arg(short = 'i', long = "input")]
    pub(super) input: Option<PathBuf>,
    /// Write normalized Tessprek to PATH instead of stdout
    #[arg(short = 'o', long = "output")]
    pub(super) output: Option<PathBuf>,
    /// Exit 1 if normalization would change the input (no write)
    #[arg(long)]
    pub(super) check: bool,
}

/// Flags for `tes edit-write`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("edit_input")
        .required(true)
        .args(["stdin", "input"])
))]
pub(super) struct EditWriteArgs {
    /// Path to the .tes file to replace
    pub(super) path: PathBuf,
    /// Projection format (only `tessprek` today)
    #[arg(long, default_value = "tessprek")]
    pub(super) format: String,
    /// Expected SHA-256 of the current on-disk file
    #[arg(long = "source-hash", required = true)]
    pub(super) source_hash: String,
    /// Read Tessera Markdown from stdin
    #[arg(long)]
    pub(super) stdin: bool,
    /// Tessera Markdown input file (alternative to --stdin)
    #[arg(short = 'i', long = "input")]
    pub(super) input: Option<PathBuf>,
    /// Compile and verify without replacing
    #[arg(long)]
    pub(super) dry_run: bool,
}

/// Flags for `tes apply`.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("apply_source")
        .required(true)
        .args(["ops", "patch"])
))]
pub(super) struct ApplyArgs {
    /// Path to the .tes file to mutate
    pub(super) path: PathBuf,
    /// Expected SHA-256 of the current on-disk file
    #[arg(long = "source-hash", required = true)]
    pub(super) source_hash: String,
    /// JSON array of typed `TesOp` mutations
    #[arg(long)]
    pub(super) ops: Option<PathBuf>,
    /// Full Tessera Markdown replacement patch
    #[arg(long)]
    pub(super) patch: Option<PathBuf>,
    /// Compile and verify without replacing; print a line diff
    #[arg(long)]
    pub(super) dry_run: bool,
}

/// Flags for `tes save`.
#[derive(Debug, Args)]
pub(super) struct SaveArgs {
    /// Path to a .tes file
    pub(super) path: PathBuf,
    /// Named draft to update
    #[arg(long)]
    pub(super) draft: Option<String>,
    /// Optional message stored on the revision
    #[arg(short = 'm', long)]
    pub(super) message: Option<String>,
}

/// Flags for `tes log`.
#[derive(Debug, Args)]
pub(super) struct LogArgs {
    /// Path to a .tes file
    pub(super) path: PathBuf,
    /// Emit full history JSON
    #[arg(long)]
    pub(super) json: bool,
}

/// Flags for `tes diff`.
#[derive(Debug, Args)]
pub(super) struct DiffArgs {
    /// Path to a .tes file
    pub(super) path: PathBuf,
    /// Left revision id or draft name
    pub(super) left: String,
    /// Right revision id or draft name
    pub(super) right: String,
}

/// Flags for `tes changelog`.
#[derive(Debug, Args)]
pub(super) struct ChangelogArgs {
    /// Path to a .tes file
    pub(super) path: PathBuf,
    /// Left revision id or draft name
    #[arg(long = "between", num_args = 2, value_names = ["LEFT", "RIGHT"])]
    pub(super) between: Vec<String>,
}

/// Flags for `tes export-revs`.
#[derive(Debug, Args)]
pub(super) struct ExportRevsArgs {
    /// Path to a .tes file with history
    pub(super) path: PathBuf,
    /// Revision id or draft name
    pub(super) rev: String,
    /// Output `.tes` path
    #[arg(short = 'o', long = "output", required = true)]
    pub(super) output: PathBuf,
    /// Attach the current THST footer to the export
    #[arg(long = "keep-history")]
    pub(super) keep_history: bool,
}

/// Flags for `tes checkout`.
#[derive(Debug, Args)]
pub(super) struct CheckoutArgs {
    /// Path to a .tes file with history
    pub(super) path: PathBuf,
    /// Revision id or draft name
    pub(super) rev: String,
}

/// Flags for `tes blame`.
#[derive(Debug, Args)]
pub(super) struct BlameArgs {
    /// Path to a .tes file with history
    pub(super) path: PathBuf,
    /// Blame only this chunk id
    #[arg(long)]
    pub(super) chunk: Option<u64>,
    /// Revision id or draft name (default: history head)
    #[arg(long)]
    pub(super) rev: Option<String>,
    /// Emit JSON report
    #[arg(long)]
    pub(super) json: bool,
}

/// Shared `--source-hash` / `--id` flags for pending accept & reject.
#[derive(Debug, Args)]
pub(super) struct PendingActionArgs {
    /// Pending id(s); omit to act on all
    #[arg(long = "id")]
    pub(super) ids: Vec<String>,
    /// Expected source hash
    #[arg(long = "source-hash", required = true)]
    pub(super) source_hash: String,
}

/// Subcommands for `tes pending`.
#[derive(Debug, Subcommand)]
pub(super) enum PendingCommands {
    /// List pending suggestions
    List {
        /// Emit JSON
        #[arg(long)]
        json: bool,
    },
    /// Queue typed ops into the THST pending slot (body unchanged)
    Suggest {
        /// JSON array of `TesOp` (file path)
        #[arg(long)]
        ops: PathBuf,
        /// Expected source hash
        #[arg(long = "source-hash", required = true)]
        source_hash: String,
        /// Optional message
        #[arg(short = 'm', long)]
        message: Option<String>,
    },
    /// Dry-run Tessprek redline of sealed body + all pending ops
    Redline {
        /// Expected source hash
        #[arg(long = "source-hash", required = true)]
        source_hash: String,
    },
    /// Apply selected (or all) pending ops to the sealed body, then drop them
    Accept(PendingActionArgs),
    /// Drop selected (or all) pending ops without changing the body
    Reject(PendingActionArgs),
}

/// Flags for `tes textconv`.
#[derive(Debug, Args)]
pub(super) struct TextconvArgs {
    /// Path to a .tes file
    pub(super) path: PathBuf,
}

/// Positional paths for `tes merge-file` (git: `%O %A %B`).
#[derive(Debug, Args)]
pub(super) struct MergeFileArgs {
    /// Common ancestor (`%O`)
    pub(super) base: PathBuf,
    /// Ours / current branch — also the output path (`%A`)
    pub(super) ours: PathBuf,
    /// Theirs / incoming branch (`%B`)
    pub(super) theirs: PathBuf,
}

#[derive(Debug, Subcommand)]
pub(super) enum LinkCommands {
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

#[derive(Debug, Subcommand)]
pub(super) enum VaultCommands {
    /// Rebuild the optional `vault.tes` catalog index
    Rebuild,
    /// List documents (uses `vault.tes` when fresh)
    List(VaultListArgs),
    /// Import a Markdown / Obsidian folder into this vault
    Import {
        /// Root directory of `.md` files (`.obsidian` skipped)
        source: PathBuf,
        /// Emit JSON report
        #[arg(long)]
        json: bool,
    },
    /// Register an external `.tes` file or extra root directory
    Add {
        /// Path to a `.tes` file or directory (may be outside the vault root)
        path: PathBuf,
    },
    /// Unregister a previously registered external path
    Remove {
        /// Path previously passed to `vault add`
        path: PathBuf,
    },
    /// List registered external members (not the automatic in-tree scan)
    Members {
        /// Emit JSON
        #[arg(long)]
        json: bool,
    },
    /// Search vault membership (scan by default; Tantivy under `.tessera/fts` when large / `--index`)
    Search(VaultSearchArgs),
}

/// Flags for `tes vault list`.
#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
pub(super) struct VaultListArgs {
    /// Filter by catalog tag
    #[arg(long)]
    pub(super) tag: Option<String>,
    /// Filter by catalog category
    #[arg(long)]
    pub(super) category: Option<String>,
    /// Filter by catalog section (path under category)
    #[arg(long)]
    pub(super) section: Option<String>,
    /// Ignore `vault.tes` and scan catalogs
    #[arg(long)]
    pub(super) force_scan: bool,
    /// Emit JSON
    #[arg(long)]
    pub(super) json: bool,
    /// Force aligned table (default when stdout is a TTY)
    #[arg(long)]
    pub(super) table: bool,
    /// Force tab-separated rows (default when stdout is not a TTY)
    #[arg(long)]
    pub(super) tsv: bool,
}

/// Flags for `tes vault search`.
#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
pub(super) struct VaultSearchArgs {
    /// Query string (Tantivy syntax when indexed). Omit with `--rebuild` to only rebuild the index.
    pub(super) query: Option<String>,
    /// Force Tantivy index (also used when membership ≥ 64 docs)
    #[arg(long, conflicts_with = "scan")]
    pub(super) index: bool,
    /// Force parallel scan (no sidecar)
    #[arg(long, conflicts_with = "index")]
    pub(super) scan: bool,
    /// Rebuild the Tantivy index under `.tessera/fts`
    #[arg(long)]
    pub(super) rebuild: bool,
    /// Always rebuild before an indexed search
    #[arg(long)]
    pub(super) force_rebuild: bool,
    /// Maximum hits (default 50)
    #[arg(long, default_value_t = 50)]
    pub(super) limit: usize,
    /// Emit JSON
    #[arg(long)]
    pub(super) json: bool,
}
