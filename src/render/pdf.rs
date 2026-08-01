//! Print/PDF export under [`crate::render`].
//!
//! Two backends:
//! - **Chromium** (default): semantic HTML + print-theme → headless Chromium
//! - **Native** (THI-294, feature `native-pdf`): print IR → `ariadnes_weave::emit_pdf`
//!
//! Browser preview (`tes serve --theme print`) still shares the HTML path with
//! the Chromium backend. PDF is never an editable canonical source.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use super::template::{ThemeFallback, resolve_pack_and_theme};
use crate::catalog::file::TesFile;
use crate::error::{Result, TesError};
use crate::io::export::{ExportOptions, ExportView, export_file};
use crate::layout::DocKind;

/// PDF generation engine for [`export_pdf`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PdfBackend {
    /// HTML + print theme → headless Chromium (current default).
    #[default]
    Chromium,
    /// Print IR → ariadnes-weave (no Chromium).
    Native,
}

impl PdfBackend {
    /// Stable CLI / docs name (`chromium` | `native`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chromium => "chromium",
            Self::Native => "native",
        }
    }
}

impl FromStr for PdfBackend {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "chromium" | "chrome" => Ok(Self::Chromium),
            "native" | "weave" | "ariadnes-weave" => Ok(Self::Native),
            other => Err(format!(
                "unknown PDF backend '{other}' (expected chromium|native)"
            )),
        }
    }
}

/// Options for themed HTML and PDF export.
#[derive(Debug, Clone)]
pub struct PdfExportOptions {
    /// Directory containing template pack folders.
    pub template_root: PathBuf,
    /// Pack id; falls back to catalog `template_id`, then [`super::template::DEFAULT_TEMPLATE_ID`].
    pub template_id: Option<String>,
    /// Theme id; defaults to [`THEME_PRINT`], or manuscript theme for `doc_kind = manuscript`.
    pub theme_id: Option<String>,
    /// Restrict PDF body to the Nth chapter (1-based H1 slice).
    pub chapter: Option<u32>,
    /// Explicit Chromium/Chrome binary; otherwise auto-detect.
    pub chrome_path: Option<PathBuf>,
    /// PDF engine; defaults to [`PdfBackend::Chromium`].
    pub backend: PdfBackend,
}

impl Default for PdfExportOptions {
    fn default() -> Self {
        Self {
            template_root: PathBuf::from("templates"),
            template_id: None,
            // None → PreferPrint / PreferManuscript from doc_kind.
            theme_id: None,
            chapter: None,
            chrome_path: None,
            backend: PdfBackend::Chromium,
        }
    }
}

/// Render standalone HTML with the selected template theme CSS embedded.
///
/// Images are inlined as data URIs so the document is self-contained for print.
///
/// # Errors
///
/// Returns open/parse errors from [`TesFile::open`], template resolution errors
/// from [`super::template::TemplatePack::resolve`], or HTML export errors from
/// [`crate::io::export::export_file`].
pub fn render_themed_html(path: impl AsRef<Path>, options: &PdfExportOptions) -> Result<String> {
    let path = path.as_ref();
    let file = TesFile::open(path)?;
    let catalog = file.catalog();
    let fallback = if file.superblock().doc_kind == DocKind::Manuscript {
        ThemeFallback::PreferManuscript
    } else {
        ThemeFallback::PreferPrint
    };
    let resolved = resolve_pack_and_theme(
        catalog.and_then(|c| c.template_id.as_deref()),
        catalog.and_then(|c| c.theme_id.as_deref()),
        &options.template_root,
        options.template_id.as_deref(),
        options.theme_id.as_deref(),
        fallback,
    )?;
    let css = resolved.pack.theme_css(&resolved.theme_id)?;

    export_file(
        &file,
        ExportView::Html,
        &ExportOptions {
            chapter: options.chapter,
            standalone: true,
            embedded_css: Some(css),
            media_url_prefix: None, // data URIs for self-contained print
            ..ExportOptions::default()
        },
    )
}

/// Export `path` to a PDF file at `output`.
///
/// Backend is selected by [`PdfExportOptions::backend`] (`chromium` default,
/// `native` for ariadnes-weave when the `native-pdf` Cargo feature is enabled).
///
/// # Errors
///
/// Returns errors from [`render_themed_html`] / [`find_chrome`] / native emit,
/// [`TesError::Io`] for output writes, or [`TesError::PdfEngine`] if emit fails,
/// the `native-pdf` feature is disabled, or output is not a PDF.
pub fn export_pdf(
    path: impl AsRef<Path>,
    output: impl AsRef<Path>,
    options: &PdfExportOptions,
) -> Result<()> {
    match options.backend {
        PdfBackend::Chromium => export_pdf_chromium(path.as_ref(), output.as_ref(), options),
        PdfBackend::Native => export_pdf_native(path.as_ref(), output.as_ref(), options),
    }
}

