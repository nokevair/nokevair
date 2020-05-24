//! Exposes a `Ctx` struct which provides various functions with shared access
//! to data and functionality that is widely used throughout the server.

use std::sync::Arc;

mod log;
pub use log::Log;

mod cfg;
pub use cfg::Cfg;

/// Provides a shared, cloneable handle to the log and config information.
#[derive(Clone)]
pub struct Ctx {
    /// A handle to the log.
    pub log: Arc<Log>,
    /// A handle to the app configuration.
    pub cfg: Arc<Cfg>,
}

impl Ctx {
    /// Initialize the context, loading config from a command-line argument.
    pub fn load() -> Option<Self> {
        let log = Log::new();
        let cfg = Cfg::load(&log)?;
        Some(Self {
            cfg: Arc::new(cfg),
            log: Arc::new(log),
        })
    }
}
