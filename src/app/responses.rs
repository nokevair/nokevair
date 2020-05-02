use hyper::{Response, Body};

use std::fmt::Display;

// TODO: When we start having a standardized page design, these pages
// need to use it. Their content can be moved to a file. We can also
// introduce a standardized template for this design.

// TODO: include some sort of logging statement
pub fn impossible<T: Display>(t: T) -> Response<Body> {
    Response::builder()
        .status(500)
        .body(Body::from(format!("{}", t)))
        .unwrap()
}

pub fn not_found() -> Response<Body> {
    Response::builder()
        .status(404)
        .body(Body::from("404 not found"))
        .unwrap()
}
