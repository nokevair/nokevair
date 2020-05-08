//! Use `rlua` to start a Lua instance and permit other tasks to query it.

use rlua::{Lua, RegistryKey};
use tokio::sync::{mpsc, oneshot};
use vec_map::VecMap;

use hyper::{Response, Body};

use std::collections::HashMap;
use std::fs::{self, File};
use std::path::Path;

use crate::conv;
use super::{Log, Result, AppState};

mod render;
pub(super) use render::with_renderer_entries;

/// Represents the ID of a particular version of the world state.
pub type Version = u32;

/// Represents a request that can be sent to the Lua task.
enum Req {
    /// A request to execute the contents of `test.lua` with the specified
    /// version of the world state. Does not expect a response.
    RunTest(Version),
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
    }
    // TODO: add another variant for reloading the entire focus dict
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
}

impl Frontend {
    /// Create the frontend.
    fn new(tx: Tx) -> Self {
        Self { tx }
    }

    /// Send a request to the backend to execute the contents of `test.lua`
    /// on version zero of the world state. Do not wait for this to complete.
    pub async fn run_test_0(&self, log: &Log) {
        if self.tx.clone().send(Req::RunTest(0)).await.is_err() {
            log.err("backend is not running");
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
    fn new(rx: Rx, log: &Log) -> Self {
        let mut self_ = Self {
            // TODO: make this into a separate function so we can add helper
            // functions and reload things more easily
            lua: Lua::new(),
            state_versions: VecMap::new(),
            rx,
            focuses: HashMap::new(),
        };
        self_.load_focuses(log);
        self_
    }

    /// Attempt to read a Message value from the file corresponding to the
    /// specified version, convert it to a Lua object, and put it in the registry.
    fn load_from_file(&self, ver: Version, log: &Log) -> Option<RegistryKey> {
        let path = format!("state/{}.msgpack", ver);

        log.info(format_args!("trying to load lua state from file '{}'", path));

        if !Path::new(&path).exists() {
            log.err("file does not exist");
            return None;
        }

        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(e) => {
                log.err(format_args!("file could not be opened: {:?}", e));
                return None
            }
        };

        let mpv = match conv::bytes_to_msgpack(&mut file) {
            Ok(file) => file,
            Err(e) => {
                log.err(format_args!("file could not be read as msgpack: {:?}", e));
                return None
            }
        };

        self.lua.context(|ctx| {
            let lv = match conv::msgpack_to_lua(mpv, ctx) {
                Ok(lv) => lv,
                Err(e) => {
                    log.err(format_args!("file could not be converted to lua object: {:?}", e));
                    return None
                }
            };

            match ctx.create_registry_value(lv) {
                Ok(key) => {
                    log.info("successfully loaded");
                    Some(key)
                }
                Err(e) => {
                    log.err(format_args!("lua object could not be added to registry: {:?}", e));
                    None
                }
            }
        })
    }

    /// If a particular version of the state has not been loaded, attempt to
    /// load it.
    fn ensure_loaded(&mut self, ver: Version, log: &Log) {
        if !self.state_versions.contains_key(ver as usize) {
            if let Some(key) = self.load_from_file(ver, log) {
                self.state_versions.insert(ver as usize, key);
            }
        }
    }

    /// Run the contents of `test.lua`.
    fn run_test(&mut self, ver: Version, log: &Log) {
        let code = match fs::read_to_string("test.lua") {
            Ok(code) => code,
            Err(e) => {
                log.err(format_args!("could not read `test.lua`: {:?}", e));
                return
            }
        };

        let res = self.lua.context(|ctx| {
            let state = match self.state_versions.get(ver as usize) {
                Some(key) => ctx.registry_value::<rlua::Value>(key)?,
                None => return Ok(()),
            };

            ctx.load(&code)
                .set_name("test.lua")?
                .eval::<rlua::Function>()?
                .call(state)
        });

        if let Err(e) = res {
            log.err(format_args!("error while running `test.lua`: {:?}", e));
        }
    }

    /// Create a future that continuously handles requests until the `Frontend` is dropped.
    pub async fn run(&mut self, app_state: &AppState) {
        while let Some(req) = self.rx.recv().await {
            // Warning: when using `app_state` here, keep in mind that there are currently
            // other tasks blocking on receiving a response from here, so there is
            // a risk of deadlocks.
            match req {
                Req::RunTest(ver) => {
                    self.ensure_loaded(ver, &app_state.log);
                    self.run_test(ver, &app_state.log);
                }

                Req::Render { ver, name, query_param, resp_tx } => {
                    let resp = match self.render(ver, &name, query_param, app_state) {
                        Ok(resp) => resp,
                        Err(resp) => resp,
                    };
                    if let Err(e) = resp_tx.send(resp) {
                        app_state.log.err(format_args!(
                            "couldn't send response to error request: {:?}",
                            e
                        ));
                    }
                }
            }
        }
    }
}

/// Create a new frontend and backend.
pub fn init(log: &Log) -> (Frontend, Backend) {
    let (tx, rx) = mpsc::channel(100);
    (Frontend::new(tx), Backend::new(rx, log))
}
