//! Defines the application state and describes how it can be used
//! to serve requests and do various other tasks.

use async_trait::async_trait;
use hyper::{Request, Response, Body, Method};
use parking_lot::RwLock;
use serde::{Serialize, Deserialize};
use tera::Context;
use tokio::time::{Duration, Instant, interval, delay_for};

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;

use crate::hyper_boilerplate::Respond;
use crate::utils;

mod ctx;
pub use ctx::Ctx;
use ctx::log;

mod error;
use error::Result;

mod lua;
pub use lua::Backend as LuaBackend;
use lua::sim::Sim;

mod templates;
use templates::Templates;

mod login;
mod responses;

/// Contains all state used by the application in a
/// concurrently-accessible format.
pub struct AppState {
    /// When was the server initialized?
    start_time: Instant,
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
            start_time: Instant::now(),
            templates: RwLock::new(Templates::load(&ctx)),
            login_tokens: RwLock::default(),
            lua: frontend,
            sim: Sim::new(),
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
        self.delay().await;
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
            ["about"] => self.try_render("about.html", &Context::new()),
            ["login"] => {
                let token = self.gen_login_token();
                let mut context = Context::new();
                context.insert("token", &token);
                self.render("login.html", &context)
            }
            ["blog"] => self.serve_blog_index(),
            ["blog", id] => {
                self.try_render(&format!("blog/{}.html", id), &Context::new())
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

                ctx.insert("num_blogs", &self.ctx.blog.ids().len());
                ctx.insert("num_focuses", &self.lua.num_focuses(&self.ctx).await);
                ctx.insert("num_templates", &self.num_templates());
                ctx.insert("template_refresh",
                    &self.ctx.cfg.runtime.template_refresh.load(Ordering::Relaxed));
                ctx.insert("sim_file",
                    &*self.ctx.cfg.runtime.sim_file.read());
                ctx.insert("sim_rate",
                    &self.ctx.cfg.runtime.sim_rate.load(Ordering::Relaxed));
                ctx.insert("num_states", &self.lua.num_states(&self.ctx).await);
                ctx.insert("uptime", &self.start_time.elapsed().as_secs());

                self.render("admin/index.html", &ctx)
            }
            ["sim_files"] => {
                let mut ctx = Context::new();

                ctx.insert("files", &lua::sim::list_files(&self.ctx));
                ctx.insert("active",
                    &*self.ctx.cfg.runtime.sim_file.read());

                self.render("admin/sim_files.html", &ctx)
            }
            ["sim_files", name] => {
                if lua::sim::is_valid_name(&name) {
                    let path = self.ctx.cfg.paths.sim.join(name);
                    self.serve_file(&path).await
                } else {
                    self.error_404()
                }
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
            ["reload_blog"] => {
                self.ctx.reload_blog();
                Ok(Self::empty_200())
            }
            ["reload_templates"] => {
                self.reload_templates();
                Ok(Self::empty_200())
            }
            ["reload_focuses"] => {
                self.lua.reload_focuses(&self.ctx).await;
                Ok(Self::empty_200())
            }
            ["update_template_refresh"] => {
                if let Some(new) = utils::parse_bytes(body) {
                    let old = self.ctx.cfg.runtime.template_refresh.swap(new, Ordering::Relaxed);
                    if new != old {
                        self.ctx.log.info(format_args!("changed template refresh to {}", new));
                    }
                    Ok(Self::empty_200())
                } else {
                    self.error_400()
                }
            }
            ["update_sim_rate"] => {
                if let Some(new) = utils::parse_bytes(body) {
                    let old = self.ctx.cfg.runtime.sim_rate.swap(new, Ordering::Relaxed);
                    if new != old {
                        self.ctx.log.info(format_args!("changed sim rate to {}", new));
                    }
                    Ok(Self::empty_200())
                } else {
                    self.error_400()
                }
            }
            ["delete_message"] => {
                if let Some(idx) = utils::parse_bytes(body) {
                    self.ctx.log.toggle_deleted(idx);
                    Ok(Self::empty_200())
                } else {
                    self.error_400()
                }
            }
            ["filter_log"] => self.serve_filter_log(&body),
            ["update_sim_file"] => {
                if let Ok(body) = String::from_utf8(body) {
                    if lua::sim::is_valid_name(&body) {
                        *self.ctx.cfg.runtime.sim_file.write() = body.into();
                        Ok(Self::empty_200())
                    } else {
                        self.error_400()
                    }
                } else {
                    self.error_400()
                }
            }
            _ => self.error_404(),
        }
    }

    /// Simulate a connection with high latency by waiting for a number of
    /// milliseconds specified in the config file.
    async fn delay(&self) {
        if let Some(latency) = self.ctx.cfg.latency {
            delay_for(Duration::from_millis(latency as u64)).await
        }
    }

    /// Generate a response to a GET request to the path "/blog".
    fn serve_blog_index(&self) -> Result<Response<Body>> {
        /// Describes how posts are serialized when passing them to Tera.
        #[derive(Serialize)]
        struct TeraPost {
            /// The ID of the post.
            id: String,
            /// The title of the post.
            title: String,
            /// The date that the post was published.
            date: String,
        }
        let mut posts = Vec::new();
        for id in self.ctx.blog.ids().iter().rev() {
            let post = match self.ctx.blog.metadata(id) {
                Some(post) => post,
                None => {
                    self.ctx.log.err("impossible - bad post id");
                    continue
                }
            };
            posts.push(TeraPost {
                id: id.to_owned(),
                title: post.title.clone(),
                date: post.date.clone()
            });
        }
        let mut context = Context::new();
        context.insert("posts", &posts);
        self.render("blog_index.html", &context)
    }

    /// Generate a response to a POST request to the path "/admin/filter_log".
    fn serve_filter_log(&self, body: &[u8]) -> Result<Response<Body>> {
        /// Describes how log messages are serialized when passing them to Tera.
        #[derive(Serialize)]
        struct TeraLogMsg {
            /// The index of the message in the log.
            idx: usize,
            /// The rest of the fields associated with messages.
            #[serde(flatten)]
            msg: log::Message,
        }
        let filter = log::Filter::from_body(body);
        let mut messages = Vec::new();
        self.ctx.log.for_each(|idx, msg| {
            if filter.permits(msg) {
                messages.push(TeraLogMsg { idx, msg: msg.clone() });
            }
        });
        let mut context = Context::new();
        context.insert("messages", &messages);
        self.render("admin/filtered_log.html", &context)
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
