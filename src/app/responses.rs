//! Utilites for generating various kinds of responses.

use hyper::{Response, Body};

use super::Result;

impl super::AppState {
    /// Generate a response that redirects to a given URL.
    pub(super) fn redirect(uri: &str) -> Response<Body> {
        Response::builder()
            .status(303)
            .header("Location", uri)
            .body(Body::empty())
            .unwrap()
    }

    /// Generate an empty response with status code 200.
    pub(super) fn empty_200() -> Response<Body> {
        Response::builder()
            .status(200)
            .body(Body::empty())
            .unwrap()
    }
    
    /// Generate a response with the content of the file at the given path.
    /// If the file is not found, return a 404.
    pub(super) async fn serve_file(&self, path: &str) -> Result<Response<Body>> {
        use tokio::fs::File;
        use hyper_staticfile::FileBytesStream;
        if let Ok(file) = File::open(path).await {
            let body = FileBytesStream::new(file).into_body();
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Ok(Response::builder()
                .status(200)
                .header("Content-Type", &format!("{}", mime))
                .body(body)
                .unwrap())
        } else {
            self.error_404()
        }
    }    
}
