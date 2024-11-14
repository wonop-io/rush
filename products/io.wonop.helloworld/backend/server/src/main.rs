use api_types::{
    ExampleApiType, ApiResponse
};
use axum::extract::Request;
use axum::extract::State;
use axum::middleware::from_fn;
use axum::middleware::Next;

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use dotenv::dotenv;
use log::{info, error};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tokio::signal;

pub struct TestState {
    pub counter: i32,
}

async fn healthcheck() -> Html<&'static str> {
    Html("Service is up")
}

async fn hello_world() -> Result<Response, StatusCode> {
    let api_response = ApiResponse {
        status: "success".to_string(),
        data: Some(ExampleApiType::new("Hello from the backend"))
    };
    Ok(Json(api_response).into_response())
}

#[tokio::main]
async fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }

    dotenv().ok();
    println!("🚀 Server is successfully on FIRE!");

    let client = Client::new();
    let app = Router::new()
        .route("/api/healthchecker", get(healthcheck))
        .route("/api/hello-world", get(hello_world))
        .layer(CorsLayer::very_permissive())
        .with_state(client);

    let addr = "0.0.0.0:8000";

    info!("Starting server at {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    let server = axum::serve(listener, app);

    // Set up graceful shutdown
    let graceful = server.with_graceful_shutdown(shutdown_signal());

    if let Err(e) = graceful.await {
        error!("Server error: {}", e);
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, starting graceful shutdown");
}
