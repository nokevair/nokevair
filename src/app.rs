use async_trait::async_trait;
use hyper::{Request, Response, Body, Method};
use serde::Deserialize;
use tera::Tera;

use tokio::time::{Duration, Instant, interval};
use tokio::stream::StreamExt;

use std::collections::HashMap;
use std::net::SocketAddr;
// TODO: with some performance testing, maybe switch to parking_lot?
use std::sync::{RwLock, PoisonError};

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

fn sha256(s: &str) -> String {
    use sha2::{Sha256, digest::Digest};
    let mut hasher = Sha256::default();
    hasher.input(s);
    let result: &[u8] = &hasher.result();
    hex::encode(&result)
}

async fn read_body(body: Body) -> Vec<u8> {
    // TODO: return an Err instead of panicking
    body.fold(Ok(Vec::new()), |acc, chunk| {
        match (acc, chunk) {
            (Err(e), _) => Err(e),
            (_, Err(e)) => Err(e),
            (Ok(mut bytes), Ok(chunk)) => {
                bytes.extend_from_slice(&chunk);
                Ok(bytes)
            }
        }
    }).await.unwrap()
}

const KEY_CLEAR_INTERVAL: u64 = 60;

pub struct AppState {
    templates: RwLock<Tera>,
    state: rmpv::Value,
    login_tokens: RwLock<HashMap<u64, Instant>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            templates: RwLock::new(templates::load()),
            state: match state::latest_idx() {
                Some(n) => state::load(n),
                None => state::new(),
            },
            login_tokens: RwLock::default(),
        }
    }

    pub async fn increment_count(&self) {
        let mut interval = interval(Duration::from_secs(1));
        let mut i = 0;
        loop {
            interval.tick().await;
            i += 1;
            // TODO: rather than doing this at regular intervals, have it be triggered
            // by an authenticated POST request from the admin console.
            if i % 4 == 0 {
                self.reload_templates();
            }
            if i % KEY_CLEAR_INTERVAL == 0 {
                self.clear_login_tokens();
            }
        }
    }

    fn render(&self, name: &str, ctx: &tera::Context) -> Response<Body> {
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

    fn reload_templates(&self) {
        let mut templates = self.templates.write()
            .unwrap_or_else(PoisonError::into_inner);
        *templates = templates::load();
    }

    fn gen_login_token(&self) -> u64 {
        let token: u64 = rand::random();
        // TODO: don't publicly log secret information
        eprintln!("Generated token: {}", token);
        let mut logins = self.login_tokens.write()
            .unwrap_or_else(PoisonError::into_inner);
        logins.insert(token, Instant::now());
        token
    }

    fn clear_login_tokens(&self) {
        let mut logins = self.login_tokens.write()
            .unwrap_or_else(PoisonError::into_inner);
        let mut cleared = 0;
        // Clear any keys that were created too long ago.
        logins.retain(|_, creation_time| {
            let is_valid = creation_time.elapsed() < Duration::from_secs(KEY_CLEAR_INTERVAL);
            if !is_valid {
                cleared += 1;
            }
            is_valid
        });
        if cleared > 0 {
            // TODO: change this to an info statement
            eprintln!("Cleared {} key{}.", cleared, if cleared > 1 { "s" } else { "" });
        }
    }

    fn login(&self, body: Vec<u8>) -> Response<Body> {
        #[derive(Deserialize)]
        struct LoginData {
            token: String,
            hash: String,
        }
        (|| {
            let LoginData { token, hash } = serde_json::from_slice(&body).ok()?;
            let token: u64 = token.parse().ok()?;
            let logins = self.login_tokens.read().unwrap();
            let creation_time = logins.get(&token)?;
            if creation_time.elapsed() > Duration::from_secs(KEY_CLEAR_INTERVAL) {
                return None;
            }
            // TODO: use a better password, and read it from a file or something
            let msg = format!("{}:foobar", token);
            if sha256(&msg) == hash {
                // TODO: set a cookie so that only authenticated people can access
                // this route.
                Some(responses::redirect("/admin"))
            } else {
                Some(responses::unauthorized())
            }
        })().unwrap_or_else(responses::bad_request)
    }
}

#[async_trait]
impl Respond for AppState {
    async fn respond(&self, _: SocketAddr, req: Request<Body>) -> Response<Body> {
        // Return an error if we somehow get a URI that doesn't have a path.
        let (head, body) = req.into_parts();
        let uri = head.uri.into_parts();
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

        if head.method == Method::GET {
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
                    "login" => {
                        let token = self.gen_login_token();
                        let mut context = Context::new();
                        context.insert("token", &token);
                        self.render("login.html", &context)
                    }
                    _ => responses::not_found(),
                }
            }
        } else if head.method == Method::POST {
            let body = read_body(body).await;
            match path.as_str() {
                "login" => self.login(body),
                _ => responses::not_found()
            }
        } else {
            responses::not_found()
        }
    }
}