use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Request, Response, Body};

use std::net::SocketAddr;

use std::sync::Arc;

pub trait Respond: Send + Sync + 'static {
    fn respond(&self, addr: SocketAddr, req: Request<Body>) -> Response<Body>;
}

pub async fn run_server<R: Respond>(responder: &Arc<R>, addr: SocketAddr) -> hyper::Result<()> {
    let service = make_service_fn(move |addr_stream: &AddrStream| {
        let remote_addr = addr_stream.remote_addr();
        let responder = Arc::clone(responder);
        async move {
            hyper::Result::Ok(service_fn(move |req| {
                let response = responder.respond(remote_addr, req);
                async move { hyper::Result::Ok(response) }
            }))
        }
    });
    hyper::Server::bind(&addr)
        .serve(service)
        .await
}