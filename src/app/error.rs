//! Utilities for error handling.

use hyper::{Response, Body};

use std::borrow::Cow;
use std::fmt::Display;

/// A custom `Result` type representing either a successful result
/// or an HTTP error response.
pub type Result<T> = std::result::Result<T, Response<Body>>;

/// Represents an application error.
pub struct Error {
    /// The associated HTTP status code (e.g. 404).
    status: u16,
    /// An associated message, if present.
    msg: Cow<'static, str>,
}

impl Error {
    /// Creates an error with the provided status code and message.
    fn new(status: u16, msg: impl Into<Cow<'static, str>>) -> Self {
        Self { status, msg: msg.into() }
    }
}

impl Into<Response<Body>> for Error {
    fn into(self) -> Response<Body> {
        Response::builder()
            .status(self.status)
            .header("Content-Type", "text/plain")
            .body(Body::from(self.msg))
            .unwrap()
    }
}

impl super::AppState {
    /// Return an error with status code 400.
    pub(super) fn error_400<T>(&self) -> Result<T> {
        Err(Error::new(400, "400 - The request was invalid.").into())
    }

    /// Return an error with status code 401.
    pub(super) fn error_401<T>(&self) -> Result<T> {
        Err(Error::new(401, "401 - Invalid authorization.").into())
    }

    /// Return an error with status code 404.
    pub(super) fn error_404<T>(&self) -> Result<T> {
        Err(Error::new(404, "400 - The requested page was not found.").into())
    }

    /// Log a message and return an error with status code 500.
    pub(super) fn error_500<T, M: Display>(&self, msg: M) -> Result<T> {
        self.log.err(msg);
        // TODO: make this use a template
        Err(Error::new(500, "500 - An internal server error occured.").into())
    }
}