use tera::Tera;

// TODO: make this return an Err instead of panicking so it can be caught
pub fn load() -> Tera {
    let mut tera = Tera::default();
    tera.add_template_file("templates/base.html.tera", Some("base.html")).unwrap();
    tera.add_template_file("templates/about.html.tera", Some("about.html")).unwrap();
    tera.add_template_file("templates/state.html.tera", Some("state.html")).unwrap();
    tera
}
