//! Various utility functions.

/// If `s` starts with `prefix`, return the rest of `s`.
pub fn strip_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.starts_with(prefix) {
        Some(&s[prefix.as_bytes().len()..])
    } else {
        None
    }
}

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
    // TODO: return an Err instead of panicking
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