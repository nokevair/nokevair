use hyper::{Response, Body};

use std::fmt::Display;

// TODO: When we start having a standardized page design, these pages
// need to use it. Their content can be moved to a file. We can also
// introduce a standardized template for this design.

// TODO: replace calls to this with some sort of logging statement
pub fn impossible<T: Display>(t: T) -> Response<Body> {
    Response::builder()
        .status(500)
        .body(Body::from(format!("impossible happened:\n{}", t)))
        .unwrap()
}

pub fn not_found() -> Response<Body> {
    Response::builder()
        .status(404)
        .body(Body::from("404 not found"))
        .unwrap()
}

pub fn unauthorized() -> Response<Body> {
    Response::builder()
        .status(401)
        .body(Body::from("401 unauthorized"))
        .unwrap()
}

pub fn bad_request() -> Response<Body> {
    Response::builder()
        .status(400)
        .body(Body::from("400 bad request"))
        .unwrap()
}

pub fn redirect(uri: &str) -> Response<Body> {
    Response::builder()
        .status(303)
        .header("Location", uri)
        .body(Body::empty())
        .unwrap()
}

pub async fn file(path: &str) -> Response<Body> {
    use tokio::fs::File;
    use hyper_staticfile::FileBytesStream;
    if let Ok(file) = File::open(path).await {
        let body = FileBytesStream::new(file).into_body();
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        Response::builder()
            .status(200)
            .header("Content-Type", &format!("{}", mime))
            .body(body)
            .unwrap()
    } else {
        not_found()
    }
}
