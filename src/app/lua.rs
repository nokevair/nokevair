//! Use `rlua` to start a Lua instance and permit other tasks to query it.

use hyper::{Response, Body};
use parking_lot::RwLock;
use rlua::{Lua, RegistryKey};
use tokio::sync::{mpsc, oneshot};
use vec_map::VecMap;

use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::conv;
use crate::utils::SourceChain;
use super::{Ctx, Result, AppState};

pub mod render;

pub mod sim;

mod version;
pub use version::Version;

/// Create a new Lua instance with several predefined functions.
fn create_lua_state(app_ctx: &Ctx) -> Lua {
    let lua = Lua::new();
    lua.context(|ctx| {
        let globals = ctx.globals();

        macro_rules! define_function {
            ($name:expr, $def:expr) => {{
                let res = ctx.create_function($def)
                    .and_then(|func| globals.set($name, func));
                if let Err(e) = res {
                    app_ctx.log.err(format_args!(
                        "lua (creating function '{}'):\n{}",
                        $name,
                        SourceChain(e)
                    ));
                }
            }}
        }

        let app_ctx_clone = app_ctx.clone();
        define_function!("log", move |_, s: String| {
            app_ctx_clone.log.lua(s);
            Ok(())
        });

        define_function!("pretty", move |_, v: rlua::Value| {
            Ok(conv::lua_to_string(v))
        });

        define_function!("rand", |_, n: u64| {
            if n == 0 {
                Err(rlua::Error::RuntimeError(String::from("rand bound must be nonzero")))
            } else {
                use rand::Rng as _;
                Ok(rand::thread_rng().gen_range(0, n))
            }
        });

        define_function!("sleep", |_, ms: u64| {
            thread::sleep(Duration::from_millis(ms));
            Ok(())
        });
    });
    lua
}

/// Represents a request that can be sent to the Lua task.
enum Req {
    /// A request to re-read and re-execute all `focus.lua` files.
    /// Does not expect a response.
    ReloadFocuses,
    /// A request to invoke the renderer to load a specific page.
    /// Expects that page as a response.
    Render {
        /// The version of the state to use
        ver: Version,
        /// The page to be rendered (e.g. `people`)
        name: String,
        /// The value of the `i` parameter passed in the URL
        /// via query string, if present
        query_param: Option<String>,
        /// The channel over which to send a response.
        resp_tx: oneshot::Sender<Response<Body>>,
    },
    /// A request to return the number of focuses that have been loaded.
    /// Expects a `usize` as a response.
    GetNumFocuses {
        /// The channel over which to send a response.
        resp_tx: oneshot::Sender<usize>,
    },
    /// A request to return the number of states that have been loaded.
    /// Expects a `usize` as a response.
    GetNumStates {
        /// The channel over which to send a response.
        resp_tx: oneshot::Sender<usize>,
    }
}

/// The sending half of the request channel.
type Tx = mpsc::Sender<Req>;
/// The receiving half of the request channel.
type Rx = mpsc::Receiver<Req>;

/// Provides async convenience methods for sending requests over
/// the channel and receiving responses.
pub struct Frontend {
    /// The connection to the Lua task.

    // TODO: determine whether it would be more efficient to
    // keep this in a RwLock rather than cloning it on every
    // message we send.
    tx: Tx,
    /// The value returned by `num_focuses()` when it was last called,
    /// or `None` if that has since been invalidated.
    num_focuses: RwLock<Option<usize>>,
}

impl Frontend {
    /// Create the frontend.
    fn new(tx: Tx) -> Self {
        Self {
            tx,
            num_focuses: RwLock::default(),
        }
    }

    /// Send a request to the backend to re-read and re-execute
    /// all `focus.lua` files. Do not wait for a response.
    pub async fn reload_focuses(&self, ctx: &Ctx) {
        if self.tx.clone().send(Req::ReloadFocuses).await.is_err() {
            ctx.log.err("backend is not running");
        } else {
            // The number of focuses may have changed, so we must
            // invalidate the cached value.
            *self.num_focuses.write() = None;
        }
    }