#[cfg(feature = "native-pdf")]
fn export_pdf_native(path: &Path, output: &Path, options: &PdfExportOptions) -> Result<()> {
    use ariadnes_weave::PrintProfileId;

    use super::print::{PrintBuildOptions, build_print_document};

    let file = TesFile::open(path)?;
    let profile = match options.theme_id.as_deref() {
        Some("manuscript") => Some(PrintProfileId::manuscript_v0()),
        Some("deck") => Some(PrintProfileId::deck_v0()),
        Some("print") => Some(PrintProfileId::print_v0()),
        _ => None,
    };
    let doc = build_print_document(
        &file,
        &PrintBuildOptions {
            chapter: options.chapter,
            profile,
        },
    )?;
    let bytes = ariadnes_weave::emit_pdf(&doc).map_err(|err| TesError::PdfEngine {
        message: format!("ariadnes-weave emit failed: {err}"),
    })?;
    if bytes.len() < 5 || &bytes[..5] != b"%PDF-" {
        return Err(TesError::PdfEngine {
            message: "ariadnes-weave output is not a PDF".into(),
        });
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, bytes)?;
    Ok(())
}

#[cfg(not(feature = "native-pdf"))]
fn export_pdf_native(_path: &Path, _output: &Path, _options: &PdfExportOptions) -> Result<()> {
    Err(TesError::PdfEngine {
        message: "native PDF backend requires the `native-pdf` Cargo feature \
                  (default; rebuild with --features native-pdf)"
            .into(),
    })
}

fn export_pdf_chromium(path: &Path, output: &Path, options: &PdfExportOptions) -> Result<()> {
    let html = render_themed_html(path, options)?;
    let chrome = match &options.chrome_path {
        Some(p) => p.clone(),
        None => find_chrome()?,
    };

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp_dir = std::env::temp_dir().join(format!("tessera-pdf-{stamp}"));
    fs::create_dir_all(&tmp_dir)?;
    let html_path = tmp_dir.join("document.html");
    let pdf_tmp = tmp_dir.join("document.pdf");
    fs::write(&html_path, html.as_bytes())?;

    let html_url = path_to_file_url(&html_path);
    let mut cmd = Command::new(&chrome);
    cmd.arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        // Small /dev/shm in CI/containers otherwise OOMs the renderer.
        .arg("--disable-dev-shm-usage")
        .arg("--no-pdf-header-footer")
        .arg(format!("--print-to-pdf={}", pdf_tmp.display()));
    // Ubuntu 23.10+ / many CI images disable unprivileged user namespaces, so
    // Chromium's sandbox aborts. Input is our own temp HTML — safe to relax.
    if chrome_needs_relaxed_sandbox() {
        cmd.arg("--no-sandbox").arg("--disable-setuid-sandbox");
    }
    let status = cmd
        .arg(&html_url)
        .status()
        .map_err(|err| TesError::PdfEngine {
            message: format!("failed to launch '{}': {err}", chrome.display()),
        })?;

    if !status.success() {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(TesError::PdfEngine {
            message: format!(
                "chrome print failed with status {status} (binary: {})",
                chrome.display()
            ),
        });
    }

    if !pdf_tmp.is_file() {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(TesError::PdfEngine {
            message: "chrome reported success but produced no PDF".into(),
        });
    }

    let bytes = fs::read(&pdf_tmp)?;
    if bytes.len() < 5 || &bytes[..5] != b"%PDF-" {
        let _ = fs::remove_dir_all(&tmp_dir);
        return Err(TesError::PdfEngine {
            message: "chrome output is not a PDF".into(),
        });
    }

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, bytes)?;
    let _ = fs::remove_dir_all(&tmp_dir);
    Ok(())
}

fn chrome_needs_relaxed_sandbox() -> bool {
    cfg!(target_os = "linux")
        || std::env::var_os("CI").is_some()
        || std::env::var_os("TES_CHROME_NO_SANDBOX").is_some()
}

/// Locate a Chromium-family browser suitable for headless print.
///
/// # Errors
///
/// Returns [`TesError::PdfEngine`] if `TES_CHROME` points to a missing binary,
/// or if no Chromium/Chrome candidate is found on `PATH` / common install paths.
pub fn find_chrome() -> Result<PathBuf> {
    if let Ok(explicit) = std::env::var("TES_CHROME") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
        return Err(TesError::PdfEngine {
            message: format!("TES_CHROME points to missing binary: {}", path.display()),
        });
    }

    let candidates = [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "chrome",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
    ];

    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Ok(path);
        }
        if let Ok(output) = Command::new("which").arg(candidate).output()
            && output.status.success()
        {
            let found = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !found.is_empty() {
                let found_path = PathBuf::from(found);
                if found_path.is_file() {
                    return Ok(found_path);
                }
            }
        }
    }

    Err(TesError::PdfEngine {
        message: "no Chromium/Chrome binary found; install Chrome or set TES_CHROME".into(),
    })
}

