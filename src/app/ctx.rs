//! Exposes a `Ctx` struct which provides various functions with shared access
//! to data and functionality that is widely used throughout the server.
//! 
//! Most functions that require the context will take it as a parameter called `ctx`.
//! In cases where this would be shadowed by the RLua context, it is instead called
//! `app_ctx`.

use std::sync::Arc;

mod blog;
pub use blog::Blog;

mod cfg;
pub use cfg::Cfg;

pub mod log;
pub use log::Log;

/// Provides a shared, cloneable handle to the log and config information.
#[derive(Clone)]
pub struct Ctx {
    /// A handle to the blog descriptor.
    pub blog: Arc<Blog>,
    /// A handle to the app configuration.
    pub cfg: Arc<Cfg>,
    /// A handle to the log.
    pub log: Arc<Log>,
}

impl Ctx {
    /// Initialize the context, loading config from a command-line argument.
    pub fn load() -> Option<Self> {
        let log = Log::new();
        let cfg = Cfg::load(&log)?;
        let blog = Blog::load(&log, &cfg)?;
        Some(Self {
            blog: Arc::new(blog),
            cfg: Arc::new(cfg),
            log: Arc::new(log),
        })
    }
}
