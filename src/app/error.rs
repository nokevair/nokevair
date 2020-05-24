//! Utilities for error handling.

use hyper::{Response, Body};
use tera::Context;

use std::fmt::Display;

use super::lua::Version;

/// A custom `Result` type representing either a successful result
/// or an HTTP error response.
pub type Result<T> = std::result::Result<T, Response<Body>>;

impl super::AppState {
    /// Return an error with status code 400.
    pub(super) fn error_400<T>(&self) -> Result<T> {
        let mut response = self.render("400.html", &Context::new())?;
        *response.status_mut() = hyper::StatusCode::from_u16(400).unwrap();
        Err(response)
    }

    /// Return an error with status code 401.
    pub(super) fn error_401<T>(&self) -> Result<T> {
        let mut response = self.render("401.html", &Context::new())?;
        *response.status_mut() = hyper::StatusCode::from_u16(401).unwrap();
        Err(response)
    }

    /// Return an error with status code 404.
    pub(super) fn error_404<T>(&self) -> Result<T> {
        let mut response = self.render("404.html", &Context::new())?;
        *response.status_mut() = hyper::StatusCode::from_u16(404).unwrap();
        Err(response)
    }

    /// Return a 404 error caused by a particular version of the state
    /// not being loadable.
    pub(super) fn error_404_no_state<T>(&self, ver: Version) -> Result<T> {
        let mut ctx = Context::new();
        ctx.insert("ver", &ver.as_usize());
        let mut response = self.render("404_no_state.html", &ctx)?;
        *response.status_mut() = hyper::StatusCode::from_u16(404).unwrap();
        Err(response)
    }

    /// Log a message and return an error with status code 500.
    pub(super) fn error_500<T, M: Display>(&self, msg: M) -> Result<T> {
        self.ctx.log.err(msg);
        let mut response = self.render("500.html", &Context::new())?;
        *response.status_mut() = hyper::StatusCode::from_u16(500).unwrap();
        Err(response)
    }

    /// Return a textual error with a custom status code.
    pub(super) fn text_error<T>(status: u16, msg: &'static str) -> Result<T> {
        Err(Response::builder()
            .status(status)
            .header("Content-Type", "text/plain")
            .body(Body::from(msg))
            .unwrap())
    }
}
