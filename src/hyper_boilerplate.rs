use async_trait::async_trait;

use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Request, Response, Body};

use std::net::SocketAddr;

use std::sync::Arc;

#[async_trait]
pub trait Respond: Send + Sync + 'static {
    async fn respond(&self, addr: SocketAddr, req: Request<Body>) -> Response<Body>;
}

pub async fn run_server<R: Respond>(responder: &Arc<R>, addr: SocketAddr) -> hyper::Result<()> {
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
    hyper::Server::bind(&addr)
        .serve(service)
        .await
}