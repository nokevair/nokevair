//! Use `rlua` to start a Lua instance and permit other tasks to query it.

use rlua::{Lua, RegistryKey};
use tokio::sync::mpsc;
use vec_map::VecMap;

use std::path::Path;
use std::fs::{self, File};

use crate::conv;
use super::Log;

/// Represents the ID of a particular version of the world state.
type Version = u32;

/// Represents a request that can be sent to the Lua task.
enum Req {
    /// A request to execute the contents of `test.lua` with the specified
    /// version of the world state. Does not expect a response.
    RunTest(Version),
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
}

/// The state held by the Lua backend thread.
pub struct Backend {
    /// The main `Lua` instance.
    lua: Lua,
    /// The versions of the world state currently loaded in the registry.
    state_versions: VecMap<RegistryKey>,
    /// The channel from which to receive requests.
    rx: Rx,
}

impl Backend {
    /// Create the backend.
    fn new(rx: Rx) -> Self {
        Self {
            lua: Lua::new(),
            state_versions: VecMap::new(),
            rx,
        }
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
    pub async fn run(&mut self, app_state: &super::AppState) {
        while let Some(req) = self.rx.recv().await {
            // Warning: when using `app_state` here, keep in mind that there are currently
            // other tasks blocking on receiving a response from here, so there is
            // a risk of deadlocks.
            match req {
                Req::RunTest(ver) => {
                    self.ensure_loaded(ver, &app_state.log);
                    self.run_test(ver, &app_state.log);
                }
            }
        }
    }
}

/// Create a new frontend and backend.
pub fn init() -> (Frontend, Backend) {
    let (tx, rx) = mpsc::channel(100);
    (Frontend::new(tx), Backend::new(rx))
}
