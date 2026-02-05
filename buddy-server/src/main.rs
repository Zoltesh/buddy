use axum::Router;
use tower_http::services::ServeDir;

mod types;

#[tokio::main]
async fn main() {
    let app = Router::new().fallback_service(ServeDir::new("frontend/dist"));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("failed to bind to 127.0.0.1:3000");

    println!("Serving on http://127.0.0.1:3000");

    axum::serve(listener, app).await.expect("server error");
}
