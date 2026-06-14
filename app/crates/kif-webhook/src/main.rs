mod admission;
mod config;
mod http;
mod k8s;
mod merge;
mod mutate;
mod queue;
mod reconcile;

use std::net::SocketAddr;
use std::sync::{Arc, atomic::AtomicBool};

use anyhow::Result;
use axum_server::tls_rustls::RustlsConfig;
use kube::Client;
use rustls::crypto::ring;
use tokio::net::TcpListener;
use tracing::{info, warn};

use config::WebhookConfig;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if ring::default_provider().install_default().is_err() {
        warn!("rustls crypto provider was already installed, skipping");
    }

    let cfg = WebhookConfig::from_env()?;
    let client = Client::try_default().await?;

    // Work queue + controller
    let ready = Arc::new(AtomicBool::new(false));
    let (q, rx) = queue::Queue::new(2048);
    let watcher_handle = tokio::spawn(reconcile::watch_cloud_role_bindings(
        client.clone(),
        q.clone(),
        Arc::clone(&ready),
    ));
    let workers_handle = tokio::spawn(reconcile::run_workers(
        client.clone(),
        cfg.reconcile_workers,
        rx,
        q.clone(),
    ));

    // A simple server for the probes
    let health_bind_addr = format!("0.0.0.0:{}", cfg.health_port);
    let health_listener = TcpListener::bind(&health_bind_addr).await?;
    info!(bind_addr = %health_bind_addr, "health server up");
    let health_handle = tokio::spawn(async move {
        let app = http::health_router(ready);
        axum::serve(health_listener, app).await
    });

    // Admission webhook server
    let bind_addr = format!("0.0.0.0:{}", cfg.https_port);
    let tls = RustlsConfig::from_pem_file(&cfg.tls_cert_path, &cfg.tls_key_path).await?;
    let app = http::admission_router(admission::AppState { cfg, client });
    info!(%bind_addr, "admission server up");

    tokio::select! {
        result = axum_server::bind_rustls(bind_addr.parse::<SocketAddr>()?, tls)
            .serve(app.into_make_service()) => {
            result?;
        }
        result = watcher_handle => {
            result??;
        }
        result = workers_handle => {
            result??;
        }
        result = health_handle => {
            result??;
        }
    }

    Ok(())
}
