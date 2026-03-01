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
use tokio::net::TcpListener;
use tracing::info;

use config::WebhookConfig;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cfg = WebhookConfig::from_env()?;
    let client = Client::try_default().await?;

    // Work queue + controller
    let ready = Arc::new(AtomicBool::new(false));
    let (q, rx) = queue::Queue::new(2048);
    tokio::spawn(reconcile::watch_cloud_role_bindings(
        client.clone(),
        q.clone(),
        Arc::clone(&ready),
    ));
    tokio::spawn(reconcile::run_workers(
        client.clone(),
        cfg.clone(),
        rx,
        q.clone(),
    ));

    // A simple server for the probes
    {
        let bind_addr = format!("0.0.0.0:{}", cfg.health_port);
        let listener = TcpListener::bind(&bind_addr).await?;
        info!(%bind_addr, "health server up");

        tokio::spawn(async move {
            let app = http::health_router(ready);
            let _ = axum::serve(listener, app).await;
        });
    }

    // Admission webhook server
    let bind_addr = format!("0.0.0.0:{}", cfg.https_port);
    info!(%bind_addr, "admission server up");

    let tls = RustlsConfig::from_pem_file(&cfg.tls_cert_path, &cfg.tls_key_path).await?;
    let app = http::admission_router(admission::AppState {
        cfg: cfg.clone(),
        client,
    });

    axum_server::bind_rustls(bind_addr.parse::<SocketAddr>()?, tls)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