    /// Send a request to the backend to render a particular state view.
    /// Wait for a response and then return it.
    pub async fn render(
        &self,
        ver: Version,
        name: String,
        query_param: Option<String>,
    ) -> Option<Response<Body>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = Req::Render { ver, name, query_param, resp_tx };
        self.tx.clone().send(req).await.ok()?;
        resp_rx.await.ok()
    }
    
    /// Return the number of focuses.
    pub async fn num_focuses(&self, ctx: &Ctx) -> usize {
        let cached = *self.num_focuses.read();
        match cached {
            Some(n) => n,
            None => {
                let (resp_tx, resp_rx) = oneshot::channel();
                let req = Req::GetNumFocuses { resp_tx };
                if self.tx.clone().send(req).await.is_err() {
                    ctx.log.err("backend is not running");
                    0
                } else if let Ok(n) = resp_rx.await {
                    *self.num_focuses.write() = Some(n);
                    n
                } else {
                    ctx.log.err("backend is not running");
                    0
                }
            },
        }
    }

    /// Return the number of states.
    pub async fn num_states(&self, ctx: &Ctx) -> usize {
        let (resp_tx, resp_rx) = oneshot::channel();
        let req = Req::GetNumStates { resp_tx };
        if self.tx.clone().send(req).await.is_err() {
            ctx.log.err("backend is not running");
            0
        } else if let Ok(n) = resp_rx.await {
            n
        } else {
            ctx.log.err("backend is not running");
            0
        }
    }
}

/// The state held by the Lua backend thread.
pub struct Backend {
    /// The main `Lua` instance.
    lua: Lua,
    /// The versions of the world state currently loaded in the registry.
    state_versions: VecMap<RegistryKey>,
    /// The channel from which to receive requests.
    rx: Rx,
    /// The functions compiled from `render/*/focus.lua` files.
    focuses: HashMap<String, RegistryKey>,
}

impl Backend {
    /// Create the backend.
    fn new(rx: Rx, ctx: &Ctx) -> Self {
        let mut self_ = Self {
            lua: create_lua_state(ctx),
            state_versions: VecMap::new(),
            rx,
            focuses: HashMap::new(),
        };
        self_.load_focuses(ctx);
        self_
    }

    /// Attempt to read a Message value from the file corresponding to the
    /// specified version, convert it to a Lua object, and put it in the registry.
    fn load_from_file(&self, ver: Version, app_ctx: &Ctx) -> Option<RegistryKey> {
        let path = ver.path(app_ctx);

        app_ctx.log.info(format_args!("loading lua state from file '{}'", path.display()));

        if !Path::new(&path).exists() {
            app_ctx.log.err("file does not exist");
            return None;
        }

        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(e) => {
                app_ctx.log.err(format_args!("file could not be opened: {}", e));
                return None
            }
        };

        let mpv = match conv::bytes_to_msgpack(&mut file) {
            Ok(file) => file,
            Err(e) => {
                app_ctx.log.err(format_args!("file could not be read as msgpack: {}", e));
                return None
            }
        };

        self.lua.context(|ctx| {
            let lv = match conv::msgpack_to_lua(mpv, ctx) {
                Ok(lv) => lv,
                Err(e) => {
                    app_ctx.log.err(format_args!(
                        "lua (msgpack -> obj):\n{}",
                        SourceChain(e)
                    ));
                    return None
                }
            };

            match ctx.create_registry_value(lv) {
                Ok(key) => Some(key),
                Err(e) => {
                    app_ctx.log.err(format_args!(
                        "lua (obj -> registry):\n{}",
                        SourceChain(e)
                    ));
                    None
                }
            }
        })
    }

    /// If a particular version of the state has not been loaded, attempt to
    /// load it.
    fn ensure_loaded(&mut self, ver: Version, ctx: &Ctx) {
        let idx = ver.as_usize();
        if !self.state_versions.contains_key(idx) {
            if let Some(key) = self.load_from_file(ver, ctx) {
                self.state_versions.insert(idx, key);
            }
        }
    }

    /// Create a future that continuously handles requests until the `Frontend` is dropped.
    pub async fn run(&mut self, app_state: &AppState) {
        while let Some(req) = self.rx.recv().await {
            // Warning: when using `app_state` here, keep in mind that there are currently
            // other tasks blocking on receiving a response from here, so there is
            // a risk of deadlocks.
            match req {
                Req::ReloadFocuses => {
                    self.unload_focuses();
                    self.load_focuses(&app_state.ctx);
                }

                Req::Render { ver, name, query_param, resp_tx } => {
                    let resp = match self.render(ver, &name, query_param, app_state) {
                        Ok(resp) => resp,
                        Err(resp) => resp,
                    };
                    if resp_tx.send(resp).is_err() {
                        app_state.ctx.log.err("couldn't send response to render request");
                    }
                }

                Req::GetNumFocuses { resp_tx } => {
                    if resp_tx.send(self.focuses.len()).is_err() {
                        app_state.ctx.log.err("couldn't send response to request for focuses");
                    }
                }

                Req::GetNumStates { resp_tx } => {
                    if resp_tx.send(self.state_versions.len()).is_err() {
                        app_state.ctx.log.err("couldn't send response to request for states");
                    }
                }
            }
        }
    }
}

/// Create a new frontend and backend.
pub fn init(ctx: &Ctx) -> (Frontend, Backend) {
    let (tx, rx) = mpsc::channel(100);
    (Frontend::new(tx), Backend::new(rx, ctx))
}
