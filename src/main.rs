use std::net::SocketAddr;
use std::sync::Arc;

mod hyper_boilerplate;

mod app;
use app::AppState;

#[tokio::main]
pub async fn main() {
    let app_state = Arc::new(AppState::new());
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    tokio::join!(
        app_state.increment_count(),
        async {
            if let Err(e) = hyper_boilerplate::run_server(&app_state, addr).await {
                // TODO: change this to some sort of logging statement
                eprintln!("Error: {}", e);
            }
        }
    );
}