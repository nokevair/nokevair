//! Defines the application state and describes how it can be used
//! to serve requests and do various other tasks.

use async_trait::async_trait;
use hyper::{Request, Response, Body, Method};
use serde::Deserialize;
use tera::Context;
use tokio::time::{Duration, Instant, interval};

use std::collections::HashMap;
use std::net::SocketAddr;
// TODO: with some performance testing, maybe switch to parking_lot?
use std::sync::{RwLock, atomic::Ordering};

use crate::hyper_boilerplate::Respond;
use crate::utils;

mod ctx;
pub use ctx::Ctx;

mod error;
use error::Result;

mod lua;
use lua::with_renderer_entries;
pub use lua::Backend as LuaBackend;
use lua::Sim;

mod templates;
use templates::Templates;

mod login;
mod responses;

/// Contains all state used by the application in a
/// concurrently-accessible format.
pub struct AppState {
    /// Contains data used to render templates.
    templates: RwLock<Templates>,
    /// Tokens used by `/login` to authenticate the user.
    login_tokens: RwLock<HashMap<u64, Instant>>,
    /// Permits interaction with the task running the Lua renderer instance.
    lua: lua::Frontend,
    /// Permits interaction with the Lua simulation program.
    sim: Sim,
    /// Context data used throughout the application (config and logging).
    ctx: Ctx,
}

impl AppState {
    /// Initialize the state.
    pub fn new(ctx: Ctx) -> (LuaBackend, Self) {
        let (frontend, backend) = lua::init(&ctx);
        (backend, Self {
            templates: RwLock::new(Templates::load(&ctx)),
            login_tokens: RwLock::default(),
            lua: frontend,
            sim: Sim::new(&ctx),
            ctx,
        })
    }

    /// Perform various bookkeeping tasks at regular intervals.
    pub async fn do_scheduled(&self) {
        let cfg = &self.ctx.cfg;
        
        let mut interval = interval(Duration::from_secs(1));
        let mut i = 0u64;
        loop {
            interval.tick().await;
            i += 1;

            macro_rules! at_interval {
                ($t:expr => $body:expr) => {
                    if i.checked_rem($t as u64) == Some(0) { $body }
                }
            }

            at_interval!(cfg.runtime.template_refresh.load(Ordering::Relaxed)
                => self.reload_templates());
            at_interval!(cfg.runtime.sim_rate.load(Ordering::Relaxed)
                => self.sim.run(self.ctx.clone()));
            at_interval!(cfg.security.auth_sweep => self.clear_login_tokens());
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
                    "could not read request body: {}",
                    e,
                )))?;
            self.handle_post_request(&path, body).await
        } else {
            self.error_404()
        }
    }

    /// Generate a response to a GET request to the given path.
    async fn handle_get_request(
        &self,
        path: &[&str],
        param: Option<String>,
    ) -> Result<Response<Body>> {
        match path {
            ["static", file] => {
                let file_path = path!(&self.ctx.cfg.paths.static_, "public", file);
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
                self.lua.render(ver, String::from(*name), param).await
                    .ok_or(())
                    .or_else(|_| self.error_500("backend is not running"))
            } else {
                self.error_404()
            }
            _ => self.error_404(),
        }
    }

    /// Generate a response to a GET request to a path that starts with `/admin`.
    async fn handle_admin_get_request(&self, path: &[&str]) -> Result<Response<Body>> {
        // TODO: put this behind some kind of authentication barrier
        match path {
            ["static", file] => {
                let file_path = path!(&self.ctx.cfg.paths.static_, "admin", file);
                self.serve_file(&file_path).await
            }
            [] => {
                let mut ctx = Context::new();

                ctx.insert("num_focuses", &self.lua.num_focuses(&self.ctx).await);
                ctx.insert("num_templates", &self.num_templates());

                ctx.insert("template_refresh",
                    &self.ctx.cfg.runtime.template_refresh.load(Ordering::Relaxed));

                self.render("admin/index.html", &ctx)
            }
            _ => self.error_404(),
        }
    }

    /// Generate a response to a POST request to the given path.
    async fn handle_post_request(
        &self,
        path: &[&str],
        body: Vec<u8>,
    ) -> Result<Response<Body>> {
        match path {
            ["login"] => self.login(body),
            ["admin", path @ ..] => self.handle_admin_post_request(path, body).await,
            _ => self.error_404(),
        }
    }

    /// Generate a response to a POST request to a path that starts with `/admin`.
    async fn handle_admin_post_request(
        &self,
        path: &[&str],
        body: Vec<u8>,
    ) -> Result<Response<Body>> {
        // TODO: put this behind some kind of authentication barrier
        match path {
            ["reload_templates"] => {
                self.reload_templates();
                Ok(Self::empty_200())
            }
            ["reload_focuses"] => {
                self.lua.reload_focuses(&self.ctx).await;
                Ok(Self::empty_200())
            }
            ["update_template_refresh"] => {
                if let Some(new) = utils::parse_u32(body) {
                    let old = self.ctx.cfg.runtime.template_refresh.swap(new, Ordering::Relaxed);
                    if new != old {
                        self.ctx.log.info(format_args!("changed template refresh to {}", new));
                    }
                    Ok(Self::empty_200())
                } else {
                    self.error_400()
                }
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
        self.ctx.log.err(format_args!("hyper shut down: {}", err))
    }
}
