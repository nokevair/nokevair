//! Utilities related to generating and maintaining world state.

use std::collections::BTreeMap;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::{Arc, RwLock, PoisonError};

use super::Log;

/// Represents the ID of a particular version of the world state.
pub type Version = u32;

/// Represents a particular version of the world state.
struct WorldState {
    /// The version number.
    ver: Version,
    /// The data for this version.
    data: Arc<rmpv::Value>, // TODO: use lua value instead of RMP value
}

impl WorldState {
    /// Create a dummy state for testing purposes:
    /// `{ people: [[0, 0], [10, 10]] }`
    fn dummy(ver: Version) -> Self {
        use rmpv::Value::Array;
        Self {
            ver,
            data: Arc::new(
                vec![("people".into(), Array(vec![
                    Array(vec![0.into(), 0.into()]),
                    Array(vec![10.into(), 10.into()]),
                ]))].into())
        }
    }

    /// Attempt to read a MessagePack value from the file corresponding to the
    /// provided ID. Return `None` if the file does not exist and `Err(_)` if
    /// the file cannot be read.
    fn load_from_file(ver: Version) -> io::Result<Option<Self>> {
        let path = format!("state/{}.msgpack", ver);
        if !Path::new(&path).exists() { return Ok(None) }
        let mut file = File::open(path)?;
        let data = rmpv::decode::value::read_value(&mut file)
            .map(Arc::new)
            .map_err::<io::Error, _>(Into::into)?;
        Ok(Some(Self { ver, data }))
    }
}

// TODO: it's not a good idea to store all the world states,
// since they can potentially grow very large. Instead,
// we should switch to some sort of LRU cache with ~20 lines
// once there are more states than this.

/// An in-memory cache of certain versions of the world state, providing
/// various ways to manipulate them.
pub struct WorldStates {
    /// All currently loaded versions.
    versions: RwLock<BTreeMap<Version, WorldState>>,
}

impl WorldStates {
    /// Attempt to load all versions of the world state from the directory `state`.
    pub fn load(log: &Log) -> Self {
        let mut id = 0;
        let mut versions = BTreeMap::new();

        loop {
            match WorldState::load_from_file(id) {
                Err(e) => log.err(format_args!("cannot load world state {}: {:?}", id, e)),
                Ok(None) => break,
                Ok(Some(state)) => { versions.insert(id, state); }
            }
            id += 1;
        }

        log.info(format_args!(
            "loaded {} world state{}",
            versions.len(),
            if versions.len() == 1 { "" } else { "s" }
        ));

        if id == 0 {
            versions.insert(id, WorldState::dummy(id));
            log.info("created dummy world state");
        }

        Self {
            versions: RwLock::new(versions),
        }
    }

    /// Get the latest version of the world state.
    pub fn latest(&self) -> (Version, Arc<rmpv::Value>) {
        let versions = self.versions.read()
            .unwrap_or_else(PoisonError::into_inner);
        let (&ver, state) = versions.iter().next_back().unwrap();
        (ver, Arc::clone(&state.data))
    }

    /// Write the current version of the world state as a new file.
    pub fn save_new_version(&self, log: &Log) {
        let (ver, state) = self.latest();

        // the potential path of the new state file
        let path = format!("state/{}.msgpack", ver + 1);
        log.info(format_args!("attempting to write file '{}'", path));

        // don't create the file if it already exists
        if Path::new(&path).exists() {
            log.err("file already exists");
            return
        }

        let mut file = match File::create(path) {
            Ok(file) => file,
            Err(e) => {
                log.err(format_args!("failed to create file: {:?}", e));
                return
            }
        };

        if let Err(e) = rmpv::encode::write_value(&mut file, &state) {
            log.err(format_args!("failed to write file: {:?}", e));
        }
    }
}