//! Utilities for loading and maintaining Tera templates.

use hyper::{Response, Body};
use tera::Tera;

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use super::{Result, Ctx};

/// Return a `Tera` instance containing all templates used by the application.
pub fn load(ctx: &Ctx) -> Tera {
    let mut tera = Tera::default();

    let mut base_path: Cow<Path>;

    macro_rules! register {
        ($name:expr => $path:expr) => {{
            if let Err(e) = tera.add_template_file(base_path.join($path), Some($name)) {
                ctx.log.err(format_args!("could not load template '{}': {}", $name, e));
            }
        }}
    }

    base_path = (&*ctx.cfg.paths.templates).into();
    
    // Generic parent, defining structure for all pages
    register!("base.html" => "base.html.tera");

    // Hard-coded content pages
    register!("about.html" => "about.html.tera");
    register!("login.html" => "login.html.tera");

    // Error messages
    register!("400.html" => "error/400.html.tera");
    register!("401.html" => "error/401.html.tera");
    register!("404.html" => "error/404.html.tera");
    register!("404_no_state.html" => "error/404_no_state.html.tera");
    register!("500.html" => "error/500.html.tera");

    // Pages accessible only to admins
    register!("admin/index.html" => "admin/index.html.tera");

    base_path = (&*ctx.cfg.paths.render).into();

    // Generic parent for all pages in the renderer
    register!("format_base.html" => "format_base.html.tera");

    base_path = PathBuf::new().into();

    // Add pages from the directory `/render`.
    super::with_renderer_entries(ctx, |name, path| {
        register!(&format!("render/{}.html", name) => path.join("format.html.tera"));
    });

    tera
}

impl super::AppState {
    /// Render a Tera template with the provided context.
    pub(super) fn render(&self, name: &str, ctx: &tera::Context) -> Result<Response<Body>> {
        let templates = self.templates.read()
            .unwrap_or_else(PoisonError::into_inner);
        match templates.render(name, ctx) {
            Ok(body) => {
                let mime = mime_guess::from_path(name).first_or_octet_stream();
                Ok(Response::builder()
                    .status(200)
                    .header("Content-Type", &format!("{}", mime))
                    .body(Body::from(body))
                    .unwrap())
            }
            Err(_) if name == "500.html" => {
                // If attempting to render the 500 page causes a 500,
                // just return a textual error to avoid infinite recursion.
                self.ctx.log.err("recursive 500");
                Self::text_error(500,
                    "500: while attempting to handle the error, the server encountered an error")
            }
            Err(e) => self.error_500(format_args!("while rendering template '{}': {}", name, e)),
        }
    }

    /// Replace the current `Tera` instance with a new one based on the current
    /// version of the template files.
    pub(super) fn reload_templates(&self) {
        let mut templates = self.templates.write()
            .unwrap_or_else(PoisonError::into_inner);
        *templates = load(&self.ctx);
    }
}
