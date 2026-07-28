//! Local browser preview (`tes serve`).
//!
//! Projects a `.tes` file through the same semantic HTML export used by
//! `tes export --html`, then applies an external template/theme pack. Themes
//! are CSS-only by default; the server binds loopback and re-reads the file
//! on every request.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::catalog::file::TesFile;
use crate::error::{Result, TesError};
use crate::export::{ExportOptions, ExportView, export_file};
use crate::template::{TemplatePack, ThemeFallback, resolve_pack_and_theme};

/// Options for [`serve_preview`].
#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// `.tes` file to preview.
    pub path: PathBuf,
    /// Directory containing template pack folders.
    pub template_root: PathBuf,
    /// Pack id; falls back to catalog `template_id`, then [`DEFAULT_TEMPLATE_ID`].
    pub template_id: Option<String>,
    /// Theme id (`draft`, `print`, …); falls back to catalog `theme_id`, then draft.
    pub theme_id: Option<String>,
    /// Bind address (default `127.0.0.1`).
    pub host: String,
    /// Bind port (`0` = ephemeral).
    pub port: u16,
    /// Inject a meta refresh so the browser reloads while editing.
    pub watch: bool,
    /// Meta-refresh interval in seconds when `watch` is set.
    pub watch_secs: u64,
    /// Permit packs that declare `requires_theme_js` (still does not execute pack JS).
    pub allow_theme_js: bool,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            path: PathBuf::from("document.tes"),
            template_root: PathBuf::from("templates"),
            template_id: None,
            theme_id: None,
            host: "127.0.0.1".into(),
            port: 7878,
            watch: false,
            watch_secs: 2,
            allow_theme_js: false,
        }
    }
}

/// Resolved pack + theme used for a preview session.
#[derive(Debug, Clone)]
pub struct PreviewContext {
    /// Loaded template pack.
    pub pack: TemplatePack,
    /// Selected theme id.
    pub theme_id: String,
}

/// Resolve pack/theme for a document without starting the server.
///
/// # Errors
///
/// Returns open errors from [`TesFile::open`], template/theme errors from
/// [`TemplatePack`], or [`TesError::ThemeJsNotAllowed`] when the pack requires JS
/// and `allow_theme_js` is false.
pub fn resolve_preview_context(options: &ServeOptions) -> Result<PreviewContext> {
    let file = TesFile::open(&options.path)?;
    let catalog = file.catalog();
    let resolved = resolve_pack_and_theme(
        catalog.and_then(|c| c.template_id.as_deref()),
        catalog.and_then(|c| c.theme_id.as_deref()),
        &options.template_root,
        options.template_id.as_deref(),
        options.theme_id.as_deref(),
        ThemeFallback::Draft,
    )?;
    if resolved.pack.manifest.requires_theme_js && !options.allow_theme_js {
        return Err(TesError::ThemeJsNotAllowed {
            template_id: resolved.pack.manifest.id.clone(),
        });
    }

    Ok(PreviewContext {
        pack: resolved.pack,
        theme_id: resolved.theme_id,
    })
}

/// Render standalone HTML for the current file + pack theme.
///
/// # Errors
///
/// Returns open errors from [`TesFile::open`], theme CSS errors from
/// [`TemplatePack::theme_css`], or HTML export errors.
pub fn render_preview_html(options: &ServeOptions, ctx: &PreviewContext) -> Result<String> {
    let file = TesFile::open(&options.path)?;
    // Confirm the theme still resolves (pack may have changed on disk).
    let _ = ctx.pack.theme_css(&ctx.theme_id)?;
    let mut html = export_file(
        &file,
        ExportView::Html,
        &ExportOptions {
            standalone: true,
            theme_href: Some("/theme.css".into()),
            media_url_prefix: Some("/media/".into()),
            ..ExportOptions::default()
        },
    )?;

    // Prefer linked theme for CSP; optionally inject watch meta.
    if options.watch {
        let secs = options.watch_secs.max(1);
        let meta = format!(
            "<meta http-equiv=\"refresh\" content=\"{secs}\">\n<meta name=\"tes-preview\" content=\"watch\">\n"
        );
        html = inject_after_head_open(&html, &meta);
    }

    if !html.contains("/theme.css") {
        html = inject_after_head_open(&html, "<link rel=\"stylesheet\" href=\"/theme.css\">\n");
    }

    Ok(html)
}

