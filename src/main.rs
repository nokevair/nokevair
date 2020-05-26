//! The server (and driver program) for Nokevair.

use std::sync::Arc;

#[macro_use]
mod utils;

mod app;
use app::AppState;
use app::Ctx;

mod conv;
mod hyper_boilerplate;

#[tokio::main]
async fn main() {
    let ctx = match Ctx::load() {
        Some(c) => c,
        None => return,
    };
    let addr = ctx.cfg.addr;
    let (mut lua_backend, app_state) = AppState::new(ctx);
    let app_state = Arc::new(app_state);
    
    tokio::join!(
        app_state.do_scheduled(),
        lua_backend.run(&app_state),
        hyper_boilerplate::run_server(&app_state, addr),
    );
}
