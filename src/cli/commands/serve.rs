//! `tes serve` loopback preview.

use crate::error::TesError;
use crate::render::preview::{ServeOptions, serve_preview};

use super::super::args::ServeArgs;
use super::super::util::resolve_template_root;

pub(in crate::cli) fn run_serve(args: ServeArgs) -> Result<(), TesError> {
    let template_root = resolve_template_root(args.template_root);
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
