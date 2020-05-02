use hyper::{Response, Body};

use std::fmt::Display;

// TODO: include some sort of logging statement
pub fn impossible<T: Display>(t: T) -> Response<Body> {
    Response::builder()
        .status(500)
        .body(Body::from(format!("{}", t)))
        .unwrap()
}