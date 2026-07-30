//! `tes export` and `tes import` (including bibliography).

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::catalog::TesFile;
use crate::catalog::read_summary_v0;
use crate::error::TesError;
use crate::io::bib::{BibFormat, BibImportOptions, export_bibliography, import_bibliography};
use crate::io::export::{ExportOptions, ExportView, export_attachment_bytes, export_view};
use crate::io::import::{
    HtmlImportOptions, MarkdownImportOptions, import_html_v0, import_markdown_v0,
};
use crate::layout::DocKind;
use crate::render::pdf::{PdfExportOptions, export_pdf};

use super::super::args::{ExportArgs, ImportArgs};
use super::super::util::{parse_doc_kind, print_out, resolve_template_root};

pub(in crate::cli) fn run_export(args: ExportArgs) -> Result<(), TesError> {
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

    if args.attachment {
        let Some(chunk_id) = args.chunk else {
            return Err(TesError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--attachment requires --chunk ID",
            )));
        };
        let Some(out_path) = args.output.as_ref() else {
            return Err(TesError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--attachment requires -o/--output PATH",
            )));
        };
        let file = TesFile::open(&args.path)?;
        let att = export_attachment_bytes(&file, chunk_id)?;
        fs::write(out_path, &att.data)?;
        return Ok(());
    }

    if args.pdf {
        let Some(out_path) = args.output.as_ref() else {
            return Err(TesError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--pdf requires -o/--output PATH",
            )));
        };
        let template_root = resolve_template_root(args.template_root);
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
        attachment_url_prefix: None,
    };
    let out = export_view(&args.path, view, &options)?;
    if let Some(path) = args.output.as_ref() {
        fs::write(path, out.as_bytes())?;
    } else {
        print_out(&out)?;
    }
    Ok(())
}

pub(in crate::cli) fn run_import(args: ImportArgs) -> Result<(), TesError> {
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
                ..MarkdownImportOptions::default()
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
