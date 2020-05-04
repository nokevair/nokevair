//! The server (and driver program) for Nokevair.

use std::net::SocketAddr;
use std::sync::Arc;

mod app;
use app::AppState;

mod hyper_boilerplate;

mod utils;

#[tokio::main]
async fn main() {
    let app_state = Arc::new(AppState::new());
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    
    tokio::join!(
        app_state.do_scheduled(),
        hyper_boilerplate::run_server(&app_state, addr),
    );
}