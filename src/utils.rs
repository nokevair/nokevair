//! Various utility functions.

use std::error::Error;
use std::fmt;

/// Hash the input string with SHA256.
pub fn sha256(s: &str) -> String {
    use sha2::{Sha256, digest::Digest};
    let mut hasher = Sha256::default();
    hasher.input(s);
    let result: &[u8] = &hasher.result();
    hex::encode(&result)
}

/// Convert the body of a request into a byte vector.
pub async fn read_body(body: hyper::Body) -> Result<Vec<u8>, hyper::Error> {
    use tokio::stream::StreamExt as _;
    body.fold(Ok(Vec::new()), |acc, chunk| {
        match (acc, chunk) {
            (Err(e), _) => Err(e),
            (_, Err(e)) => Err(e),
            (Ok(mut bytes), Ok(chunk)) => {
                bytes.extend_from_slice(&chunk);
                Ok(bytes)
            }
        }
    }).await
}

/// Attempt to interpret a byte vector as a UTF-8 encoded `u32`.
pub fn parse_u32(bytes: Vec<u8>) -> Option<u32> {
    let s = String::from_utf8(bytes).ok()?;
    s.parse().ok()
}

/// Join a sequence of paths together into a `PathBuf`.
#[macro_export]
macro_rules! path {
    ($fst:expr $(, $parts:expr)*) => {{
        let mut path = std::path::PathBuf::from($fst);
        $(path.push($parts);)*
        path
    }}
}

/// Wraps a type implementing `Error` and implements `Display` by repeatedly
/// calling `source()` and putting each message on a separate line.
pub struct SourceChain<E>(pub E);

impl<E: Error> fmt::Display for SourceChain<E> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)?;
        let mut source = self.0.source();
        while let Some(e) = source {
            write!(f, "\n{}", e)?;
            source = e.source();
        }
        Ok(())
    }
}
