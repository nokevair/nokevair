//! Exports the `Version` type, used to manage different versions
//! of the world state.

use std::path::Path;
use std::str::FromStr;

/// Represents a particular version of the world state.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Version(u32);

impl Version {
    /// Return the next version.
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// Return the previous version, or None if this is the first version.
    pub fn previous(self) -> Option<Self> {
        self.0.checked_sub(1).map(Self)
    }

    /// Return the path associated with this version of the state.
    pub fn path(self) -> String {
        // TODO: have the directory be configurable
        format!("state/{}.msgpack", self.0)
    }

    /// Return the first version with no associated state file.
    pub fn next_available() -> Self {
        let mut ver = Self(0);
        while Path::new(&ver.path()).exists() {
            ver = ver.next();
        }
        ver
    }

    /// Convert this version to a `usize`.
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl FromStr for Version {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}
