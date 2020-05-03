use async_trait::async_trait;
use hyper::{Request, Response, Body, Method};
use serde::Deserialize;
use tera::Tera;
use tokio::time::{Duration, interval};

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
// TODO: with some performance testing, maybe switch to parking_lot?
use std::sync::RwLock;

use crate::hyper_boilerplate::Respond;

mod responses;
mod state;
mod templates;

fn strip_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.starts_with(prefix) {
        Some(&s[prefix.as_bytes().len()..])
    } else {
        None
    }
}

pub struct AppState {
    count: AtomicU64,
    templates: RwLock<Tera>,
    state: rmpv::Value,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
            templates: RwLock::new(templates::load()),
            state: match state::latest_idx() {
                Some(n) => state::load(n),
                None => state::new(),
            }
        }
    }

    pub async fn increment_count(&self) {
        let mut interval = interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let prev_val = self.count.fetch_add(1, Ordering::Relaxed);
            // TODO: rather than doing this at regular intervals, have it be triggered
            // by an authenticated POST request from the admin console.
            if prev_val % 4 == 0 {
                self.reload_templates();
            }
        }
    }

    pub fn render(&self, name: &str, ctx: &tera::Context) -> Response<Body> {
        let templates = match self.templates.read() {
            Ok(templates) => templates,
            Err(_) => return responses::impossible("cant get templates"),
        };
        match templates.render(name, ctx) {
            Ok(body) => {
                let mime = mime_guess::from_path(name).first_or_octet_stream();
                Response::builder()
                    .status(200)
                    .header("Content-Type", &format!("{}", mime))
                    .body(Body::from(body))
                    .unwrap()
            }
            Err(e) => match e.kind {
                tera::ErrorKind::TemplateNotFound(_) => responses::not_found(),
                _ => responses::impossible(format!("{:?}", e)),
            }
        }
    }

    pub fn reload_templates(&self) {
        use std::sync::PoisonError;
        let mut template_lock = self.templates.write()
            .unwrap_or_else(PoisonError::into_inner);
        *template_lock = templates::load();
    }
}

#[async_trait]
impl Respond for AppState {
    async fn respond(&self, _: SocketAddr, req: Request<Body>) -> Response<Body> {
        // Return an error if we somehow get a URI that doesn't have a path.
        let uri = req.uri().clone().into_parts();
        let path_and_query = match uri.path_and_query {
            None => return responses::impossible("no path"),
            Some(pnq) => pnq,
        };

        // Parse a query string of the form `?i=...`
        #[derive(Deserialize)]
        struct QueryDecode { i: String }
        let path = path_and_query.path().trim_matches('/').to_owned();
        let query_param = path_and_query.query().and_then(|query|
            serde_urlencoded::from_str::<QueryDecode>(query)
                .ok()
                .map(|q| q.i));

        if req.method() == Method::GET {
            if let Some(path) = strip_prefix(&path, "static/") {
                let path = format!("static/public/{}", path);
                responses::file(&path).await
            } else {
                use tera::Context;
                match path.as_str() {
                    "about" => self.render("about.html", &Context::new()),
                    "state" => {
                        let mut context = Context::new();
                        context.insert("state", &format!("{}", self.state));
                        self.render("state.html", &context)
                    },
                    _ => responses::not_found(),
                }
            }
        } else {
            responses::not_found()
        }
    }
}