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

    let running_in_cluster = cfg.running_in_cluster;
    let jwks_file_path = cfg.jwks_file_path.clone();
    let jwks_secret_name = cfg.jwks_secret_name.clone();

    let state = AppState {
        cfg,
        jwks: store.clone(),
    };

    tokio::spawn(async move {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            match tokio::fs::read(&jwks_file_path).await {
                Ok(bytes) => match parse_jwks(bytes) {
                    Ok(jwks) => {
                        store.set(jwks).await;
                        info!(jwks_path = %jwks_file_path, "loaded JWKS from file");
                        break;
                    }
                    Err(e) => {
                        error!("failed to parse JWKS from {}: {}", jwks_file_path, e);
                        exit(1);
                    }
                },
                Err(e) => {
                    if Instant::now() < deadline {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    } else {
                        error!("failed to read JWKS_FILE_PATH={}: {}", jwks_file_path, e);
                        exit(1);
                    }
                }
            }
        }

        if running_in_cluster {
            let ns = match shared::pod_namespace() {
                Ok(ns) => ns,
                Err(e) => {
                    error!("failed to determine pod namespace: {}", e);
                    exit(1);
                }
            };

            info!("watching for JWKS secret changes");
            if let Err(e) =
                kube_watch::run_secret_watcher(ns, jwks_secret_name, store.clone()).await
            {
                error!("JWKS secret watcher failed: {}", e);
                exit(1);
            }
        }
    });

    info!(%bind_addr, "issuer server up");
    axum::serve(listener, router(state)).await?;
    Ok(())
}
