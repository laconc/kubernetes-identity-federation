use std::{env, path::PathBuf};

use anyhow::{Context, Result};

use kif_api::shared;

#[derive(Clone, Debug)]
pub struct AgentConfig {
    pub port: u16,
    pub federation_url: String,
    pub service_account_name: String,
    pub namespace: String,

    pub sa_token_path: PathBuf,
    pub aws_token_path: PathBuf,

    pub refresh_skew_seconds: u64,
    pub min_refresh_seconds: u64,
    pub max_jitter_seconds: u64,
}

impl AgentConfig {
    pub fn from_env() -> Result<Self> {
        let port = env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5004);

        let federation_url =
            env::var("FEDERATION_URL").unwrap_or("http://kif-federation:5001".to_string());

        let service_account_name =
            env::var("SERVICE_ACCOUNT_NAME").context("SERVICE_ACCOUNT_NAME is required")?;

        let sa_token_path = env::var("SA_TOKEN_PATH")
            .map(PathBuf::from)
            .unwrap_or(PathBuf::from(
                "/var/run/secrets/kubernetes.io/serviceaccount/token",
            ));

        let aws_token_path = env::var("AWS_TOKEN_PATH")
            .map(PathBuf::from)
            .unwrap_or(PathBuf::from("/var/run/kif/aws/token"));

        let namespace = shared::pod_namespace()?;

        let refresh_skew_seconds = env::var("REFRESH_SKEW_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);

        let min_refresh_seconds = env::var("MIN_REFRESH_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);

        let max_jitter_seconds = env::var("JITTER_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);

        Ok(Self {
            port,
            federation_url,
            service_account_name,
            namespace,
            sa_token_path,
            aws_token_path,
            refresh_skew_seconds,
            min_refresh_seconds,
            max_jitter_seconds,
        })
    }
}
