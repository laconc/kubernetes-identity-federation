mod config;
mod http;
mod jwks;
mod kube_watch;

use std::process::exit;

use anyhow::Result;
use tokio::net::TcpListener;
use tokio::time::{Duration, Instant};
use tracing::{error, info};

use config::{AppState, IssuerConfig};
use http::router;
use jwks::{JwksStore, parse_jwks};
use kif_api::shared;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cfg = IssuerConfig::from_env()?;
    let store = JwksStore::new();

    let bind_addr = format!("0.0.0.0:{}", cfg.port);
    let listener = TcpListener::bind(&bind_addr).await?;
    info!(%bind_addr, "issuer server up");

    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        match tokio::fs::read(&cfg.jwks_file_path).await {
            Ok(bytes) => match parse_jwks(bytes) {
                Ok(jwks) => {
                    store.set(jwks).await;
                    info!(jwks_path = %cfg.jwks_file_path, "loaded JWKS from file");
                    break;
                }
                Err(e) => {
                    error!("failed to parse JWKS from {}: {}", cfg.jwks_file_path, e);
                    exit(1);
                }
            },
            Err(e) => {
                if Instant::now() < deadline {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                } else {
                    error!(
                        "failed to read JWKS_FILE_PATH={}: {}",
                        cfg.jwks_file_path, e
                    );
                    exit(1);
                }
            }
        }
    }

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

    let state = AppState { cfg, jwks: store };
    axum::serve(listener, router(state)).await?;
    Ok(())
}
