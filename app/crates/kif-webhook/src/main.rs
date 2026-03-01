mod admission;
mod config;
mod http;
mod k8s;
mod merge;
mod mutate;
mod queue;
mod reconcile;

use std::net::SocketAddr;

use anyhow::Result;
use axum_server::tls_rustls::RustlsConfig;
use kube::Client;
use tokio::net::TcpListener;
use tracing::{info, warn};

use config::WebhookConfig;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cfg = WebhookConfig::from_env()?;
    let client = Client::try_default().await?;

    // Work queue + controller
    let (q, _rx) = queue::Queue::new(2048);
    {
        let client = client.clone();
        let cfg = cfg.clone();
        let q = q.clone();
        tokio::spawn(async move {
            if let Err(e) = reconcile::watch_cloud_role_bindings(client, q, cfg).await {
                warn!(error=?e, "watcher exited");
            }
        });
    }
    {
        let client = client.clone();
        let cfg = cfg.clone();
        let (q, rx) = queue::Queue::new(2048);
        tokio::spawn(reconcile::watch_cloud_role_bindings(
            client.clone(),
            q.clone(),
            cfg.clone(),
        ));
        tokio::spawn(reconcile::run_workers(
            client.clone(),
            cfg.clone(),
            rx,
            q.clone(),
        ));
    }

    // A simple server for the probes
    {
        let bind_addr = format!("0.0.0.0:{}", cfg.health_port);
        let listener = TcpListener::bind(&bind_addr).await?;
        info!(%bind_addr, "health server up");

        tokio::spawn(async move {
            let app = http::health_router();
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
