mod config;
mod http;
mod jwks;
mod kube_watch;

use anyhow::{Result, bail};
use tokio::net::TcpListener;
use tokio::time::{Duration, Instant};
use tracing::info;

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

    let state = AppState {
        cfg: cfg.clone(),
        jwks: store.clone(),
    };

    let jwks_handle = tokio::spawn(load_jwks_and_watch(
        store,
        cfg.jwks_file_path,
        cfg.jwks_secret_name,
        cfg.running_in_cluster,
    ));

    info!(%bind_addr, "issuer server up");

    // Serve in its own task so that a *successful* completion of the JWKS
    // loader (when not running in-cluster there is nothing left to watch after
    // the initial load) does not cancel the HTTP server. A failure in either
    // task still brings the process down.
    let mut server = tokio::spawn(async move { axum::serve(listener, router(state)).await });

    tokio::select! {
        result = &mut server => {
            result??;
        }
        result = jwks_handle => {
            result??;
            server.await??;
        }
    }

    Ok(())
}

async fn load_jwks_and_watch(
    store: JwksStore,
    jwks_file_path: String,
    jwks_secret_name: String,
    running_in_cluster: bool,
) -> Result<()> {
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
                    bail!("failed to parse JWKS from {}: {}", jwks_file_path, e);
                }
            },
            Err(e) => {
                if Instant::now() < deadline {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                } else {
                    bail!("failed to read JWKS_FILE_PATH={}: {}", jwks_file_path, e);
                }
            }
        }
    }

    if running_in_cluster {
        let ns = shared::pod_namespace()?;
        info!("watching for JWKS secret changes");
        kube_watch::run_secret_watcher(ns, jwks_secret_name, store).await?;
    }

    Ok(())
}
