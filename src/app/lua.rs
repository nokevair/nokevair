//! Use `rlua` to start a Lua instance and permit other tasks to query it.

use tokio::sync::mpsc;

use rlua::Lua;

/// Represents a request that can be sent to the Lua task.
pub enum Req {
    /// A request to execute a block of code. Does not expect a response.
    Exec(String),
}

/// The sending half of the request channel.
pub type Tx = mpsc::Sender<Req>;
/// The receiving half of the request channel.
pub type Rx = mpsc::Receiver<Req>;

/// Provides async convenience methods for sending requests over
/// the channel and receiving responses.
pub struct Frontend {
    /// The connection to the Lua task.
    tx: Tx,
}

impl Frontend {
    /// Create the frontend.
    pub fn new(tx: Tx) -> Self {
        Self { tx }
    }

    /// Send a request to execute a block of code. Do not wait for a response.
    pub async fn exec(&self, log: &super::Log, code: String) {
        // TODO: Is it more efficient to clone the sender here or
        // to keep it in an Arc? Performance testing may be needed.
        if let Err(e) = self.tx.clone().send(Req::Exec(code)).await {
            log.err(format_args!("failed to execute code: {}", e));
        }
    }
}

impl super::AppState {
    /// Create a `Lua` instance and handle requests to manipulate it.
    pub async fn run_lua(&self, mut rx: Rx) {
        let lua = Lua::new();
        while let Some(req) = rx.recv().await {
            match req {
                Req::Exec(code) => {
                    let res = lua.context(|ctx| {
                        ctx.load(&code).exec()
                    });
                    if let Err(e) = res {
                        self.log.err(format_args!("error while executing lua code: {:?}", e))
                    }
                }
            }
        }
    }
}
