mod config;
mod federation;
mod http;
mod writer;

use std::process::exit;
use std::time::Duration;

use anyhow::{Context, Result};
use config::AgentConfig;
use federation::MintRequest;
use tokio::{net::TcpListener, sync::watch, time::sleep};
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cfg = AgentConfig::from_env()?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("build reqwest client")?;

    // Ready channel to signal when the first successful token mint has occurred, for readyz.
    // We don't want to start the main containers until we have our minted tokens.
    let (ready_tx, ready_rx) = watch::channel(false);

    let http_state = http::HttpState { ready_rx };
    let app = http::router(http_state);

    let cfg_clone = cfg.clone();
    let client_clone = client.clone();
    let ready_tx_clone = ready_tx.clone();

    tokio::spawn(async move {
        if let Err(e) = refresh_loop(cfg_clone, client_clone, ready_tx_clone).await {
            error!(error=?e, "refresh loop exited");
            exit(1);
        }
    });

    let bind_addr = format!("0.0.0.0:{}", cfg.port);
    let listener = TcpListener::bind(&bind_addr).await?;
    info!(%bind_addr, "agent listening");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Main loop to refresh tokens. On each iteration, it reads the ServiceAccount token,
/// calls the federation API to mint a new AWS token, writes the AWS token to disk,
/// and sleeps until it's time to refresh again.
async fn refresh_loop(
    cfg: AgentConfig,
    client: reqwest::Client,
    ready_tx: watch::Sender<bool>,
) -> Result<()> {
    let mut first_success = false;

    loop {
        let sa_token = tokio::fs::read_to_string(&cfg.sa_token_path)
            .await
            .context(format!(
                "failed reading ServiceAccount token from {}",
                cfg.sa_token_path.display()
            ))?;
        let sa_token = sa_token.trim();
        if sa_token.is_empty() {
            warn!("ServiceAccount token file was empty; retrying");
            sleep(Duration::from_secs(cfg.min_refresh_seconds)).await;
            continue;
        }

        let req = MintRequest {
            namespace: cfg.namespace.clone(),
            service_account_name: cfg.service_account_name.clone(),
        };

        match federation::mint(&client, &cfg.federation_url, sa_token, req).await {
            Ok(resp) => {
                if let Some(aws) = resp.aws {
                    writer::atomic_write(&cfg.aws_token_path, &aws.token).context(format!(
                        "failed writing AWS token to {}",
                        cfg.aws_token_path.display()
                    ))?;

                    if !first_success {
                        first_success = true;
                        let _ = ready_tx.send(true);
                        info!("first token minted, I'm ready");
                    }

                    let sleep_for = compute_sleep_seconds(
                        aws.expires_in_seconds,
                        cfg.refresh_skew_seconds,
                        cfg.min_refresh_seconds,
                        cfg.max_jitter_seconds,
                    );

                    info!(
                        expires_in_seconds = aws.expires_in_seconds,
                        sleep_for_seconds = sleep_for,
                        "token minted"
                    );
                    sleep(Duration::from_secs(sleep_for)).await;
                } else {
                    warn!("mint succeeded but no AWS token returned; retrying");
                    sleep(Duration::from_secs(cfg.min_refresh_seconds)).await;
                }
            }
            Err(e) => {
                warn!(error=?e, "mint failed; retrying");
                sleep(Duration::from_secs(cfg.min_refresh_seconds)).await;
            }
        }
    }
}

/// Compute how many seconds to sleep before refreshing the token. Formula: expires_in - skew - jitter.
fn compute_sleep_seconds(expires_in: u64, skew: u64, min_sleep: u64, max_jitter: u64) -> u64 {
    let j = compute_jitter(max_jitter);
    let target = expires_in.saturating_sub(skew).saturating_sub(j);
    target.max(min_sleep)
}

fn compute_jitter(max: u64) -> u64 {
    if max == 0 {
        return 0;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    nanos % (max + 1)
}
