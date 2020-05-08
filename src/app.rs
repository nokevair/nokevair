//! Defines the application state and describes how it can be used
//! to serve requests and do various other tasks.

use async_trait::async_trait;
use hyper::{Request, Response, Body, Method};
use serde::Deserialize;
use tera::{Tera, Context};
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

pub mod lua;
use lua::with_renderer_entries;

mod login;
mod responses;
mod templates;

/// Parse `"10/foo"` into `(10, "foo")`.
fn parse_version_and_name(s: &str) -> Option<(lua::Version, String)> {
    let slash = s.find('/')?;
    let (ver, path) = s.split_at(slash);
    let (_slash, name) = path.split_at(1);
    let ver = ver.parse().ok()?;
    Some((ver, name.to_string()))
}

/// Contains all state used by the application in a
/// concurrently-accessible format.
pub struct AppState {
    /// The `Tera` instance used to render templates.
    templates: RwLock<Tera>,
    /// Tokens used by `/login` to authenticate the user.
    login_tokens: RwLock<HashMap<u64, Instant>>,
    /// The list of log messages.
    log: Log,
    /// Permits interaction with the task running the Lua instance.
    lua: lua::Frontend,
}

impl AppState {
    /// Initialize the state.
    pub fn new() -> (lua::Backend, Self) {
        let log = Log::new();
        let (frontend, backend) = lua::init(&log);
        (backend, Self {
            templates: RwLock::new(templates::load(&log)),
            login_tokens: RwLock::default(),
            log,
            lua: frontend,
        })
    }

    /// Perform various bookkeeping tasks at regular intervals.
    pub async fn do_scheduled(&self) {
        let mut interval = interval(Duration::from_secs(1));
        let mut i = 0u64;
        loop {
            interval.tick().await;
            i += 1;
            // TODO: add an option not to continuously reload
            // the templates, since we've added a button for it
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
        let param = path_and_query.query().and_then(Self::get_query_param);

        // Parse the request path into its components.
        let path = path_and_query.path()
            .trim_matches('/')
            .split('/')
            .collect::<Vec<_>>();

        if head.method == Method::GET {
            self.handle_get_request(&path, param).await
        } else if head.method == Method::POST {
            let body = utils::read_body(body).await
                .or_else(|e| self.error_500(format_args!(
                    "could not read request body: {:?}",
                    e,
                )))?;
            self.handle_post_request(&path, body).await
        } else {
            self.error_404()
        }
    }

    async fn handle_get_request(
        &self,
        path: &[&str],
        param: Option<String>,
    ) -> Result<Response<Body>> {
        match path {
            ["static", file] => {
                let file_path = format!("static/public/{}", file);
                self.serve_file(&file_path).await
            }
            ["about"] => self.render("about.html", &Context::new()),
            ["login"] => {
                let token = self.gen_login_token();
                let mut context = Context::new();
                context.insert("token", &token);
                self.render("login.html", &context)
            }
            ["admin", path @ ..] => self.handle_admin_get_request(path).await,
            [ver, name] => if let Ok(ver) = ver.parse() {
                self.lua.render(ver, name.to_string(), param).await
                    .ok_or(())
                    .or_else(|_| self.error_500("backend is not running"))
            } else {
                self.error_404()
            }
            _ => self.error_404(),
        }
    }

    async fn handle_admin_get_request(&self, path: &[&str]) -> Result<Response<Body>> {
        // TODO: put this behind some kind of authentication barrier
        match path {
            // TODO: now that we aren't parsing URIs really stupidly, we can
            // rename this route to `[]`.
            ["index"] => self.render("admin/index.html", &Context::new()),
            _ => self.error_404(),
        }
    }

    async fn handle_post_request(
        &self,
        path: &[&str],
        body: Vec<u8>,
    ) -> Result<Response<Body>> {
        match path {
            ["login"] => self.login(body),
            ["admin", path @ ..] => self.handle_admin_post_request(path).await,
            _ => self.error_404(),
        }
    }

    async fn handle_admin_post_request(&self, path: &[&str]) -> Result<Response<Body>> {
        match path {
            ["reload_templates"] => {
                self.reload_templates();
                Ok(Self::empty_200())
            }
            ["reload_focuses"] => {
                self.lua.reload_focuses(&self.log).await;
                Ok(Self::empty_200())
            }
            _ => self.error_404(),
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
