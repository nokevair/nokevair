//! Utilities for loading and maintaining Tera templates.

use hyper::{Response, Body};
use tera::Tera;

use std::sync::PoisonError;

use super::Result;

/// Return a `Tera` instance containing all templates used by the application.
pub fn load() -> Tera {
    // TODO: return an Err instead of panicking so it can be caught
    let mut tera = Tera::default();
    tera.add_template_file("templates/base.html.tera", Some("base.html")).unwrap();
    tera.add_template_file("templates/about.html.tera", Some("about.html")).unwrap();
    tera.add_template_file("templates/state.html.tera", Some("state.html")).unwrap();
    tera.add_template_file("templates/login.html.tera", Some("login.html")).unwrap();
    tera
}

impl super::AppState {
    /// Render a Tera template with the provided context.
    /// TODO: `error_*()` functions will eventually attempt to call this function,
    /// so we need to remove their invocations here to avoid infinite recursion. 
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
                tera::ErrorKind::TemplateNotFound(_) => self.error_404(),
                _ => self.error_500(format!("while rendering template: {:?}", e)),
            }
        }
    }

    /// Replace the current `Tera` instance with a new one based on the current
    /// version of the template files.
    pub(super) fn reload_templates(&self) {
        let mut templates = self.templates.write()
            .unwrap_or_else(PoisonError::into_inner);
        *templates = load();
    }
}