/// Bind loopback and serve until `shutdown` is set (or forever if `None`).
///
/// # Errors
///
/// Returns [`TesError::InvalidServeHost`] for a non-loopback host, context errors from
/// [`resolve_preview_context`], or [`TesError::Io`] if bind/accept fails.
pub fn serve_preview(options: &ServeOptions, shutdown: Option<&Arc<AtomicBool>>) -> Result<()> {
    validate_loopback_host(&options.host)?;
    let ctx = resolve_preview_context(options)?;
    let listener = TcpListener::bind((options.host.as_str(), options.port))?;
    listener.set_nonblocking(false)?;
    let addr = listener.local_addr()?;

    eprintln!(
        "serving {} with template '{}' theme '{}' at http://{addr}/",
        options.path.display(),
        ctx.pack.manifest.id,
        ctx.theme_id
    );
    if options.watch {
        eprintln!(
            "watch enabled: browser meta-refresh every {}s (CSS-only themes)",
            options.watch_secs.max(1)
        );
    }
    eprintln!("press Ctrl-C to stop");

    // Accept loop.
    listener.set_nonblocking(true)?;
    loop {
        if shutdown.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            break;
        }
        match listener.accept() {
            Ok((stream, peer)) => {
                if let Err(err) = handle_client(stream, options, &ctx) {
                    eprintln!("preview error from {peer}: {err}");
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(TesError::Io(err)),
        }
    }
    Ok(())
}

fn handle_client(
    mut stream: TcpStream,
    options: &ServeOptions,
    ctx: &PreviewContext,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let path = request_path(&req).unwrap_or("/");

    match path {
        "/" | "/index.html" => {
            // Re-resolve theme CSS each request so pack edits apply on refresh.
            let html = render_preview_html(options, ctx)?;
            write_response(
                &mut stream,
                200,
                "text/html; charset=utf-8",
                html.as_bytes(),
                true,
            )?;
        }
        "/theme.css" => {
            let css = ctx.pack.theme_css(&ctx.theme_id)?;
            write_response(
                &mut stream,
                200,
                "text/css; charset=utf-8",
                css.as_bytes(),
                true,
            )?;
        }
        "/healthz" => {
            write_response(&mut stream, 200, "text/plain; charset=utf-8", b"ok\n", true)?;
        }
        other if other.starts_with("/media/") => {
            let id_str = other.trim_start_matches("/media/");
            match id_str.parse::<u64>() {
                Ok(chunk_id) => match load_image_media(&options.path, chunk_id) {
                    Ok((media_type, data)) => {
                        write_response(&mut stream, 200, &media_type, &data, true)?;
                    }
                    Err(_) => {
                        write_response(
                            &mut stream,
                            404,
                            "text/plain; charset=utf-8",
                            b"media not found\n",
                            true,
                        )?;
                    }
                },
                Err(_) => {
                    write_response(
                        &mut stream,
                        404,
                        "text/plain; charset=utf-8",
                        b"not found\n",
                        true,
                    )?;
                }
            }
        }
        _ => {
            write_response(
                &mut stream,
                404,
                "text/plain; charset=utf-8",
                b"not found\n",
                true,
            )?;
        }
    }
    Ok(())
}

fn load_image_media(path: &Path, chunk_id: u64) -> Result<(String, Vec<u8>)> {
    use crate::catalog::index::ChunkType;
    use crate::catalog::media::ImagePayload;

    let file = TesFile::open(path)?;
    let entry = file.chunk_by_id(chunk_id)?;
    if entry.chunk_type != ChunkType::Image {
        return Err(TesError::InvalidImage {
            message: format!("chunk {chunk_id} is not an image"),
        });
    }
    let raw = file.decode_payload(entry)?;
    let image = ImagePayload::from_bytes(&raw)?;
    Ok((image.media_type, image.data))
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
    no_store: bool,
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Error",
    };
    let cache = if no_store {
        "Cache-Control: no-store\r\n"
    } else {
        ""
    };
    // CSS-only preview: no scripts, no remote assets.
    let csp = "Content-Security-Policy: default-src 'none'; style-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n";
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\n\
Content-Type: {content_type}\r\n\
Content-Length: {}\r\n\
{cache}{csp}\
Connection: close\r\n\
X-Content-Type-Options: nosniff\r\n\
\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

fn request_path(req: &str) -> Option<&str> {
    let line = req.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    if method != "GET" && method != "HEAD" {
        return None;
    }
    let target = parts.next()?;
    Some(target.split('?').next().unwrap_or(target))
}

fn inject_after_head_open(html: &str, snippet: &str) -> String {
    if let Some(idx) = html.find("<head>") {
        let insert_at = idx + "<head>".len();
        let mut out = String::with_capacity(html.len() + snippet.len() + 1);
        out.push_str(&html[..insert_at]);
        out.push('\n');
        out.push_str(snippet);
        out.push_str(&html[insert_at..]);
        out
    } else {
        format!("{snippet}{html}")
    }
}

fn validate_loopback_host(host: &str) -> Result<()> {
    match host {
        "127.0.0.1" | "localhost" | "::1" => Ok(()),
        other => Err(TesError::InvalidServeHost {
            host: other.to_string(),
        }),
    }
}

/// Convenience: local address after a successful bind (for tests).
///
/// # Errors
///
/// Returns [`TesError::InvalidServeHost`] for a non-loopback host, context errors from
/// [`resolve_preview_context`], or [`TesError::Io`] if bind fails.
pub fn bind_preview_listener(
    options: &ServeOptions,
) -> Result<(TcpListener, SocketAddr, PreviewContext)> {
    validate_loopback_host(&options.host)?;
    let ctx = resolve_preview_context(options)?;
    let listener = TcpListener::bind((options.host.as_str(), options.port))?;
    let addr = listener.local_addr()?;
    Ok((listener, addr, ctx))
}

/// Shared export path helper for callers that want HTML without serving.
///
/// # Errors
///
/// Returns errors from [`resolve_preview_context`] or [`render_preview_html`].
pub fn preview_html_for_path(
    path: impl AsRef<Path>,
    template_root: impl AsRef<Path>,
    theme_id: Option<&str>,
) -> Result<String> {
    let options = ServeOptions {
        path: path.as_ref().to_path_buf(),
        template_root: template_root.as_ref().to_path_buf(),
        theme_id: theme_id
            .map(str::to_string)
            .or_else(|| Some(crate::template::THEME_DRAFT.into())),
        ..ServeOptions::default()
    };
    let ctx = resolve_preview_context(&options)?;
    render_preview_html(&options, &ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::session::TesWriterSession;
    use crate::layout::DocKind;
    use std::net::TcpStream;
    use std::thread;

    fn sample_tes(dir: &Path) -> PathBuf {
        use crate::catalog::chunk::TextHeader;
        use crate::catalog::document::DocumentCatalog;

        let path = dir.join("note.tes");
        let mut session = TesWriterSession::create(&path, DocKind::Note);
        session
            .set_catalog(DocumentCatalog::new(
                "550e8400-e29b-41d4-a716-446655440099",
                "Preview note",
                "2026-07-25T00:00:00Z",
                "2026-07-25T00:00:00Z",
                DocKind::Note,
            ))
            .unwrap();
        session
            .add_text_chunk(&TextHeader::heading(1), "Hello preview")
            .unwrap();
        session
            .add_text_chunk(
                &TextHeader::paragraph(),
                "This is served through the HTML export path.",
            )
            .unwrap();
        session.commit().unwrap();
        path
    }

    #[test]
    fn renders_with_minimal_draft_theme() {
        let dir = tempfile::tempdir().unwrap();
        let tes = sample_tes(dir.path());
        let templates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
        let html = preview_html_for_path(&tes, &templates, Some("draft")).unwrap();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Hello preview"));
        assert!(html.contains("/theme.css"));
        assert!(html.contains("data-doc-id"));
    }

    #[test]
    fn serve_loopback_returns_html_and_css() {
        let dir = tempfile::tempdir().unwrap();
        let tes = sample_tes(dir.path());
        let templates = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
        let options = ServeOptions {
            path: tes,
            template_root: templates,
            port: 0,
            watch: true,
            watch_secs: 3,
            ..ServeOptions::default()
        };
        let (listener, addr, ctx) = bind_preview_listener(&options).unwrap();
        listener.set_nonblocking(false).unwrap();

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_flag = Arc::clone(&shutdown);
        let options_clone = options.clone();
        let handle = thread::spawn(move || {
            // One request for /, one for /theme.css, then stop.
            for _ in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                handle_client(stream, &options_clone, &ctx).unwrap();
            }
            shutdown_flag.store(true, Ordering::Relaxed);
        });

        let mut html = String::new();
        {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream
                .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .unwrap();
            stream.read_to_string(&mut html).unwrap();
        }
        assert!(html.contains("HTTP/1.1 200"));
        assert!(html.contains("Hello preview"));
        assert!(html.contains("http-equiv=\"refresh\""));
        assert!(html.contains("Content-Security-Policy"));

        let mut css = String::new();
        {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream
                .write_all(
                    b"GET /theme.css HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            stream.read_to_string(&mut css).unwrap();
        }
        assert!(css.contains("HTTP/1.1 200"));
        assert!(css.contains("--tes-bg") || css.contains("@page"));

        handle.join().unwrap();
        assert!(shutdown.load(Ordering::Relaxed));
    }
}
