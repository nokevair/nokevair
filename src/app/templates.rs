//! Utilities for loading and maintaining Tera templates.

use tera::Tera;

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

}