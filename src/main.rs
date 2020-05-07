//! The server (and driver program) for Nokevair.

use std::net::SocketAddr;
use std::sync::Arc;

mod app;
use app::AppState;

mod conv;
mod hyper_boilerplate;
mod utils;

#[tokio::main]
async fn main() {
    let (mut lua_backend, app_state) = AppState::new();
    let app_state = Arc::new(app_state);
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    
    tokio::join!(
        app_state.do_scheduled(),
        lua_backend.run(&app_state),
        hyper_boilerplate::run_server(&app_state, addr),
    );
}
