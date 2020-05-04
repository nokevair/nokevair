//! Defines the application state and describes how it can be used
//! to serve requests and do various other tasks.

use async_trait::async_trait;
use hyper::{Request, Response, Body, Method};
use serde::Deserialize;
use tera::Tera;

use tokio::time::{Duration, Instant, interval};

use std::collections::HashMap;
use std::net::SocketAddr;
// TODO: with some performance testing, maybe switch to parking_lot?
use std::sync::RwLock;

use crate::hyper_boilerplate::Respond;
use crate::utils;

mod error;
use error::Result;

mod log;
use log::Log;

mod login;
mod responses;
mod state;
mod templates;

/// Contains all state used by the application in a
/// concurrently-accessible format.
pub struct AppState {
    /// The `Tera` instance used to render templates.
    templates: RwLock<Tera>,
    /// The list of log messages.
    log: Log,
    /// The current version of the world state.
    state: rmpv::Value,
    /// Tokens used by `/login` to authenticate the user.
    login_tokens: RwLock<HashMap<u64, Instant>>,
}

impl AppState {
    /// Initialize the state.
    pub fn new() -> Self {
        let log = Log::new();
        Self {
            templates: RwLock::new(templates::load(&log)),
            log,
            state: match state::latest_idx() {
                Some(n) => state::load(n),
                None => state::new(),
            },
            login_tokens: RwLock::default(),
        }
    }

    /// Perform various bookkeeping tasks at regular intervals.
    pub async fn do_scheduled(&self) {
        let mut interval = interval(Duration::from_secs(1));
        let mut i = 0u64;
        loop {
            interval.tick().await;
            i += 1;
            // TODO: rather than doing this at regular intervals, have it be triggered
            // by an authenticated POST request from the admin console.
            if i % 4 == 0 {
                self.reload_templates();
            }
            if i % 240 == 0 {
                self.clear_login_tokens();
            }
        }
    }

    /// Parse a query string (in the form `?i=...`) and return the parameter.
    fn get_query_param(query: &str) -> Option<String> {
        /// Describes the format that query strings are expected to be in.
        #[derive(Deserialize)]
        struct QueryDecode {
            /// The parameter.
            i: String
        }
        serde_urlencoded::from_str::<QueryDecode>(query)
            .ok()
            .map(|q| q.i)
    }

    /// Generate a response to the given request. Wrap the response
    /// in `Ok(_)` if it was successful, and in `Err(_)` if it was not.
    async fn try_respond(&self, req: Request<Body>) -> Result<Response<Body>> {
        // Return an error if we somehow get a URI that doesn't have a path.
        let (head, body) = req.into_parts();
        let uri = head.uri.into_parts();
        let path_and_query = match uri.path_and_query {
            None => self.error_500("request URL does not contain a path")?,
            Some(pnq) => pnq,
        };

        // Parse query strings if they are present.
        let path = path_and_query.path().trim_matches('/').to_owned();
        let param = path_and_query.query().and_then(Self::get_query_param);

        if head.method == Method::GET {
            if let Some(path) = utils::strip_prefix(&path, "static/") {
                let path = format!("static/public/{}", path);
                self.serve_file(&path).await
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
                    _ => self.error_404(),
                }
            }
        } else if head.method == Method::POST {
            let body = utils::read_body(body).await
                .or_else(|e| self.error_500(format_args!(
                    "could not read request body: {:?}",
                    e,
                )))?;
            match path.as_str() {
                "login" => self.login(body),
                _ => self.error_404(),
            }
        } else {
            self.error_404()
        }
    }
}

#[async_trait]
impl Respond for AppState {
    async fn respond(&self, _: SocketAddr, req: Request<Body>) -> Response<Body> {
        match self.try_respond(req).await {
            Ok(resp) => resp,
            Err(resp) => resp,
        }
    }
    fn shutdown_on_err(&self, err: hyper::Error) {
        self.log.err(format_args!("hyper shut down: {:?}", err))
    }
}