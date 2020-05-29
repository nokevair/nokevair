//! Defines the format of `Config.toml` files.

use serde::Deserialize;

use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{RwLock, atomic::AtomicU32};

use super::Log;

/// Contains all config information (deserialized from a TOML file given as
/// a command-line argument).
#[derive(Deserialize, Debug)]
pub struct Cfg {
    /// The `addr` field of the config file.
    pub addr: SocketAddr,
    /// The `latency` field of the config file.
    pub latency: Option<u16>,
    /// The `[runtime]` section of the config file.
    pub runtime: Runtime,
    /// The `[paths]` section of the config file.
    pub paths: Paths,
    /// The `[security]` section of the config file.
    pub security: Security,
}

/// Represents parts of the config that are mutably shared so they can
/// be configured at runtime via the admin panel.
#[derive(Deserialize, Debug)]
pub struct Runtime {
    /// How frequently do we reload templates? If this is zero, then never.
    #[serde(rename="template-refresh")]
    pub template_refresh: AtomicU32,
    /// The path to the file containing the Lua simulation code.
    #[serde(rename="sim-file")]
    pub sim_file: RwLock<PathBuf>,
    /// How frequently do we run the simulation? If this is zero, then never.
    #[serde(rename="sim-rate")]
    pub sim_rate: AtomicU32,
}

/// The part of the config that provides paths to directories where certain
/// files are found and stored.
#[derive(Deserialize, Debug)]
pub struct Paths {
    /// Renderer files.
    pub render: PathBuf,
    /// Simulation files.
    pub sim: PathBuf,
    /// State files.
    pub state: PathBuf,
    /// Templates that aren't part of the renderer.
    pub templates: PathBuf,
    /// Static files.
    #[serde(rename="static")]
    pub static_: PathBuf,
}

/// The part of the config that provides various parameters relating to authentication.
#[derive(Deserialize, Debug)]
pub struct Security {
    /// For how many seconds is a login challenge token considered valid?
    #[serde(rename="auth-timeout")]
    pub auth_timeout: u32,
    /// How frequently do we sweep the login challenge token list for outdated entries?
    #[serde(rename="auth-sweep")]
    pub auth_sweep: u32,
    /// The password used to access the admin panel.
    /// (This is not read from the config file, but instead via environment variable.)
    #[serde(skip)]
    pub login_password: Option<String>,
}

/// Attempt to read the admin password from the `PW` environment variable.
fn get_admin_password(log: &Log) -> Option<String> {
    let pw = env::var("PW").ok();
    if pw.is_none() {
        log.info("specify admin password via PW environment variable to enable admin login");
    }
    pw
}

impl Cfg {
    /// Load config from the TOML file passed as a command-line argument.
    pub fn load(log: &Log) -> Option<Self> {
        let mut args = env::args().skip(1);
        let path = PathBuf::from(match args.next() {
            Some(p) => p,
            None => { log.err("no config file specified"); return None }
        });
        let contents = match fs::read(&path) {
            Ok(c) => c,
            Err(e) => { log.err(format_args!("while reading config file: {}", e)); return None }
        };
        let mut self_: Self = match toml::from_slice(&contents) {
            Ok(s) => s,
            Err(e) => { log.err(format_args!("while parsing config file: {}", e)); return None }
        };
        self_.security.login_password = get_admin_password(log);
        // Change directory to the location of the config file so that `Paths` is relative to it
        if let Some(containing_dir) = path.parent() {
            if containing_dir != Path::new("") {
                if let Err(e) = env::set_current_dir(containing_dir) {
                    log.err(format_args!("could not cd to '{}': {}", containing_dir.display(), e));
                }
            }
        } else {
            log.err("impossible - config file has no parent dir");
        }
        Some(self_)
    }
}
