use api_types::{ApiResponse, ExampleApiType};
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use colored::Colorize;
use dotenv::dotenv;
use log::{error, info};
use reqwest::Client;
use tokio::signal;
use tower_http::cors::CorsLayer;
// Test
pub struct TestState {
    pub counter: i32,
}

async fn healthcheck() -> Html<&'static str> {
    Html("Service is up")
}

async fn hello_world() -> Result<Response, StatusCode> {
    let api_response = ApiResponse {
        status: "success".to_string(),
        data: Some(ExampleApiType::new("Hello from the backend")),
    };
    Ok(Json(api_response).into_response())
}

#[tokio::main]
async fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }
    dotenv().ok();

    // Initialize the logger
    env_logger::init();

    println!(
        "{}",
        "🚀 Server is successfully on FIRE!".bright_green().bold()
    );

    let client = Client::new();
    let app = Router::new()
        .route("/api/healthchecker", get(healthcheck))
        .route("/api/hello-world", get(hello_world))
        .layer(CorsLayer::very_permissive())
        .with_state(client);

    let addr = "0.0.0.0:8000";

    info!("{}", format!("Starting server at {}", addr).blue());

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    let server = axum::serve(listener, app);

    // Set up graceful shutdown
    let graceful = server.with_graceful_shutdown(shutdown_signal());

    if let Err(e) = graceful.await {
        error!("{}", format!("Server error: {}", e).red().bold());
    }

    println!(
        "{} [ {} ]",
        "Server was terminated!".bold().red(),
        "DONE".bold().green()
    );
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

    info!(
        "{}",
        "Shutdown signal received, starting graceful shutdown".yellow()
    );
}
