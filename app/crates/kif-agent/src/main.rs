mod config;
mod federation;
mod http;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use rand::RngExt;
use tokio::{net::TcpListener, time::sleep};
use tracing::{error, info, warn};

use config::AgentConfig;
use federation::{MintError, MintRequest};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cfg = AgentConfig::from_env()?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("build reqwest client")?;

    // Ready channel to signal when the first successful token mint has occurred, for readyz.
    // We don't want to start the main containers until we have our minted tokens.
    let ready = Arc::new(AtomicBool::new(false));

    let port = cfg.port;
    let refresh_handle = tokio::spawn(refresh_loop(cfg, client, Arc::clone(&ready)));

    let http_state = http::HttpState { ready };
    let app = http::router(http_state);
    let bind_addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&bind_addr).await?;
    info!(%bind_addr, "agent server up");

    tokio::select! {
        result = axum::serve(listener, app) => {
            result?;
        }
        result = refresh_handle => {
            result??;
        }
    }

    Ok(())
}

/// Main loop to refresh tokens. On each iteration, it reads the ServiceAccount token,
/// calls the federation API to mint a new AWS token, writes the AWS token to disk,
/// and sleeps until it's time to refresh again.
async fn refresh_loop(
    cfg: AgentConfig,
    client: reqwest::Client,
    ready: Arc<AtomicBool>,
) -> Result<()> {
    let mut first_success = false;

    let req = MintRequest {
        namespace: cfg.namespace.clone(),
        service_account_name: cfg.service_account_name.clone(),
        config_hash: cfg.config_hash.clone(),
        pod_name: cfg.pod_name.clone(),
    };

    loop {
        let sa_token = match tokio::fs::read_to_string(&cfg.sa_token_path).await {
            Ok(token) => token.trim().to_string(),
            Err(e) => {
                warn!(
                    error = ?e,
                    path = %cfg.sa_token_path.display(),
                    "failed reading ServiceAccount token; retrying"
                );
                sleep(Duration::from_secs(cfg.min_refresh_seconds)).await;
                continue;
            }
        };
        if sa_token.is_empty() {
            warn!("ServiceAccount token file was empty; retrying");
            sleep(Duration::from_secs(cfg.min_refresh_seconds)).await;
            continue;
        }

        match federation::mint(&client, &cfg.federation_url, &sa_token, &req).await {
            Ok(resp) => {
                if let Some(aws) = resp.aws {
                    federation::atomic_write(&cfg.aws_token_path, &aws.token).with_context(
                        || {
                            format!(
                                "failed writing AWS token to {}",
                                cfg.aws_token_path.display()
                            )
                        },
                    )?;

                    if !first_success {
                        first_success = true;
                        ready.store(true, Ordering::Relaxed);
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
            Err(MintError::ConfigHashMismatch) => {
                error!(
                    "the config has changed since this Pod was admitted; restart the Pod to pick up the recent changes"
                );
                return Err(anyhow!("config hash mismatch"));
            }
            Err(MintError::Other(e)) => {
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
    let mut rng = rand::rng();
    rng.random_range(0..=max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_sleep_normal_case() {
        // With jitter=0: sleep = expires_in - skew = 3600 - 300 = 3300
        let sleep = compute_sleep_seconds(3600, 300, 60, 0);
        assert_eq!(sleep, 3300);
    }

    #[test]
    fn compute_sleep_saturating_skew_clamps_to_min() {
        // skew > expires_in → saturating_sub → 0, clamped to min
        let sleep = compute_sleep_seconds(100, 200, 60, 0);
        assert_eq!(sleep, 60);
    }

    #[test]
    fn compute_sleep_never_below_min() {
        let sleep = compute_sleep_seconds(0, 0, 30, 0);
        assert_eq!(sleep, 30);
    }

    #[test]
    fn compute_jitter_zero_when_max_zero() {
        assert_eq!(compute_jitter(0), 0);
    }

    #[test]
    fn compute_jitter_within_range() {
        for _ in 0..100 {
            let j = compute_jitter(60);
            assert!(j <= 60);
        }
    }
}
