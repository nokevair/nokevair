//! Utilities for loading and maintaining Tera templates.

use hyper::{Response, Body};
use tera::Tera;

use std::sync::PoisonError;

use super::{Result, Log};

/// Return a `Tera` instance containing all templates used by the application.
pub fn load(log: &Log) -> Tera {
    let mut tera = Tera::default();

    macro_rules! register {
        ($name:expr => $path:expr) => {
            if let Err(e) = tera.add_template_file($path, Some($name)) {
                log.err(format_args!("could not load template {:?}: {:?}", $name, e));
            }
        }
    }

    register!("base.html"  => "templates/base.html.tera");
    register!("about.html" => "templates/about.html.tera");
    register!("state.html" => "templates/state.html.tera");
    register!("login.html" => "templates/login.html.tera");

    register!("400.html" => "templates/error/400.html.tera");
    register!("401.html" => "templates/error/401.html.tera");
    register!("404.html" => "templates/error/404.html.tera");
    register!("500.html" => "templates/error/500.html.tera");

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
            Err(e) => match e.kind {
                tera::ErrorKind::TemplateNotFound(_) => {
                    if name == "404.html" {
                        // If attempting to render the 404 page causes a 404,
                        // just return a textual error to avoid infinite recursion.
                        self.log.err("recursive 404");
                        Self::text_error(404, "404: the 404 page was not found")
                    } else {
                        self.error_404()
                    }
                }
                _ => {
                    if name == "500.html" {
                        // If attempting to render the 500 page causes a 500,
                        // just return a textual error to avoid infinite recursion.
                        self.log.err("recursive 500");
                        Self::text_error(500,
                            "500: while attempting to handle the error, \
                             the server encountered an error")
                    } else {
                        self.error_500(format_args!("while rendering template: {:?}", e))
                    }
                }
            }
        }
    }

    /// Replace the current `Tera` instance with a new one based on the current
    /// version of the template files.
    pub(super) fn reload_templates(&self) {
        let mut templates = self.templates.write()
            .unwrap_or_else(PoisonError::into_inner);
        *templates = load(&self.log);
    }
}
