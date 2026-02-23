mod config;
mod http;
mod jwt;
mod k8s;

use anyhow::Result;
use axum::Router;
use tokio::net::TcpListener;
use tracing::info;

use config::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let state = AppState::from_env().await?;
    let port = state.port;
    let bind = format!("0.0.0.0:{port}");
    info!(%bind, "federation listening");
    let listener = TcpListener::bind(bind).await?;
    let app: Router = http::router(state);
    axum::serve(listener, app).await?;
    Ok(())
}
