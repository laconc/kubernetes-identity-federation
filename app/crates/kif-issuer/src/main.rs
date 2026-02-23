mod config;
mod http;
mod jwks;
mod kube_watch;

use std::process::exit;

use anyhow::{Context, Result, anyhow};
use tokio::net::TcpListener;
use tracing::{error, info};

use config::{AppState, IssuerConfig};
use http::router;
use jwks::{JwksStore, parse_jwks};
use kif_api::shared;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cfg = IssuerConfig::from_env()?;
    let store = JwksStore::new();

    let path = cfg.jwks_file_path.clone();
    let bytes = tokio::fs::read(&path)
        .await
        .context(anyhow!("failed to read JWKS_FILE_PATH={}", path))?;
    let jwks = parse_jwks(bytes)?;
    store.set(jwks).await;

    info!(%path, "loaded JWKS from file");

    if cfg.running_in_cluster {
        let ns = shared::pod_namespace()?;
        let secret_name = cfg.jwks_secret_name.clone();
        let store_clone = store.clone();

        tokio::spawn(async move {
            if let Err(e) = kube_watch::run_secret_watcher(ns, secret_name, store_clone).await {
                error!("JWKS secret watcher failed: {}", e);
                exit(1);
            }
        });

        info!("watching for JWKS secret changes");
    }

    let bind_addr = format!("0.0.0.0:{}", cfg.port.clone());
    let listener = TcpListener::bind(&bind_addr).await?;
    info!(%bind_addr, "issuer listening");

    let state = AppState { cfg, jwks: store };
    axum::serve(listener, router(state)).await?;
    Ok(())
}