fn path_to_file_url(path: &Path) -> String {
    let abs = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let raw = abs.to_string_lossy().replace('\\', "/");
    let mut url = String::from("file://");
    // Windows paths become file:///C:/…; Unix paths become file:///…
    if !raw.starts_with('/') {
        url.push('/');
    }
    for byte in raw.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                url.push(*byte as char);
            }
            b => {
                let _ = write!(url, "%{b:02X}");
            }
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DocumentCatalog, TesWriterSession, TextHeader};
    use crate::layout::DocKind;
    use tempfile::tempdir;

    fn sample_doc(dir: &Path) -> PathBuf {
        let path = dir.join("print.tes");
        let mut session = TesWriterSession::create(&path, DocKind::Document);
        session
            .set_catalog(DocumentCatalog::new(
                "880e8400-e29b-41d4-a716-446655440003",
                "Print specimen",
                "2026-07-25T00:00:00Z",
                "2026-07-25T00:00:00Z",
                DocKind::Document,
            ))
            .unwrap();
        session
            .add_text_chunk(&TextHeader::heading(1), "Print specimen")
            .unwrap();
        session
            .add_text_chunk(
                &TextHeader::paragraph(),
                "This document exercises the shared HTML + print theme path.",
            )
            .unwrap();
        session.commit().unwrap();
        path
    }

    #[test]
    fn themed_html_embeds_print_css() {
        let dir = tempdir().unwrap();
        let tes = sample_doc(dir.path());
        let templates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
        let html = render_themed_html(
            &tes,
            &PdfExportOptions {
                template_root: templates,
                theme_id: Some("print".into()),
                ..PdfExportOptions::default()
            },
        )
        .unwrap();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("@page"));
        assert!(html.contains("Print specimen"));
        assert!(html.contains("<style>"));
        assert!(!html.contains("/theme.css"));
    }

    #[test]
    fn manuscript_theme_and_chapter_scope() {
        let dir = tempdir().unwrap();
        let tes = dir.path().join("ms.tes");
        fs::write(&tes, crate::fixtures::samples::encode_manuscript_chapters()).unwrap();
        let templates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
        let html = render_themed_html(
            &tes,
            &PdfExportOptions {
                template_root: templates,
                chapter: Some(2),
                // catalog theme_id is manuscript; PreferManuscript also applies.
                ..PdfExportOptions::default()
            },
        )
        .unwrap();
        assert!(html.contains("Courier"));
        assert!(html.contains("line-height: 2"));
        assert!(html.contains("Chapter 2"));
        assert!(html.contains("lantern blinked"));
        assert!(!html.contains("Chapter 1"));
        assert!(!html.contains("Chapter 3"));
        assert!(!html.contains("beta readers"));
    }

    #[cfg(feature = "native-pdf")]
    #[test]
    fn export_pdf_native_note_three_chunks() {
        let dir = tempdir().unwrap();
        let tes = dir.path().join("note.tes");
        fs::write(&tes, crate::fixtures::v0::encode_note_three_chunks()).unwrap();
        let out = dir.path().join("native.pdf");
        export_pdf(
            &tes,
            &out,
            &PdfExportOptions {
                backend: PdfBackend::Native,
                ..PdfExportOptions::default()
            },
        )
        .unwrap();
        let bytes = fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.len() > 200);
    }

    #[cfg(feature = "native-pdf")]
    #[test]
    fn export_pdf_native_manuscript_chapter() {
        let dir = tempdir().unwrap();
        let tes = dir.path().join("ms.tes");
        fs::write(&tes, crate::fixtures::samples::encode_manuscript_chapters()).unwrap();
        let out = dir.path().join("ch2.pdf");
        export_pdf(
            &tes,
            &out,
            &PdfExportOptions {
                backend: PdfBackend::Native,
                chapter: Some(2),
                theme_id: Some("manuscript".into()),
                ..PdfExportOptions::default()
            },
        )
        .unwrap();
        let bytes = fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.len() > 200);
    }

    #[test]
    fn export_pdf_when_chrome_available() {
        let Ok(chrome) = find_chrome() else {
            eprintln!("skipping PDF integration test: no Chrome/Chromium");
            return;
        };
        let dir = tempdir().unwrap();
        let tes = sample_doc(dir.path());
        let out = dir.path().join("out.pdf");
        let templates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
        match export_pdf(
            &tes,
            &out,
            &PdfExportOptions {
                template_root: templates,
                theme_id: Some("print".into()),
                chrome_path: Some(chrome),
                ..PdfExportOptions::default()
            },
        ) {
            Ok(()) => {
                let bytes = fs::read(&out).unwrap();
                assert!(bytes.starts_with(b"%PDF-"));
                assert!(bytes.len() > 500);
            }
            // Binary present but headless print crashes (GPU/sandbox/host issues).
            Err(TesError::PdfEngine { message }) => {
                eprintln!("skipping PDF integration test: {message}");
            }
            Err(err) => panic!("unexpected PDF export error: {err}"),
        }
    }
}
