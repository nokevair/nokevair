//! Expose the `Respond` trait and provide an abstraction over the details necessary
//! to initialize and run a server.

use async_trait::async_trait;

use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Request, Response, Body};

use std::net::SocketAddr;

use std::sync::Arc;

/// Represents a type capable of being used to generate responses to a request
/// (i.e. server state).
#[async_trait]
pub trait Respond: Send + Sync + 'static {
    /// Generate a response to the request.
    async fn respond(&self, addr: SocketAddr, req: Request<Body>) -> Response<Body>;
    /// Respond to the server shutting down due to a Hyper error.
    fn shutdown_on_err(&self, err: hyper::Error);
}

/// Run a server, using `responder` to generate responses to requests. Keep running
/// until Hyper experiences an error.
pub async fn run_server<R: Respond>(responder: &Arc<R>, addr: SocketAddr) {
    let service = make_service_fn(move |addr_stream: &AddrStream| {
        let remote_addr = addr_stream.remote_addr();
        let responder = Arc::clone(responder);
        async move {
            hyper::Result::Ok(service_fn(move |req| {
                let responder = Arc::clone(&responder);
                async move {
                    let response = responder.respond(remote_addr, req);
                    hyper::Result::Ok(response.await)
                }
            }))
        }
    });
    if let Err(e) = hyper::Server::bind(&addr).serve(service).await {
        responder.shutdown_on_err(e);
    }
}