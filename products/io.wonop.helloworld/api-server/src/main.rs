//! API Server - A Rust/Axum HTTP server built with Bazel for Rush demo.

use axum::{
    extract::Path,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

/// Info response with build details
#[derive(Serialize)]
struct InfoResponse {
    message: String,
    build: &'static str,
    version: &'static str,
    path: String,
}

/// Generic API response
#[derive(Serialize)]
struct ApiResponse {
    message: String,
    path: String,
}

/// Root handler - returns hello message
async fn root() -> Json<InfoResponse> {
    info!("Handling root request");
    Json(InfoResponse {
        message: "Hello from Bazel Rust API!".to_string(),
        build: "Built with Bazel and Rush",
        version: env!("CARGO_PKG_VERSION"),
        path: "/".to_string(),
    })
}

/// Health check endpoint
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy",
        service: "api-server",
    })
}

/// Info endpoint with detailed service information
async fn info() -> Json<InfoResponse> {
    info!("Handling info request");
    Json(InfoResponse {
        message: "Rust API Server".to_string(),
        build: "Built with Bazel and Rush",
        version: env!("CARGO_PKG_VERSION"),
        path: "/info".to_string(),
    })
}

/// Echo endpoint - echoes back the path parameter
async fn echo(Path(message): Path<String>) -> Json<ApiResponse> {
    info!(message = %message, "Handling echo request");
    Json(ApiResponse {
        message: format!("Echo: {}", message),
        path: format!("/echo/{}", message),
    })
}

/// Catch-all handler for any other paths
async fn catch_all(uri: axum::http::Uri) -> Json<ApiResponse> {
    info!(path = %uri.path(), "Handling catch-all request");
    Json(ApiResponse {
        message: "Hello from Bazel Rust API!".to_string(),
        path: uri.path().to_string(),
    })
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");

    // Build our application with routes
    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/info", get(info))
        .route("/echo/{message}", get(echo))
        .fallback(get(catch_all));

    // Run on port 8080
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    
    println!("==================================================");
    println!("Bazel Rust API Server is UP and RUNNING!");
    println!("Listening on: http://{}", addr);
    println!("Built with Bazel and deployed via Rush");
    println!("==================================================");
    
    info!(address = %addr, "Starting API server");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
