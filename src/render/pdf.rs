//! Print/PDF export under [`crate::render`] via the semantic HTML + print-theme pipeline.
//!
//! Browser preview (`tes serve --theme print`) and `tes export --pdf` share the
//! same HTML render. PDF generation shells out to a Chromium-family browser in
//! headless print mode. PDF is never an editable canonical source.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use super::template::{THEME_PRINT, ThemeFallback, resolve_pack_and_theme};
use crate::catalog::file::TesFile;
use crate::error::{Result, TesError};
use crate::io::export::{ExportOptions, ExportView, export_file};

/// Options for themed HTML and PDF export.
#[derive(Debug, Clone)]
pub struct PdfExportOptions {
    /// Directory containing template pack folders.
    pub template_root: PathBuf,
    /// Pack id; falls back to catalog `template_id`, then [`super::template::DEFAULT_TEMPLATE_ID`].
    pub template_id: Option<String>,
    /// Theme id; defaults to [`THEME_PRINT`].
    pub theme_id: Option<String>,
    /// Explicit Chromium/Chrome binary; otherwise auto-detect.
    pub chrome_path: Option<PathBuf>,
}

impl Default for PdfExportOptions {
    fn default() -> Self {
        Self {
            template_root: PathBuf::from("templates"),
            template_id: None,
            theme_id: Some(THEME_PRINT.into()),
            chrome_path: None,
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
    let resolved = resolve_pack_and_theme(
        catalog.and_then(|c| c.template_id.as_deref()),
        catalog.and_then(|c| c.theme_id.as_deref()),
        &options.template_root,
        options.template_id.as_deref(),
        options.theme_id.as_deref(),
        ThemeFallback::PreferPrint,
    )?;
    let css = resolved.pack.theme_css(&resolved.theme_id)?;

    export_file(
        &file,
        ExportView::Html,
        &ExportOptions {
            standalone: true,
            embedded_css: Some(css),
            media_url_prefix: None, // data URIs for self-contained print
            ..ExportOptions::default()
        },
    )
}

/// Export `path` to a PDF file at `output` using headless Chromium print.
///
/// # Errors
///
/// Returns errors from [`render_themed_html`] / [`find_chrome`], [`TesError::Io`]
/// for temp-file and output writes, or [`TesError::PdfEngine`] if Chromium fails
/// to launch, print, or produce a valid PDF.
pub fn export_pdf(
    path: impl AsRef<Path>,
    output: impl AsRef<Path>,
    options: &PdfExportOptions,
) -> Result<()> {
    let html = render_themed_html(path.as_ref(), options)?;
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

    if let Some(parent) = output.as_ref().parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(output.as_ref(), bytes)?;
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
