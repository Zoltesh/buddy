use axum::Router;
use tower_http::services::ServeDir;

mod config;
mod provider;
mod types;

#[tokio::main]
async fn main() {
    let config = config::Config::load().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let addr = config.bind_address();
    let app = Router::new().fallback_service(ServeDir::new("frontend/dist"));

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to bind to {addr}: {e}");
            std::process::exit(1);
        });

    println!("Serving on http://{addr}");

    axum::serve(listener, app).await.expect("server error");
}
