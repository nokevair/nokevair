//! Utilities for loading and maintaining Tera templates.

use hyper::{Response, Body};
use tera::Tera;

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use super::{Result, Ctx};
use super::lua;
use super::utils::SourceChain;

/// Contains data related to rendering templates.
pub struct Templates {
    /// The `Tera` instance holding the templates.
    tera: Tera,
    /// The number of templates contained in that instance.
    len: usize,
}

impl Templates {
    /// Create a `Tera` instance containing all templates used by the application.
    pub fn load(ctx: &Ctx) -> Self {
        let mut tera = Tera::default();
        tera.autoescape_on(vec![".html.tera"]);

        let mut len = 0;
    
        let mut base_path: Cow<Path>;
    
        macro_rules! register {
            ($name:expr => $path:expr) => {{
                if let Err(e) = tera.add_template_file(base_path.join($path), Some($name)) {
                    ctx.log.err(format_args!("tera:\n{}", SourceChain(e)));
                } else {
                    len += 1;
                }
            }}
        }
    
        base_path = (&*ctx.cfg.paths.templates).into();
        
        // Generic parent, defining structure for all pages
        register!("base.html" => "base.html.tera");
    
        // Hard-coded content pages
        register!("about.html" => "about.html.tera");
        register!("login.html" => "login.html.tera");
        register!("blog_index.html" => "blog_index.html.tera");
    
        // Error messages
        register!("400.html" => "error/400.html.tera");
        register!("401.html" => "error/401.html.tera");
        register!("404.html" => "error/404.html.tera");
        register!("404_no_state.html" => "error/404_no_state.html.tera");
        register!("500.html" => "error/500.html.tera");
    
        // Pages accessible only to admins
        register!("admin/index.html" => "admin/index.html.tera");
        register!("admin/filtered_log.html" => "admin/filtered_log.html.tera");
        register!("admin/sim_files.html" => "admin/sim_files.html.tera");

        // Blog posts
        register!("blog_base.html" => "blog_base.html.tera");
        for id in ctx.blog.ids().iter() {
            register!(&format!("blog/{}.html", id) => format!("blog/{}.html.tera", id));
        }
    
        base_path = (&*ctx.cfg.paths.render).into();
    
        // Generic parent for all pages in the renderer
        register!("format_base.html" => "format_base.html.tera");
    
        base_path = PathBuf::new().into();
    
        // Add pages from the directory `/render`.
        lua::render::with_entries(ctx, |name, path| {
            register!(&format!("render/{}.html", name) => path.join("format.html.tera"));
        });
    
        Self { tera, len }
    }
}

impl super::AppState {
    /// Render a Tera template with the provided context.
    /// 
    /// If `expect_present` is true, treat a missing template error as 500.
    /// If not, treat it as a 404.
    fn render_with_config(
        &self,
        name: &str,
        ctx: &tera::Context,
        expect_present: bool,
    ) -> Result<Response<Body>> {
        let templates = self.templates.read();
        match templates.tera.render(name, ctx) {
            Ok(body) => {
                let mime = mime_guess::from_path(name).first_or_octet_stream();
                Ok(Response::builder()
                    .status(200)
                    .header("Content-Type", &format!("{}", mime))
                    .body(Body::from(body))
                    .unwrap())
            }
            Err(e) => {
                if !expect_present && matches!(e.kind, tera::ErrorKind::TemplateNotFound(_)) {
                    if name == "404.html" {
                        self.ctx.log.err("recursive 404");
                        Self::text_error(404, "404: the 404 page was not found")
                    } else {
                        self.error_404()
                    }
                } else {
                    if name == "500.html" {
                        self.ctx.log.err("recursive 500");
                        Self::text_error(500,
                            "500: while attempting to handle the error, \
                             the server encountered an error")
                    } else {
                        self.error_500(format_args!("tera:\n{}", SourceChain(e)))
                    }
                }
            }
        }
    }

    /// Render a Tera template with the provided context. If the provided template does not
    /// exist, return a 500 error.
    pub(super) fn render(&self, name: &str, ctx: &tera::Context) -> Result<Response<Body>> {
        self.render_with_config(name, ctx, true)
    }

    /// Render a Tera template with the provided context. If the provided template does not
    /// exist, return a 404 error.
    pub(super) fn try_render(&self, name: &str, ctx: &tera::Context) -> Result<Response<Body>> {
        self.render_with_config(name, ctx, false)
    }

    /// Replace the current `Tera` instance with a new one based on the current
    /// version of the template files.
    pub(super) fn reload_templates(&self) {
        *self.templates.write() = Templates::load(&self.ctx);
    }

    /// Return the number of templates that are currently loaded.
    pub(super) fn num_templates(&self) -> usize {
        self.templates.read().len
    }
}
