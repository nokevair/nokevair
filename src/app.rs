use hyper::{Request, Response, Body};

use serde::Deserialize;

use tokio::time::{Duration, interval};

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::hyper_boilerplate::Respond;

mod responses;

pub struct AppState {
    count: AtomicU64,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
        }
    }
    pub async fn increment_count(&self) {
        let mut interval = interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            self.count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Respond for AppState {
    fn respond(&self, _: SocketAddr, req: Request<Body>) -> Response<Body> {
        // Return an error if we somehow get a URI that doesn't have a path.
        let uri = req.uri().clone().into_parts();
        let path_and_query = match uri.path_and_query {
            None => return responses::impossible("no path"),
            Some(pnq) => pnq,
        };

        // Parse a query string of the form `?i=...`
        #[derive(Deserialize)]
        struct QueryDecode { i: String }
        let path = path_and_query.path().trim_matches('/').to_owned();
        let query_param = path_and_query.query().and_then(|query|
            serde_urlencoded::from_str::<QueryDecode>(query)
                .ok()
                .map(|q| q.i));

        let count = self.count.load(Ordering::Relaxed);
        let content = format!(
            "Path:  {}\n\
             Param: {}\n\
             Count: {}",
             path,
             query_param.unwrap_or_else(|| String::from("none")),
             count,
        );
        Response::new(Body::from(content))
    }
}