use axum::{routing::get, Router};
use std::env;

#[tokio::main]
async fn main() {
    // Print connection strings from environment
    println!("Environment variables:");
    for (key, value) in env::vars() {
        if key.contains("DATABASE_URL") || key.contains("REDIS_URL") || key.contains("S3_") {
            println!("  {} = {}", key, value);
        }
    }

    // Simple HTTP server
    let app = Router::new()
        .route("/", get(|| async { "Hello from app with local services!" }))
        .route("/health", get(|| async { "OK" }));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .unwrap();
    
    println!("Server running on http://0.0.0.0:8080");
    axum::serve(listener, app).await.unwrap();
}