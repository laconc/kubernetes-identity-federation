use std::env;

use anyhow::{Context, Result};
use openidconnect::IssuerUrl;

use crate::jwks::JwksStore;

#[derive(Clone)]
pub struct AppState {
    pub cfg: IssuerConfig,
    pub jwks: JwksStore,
}

#[derive(Clone, Debug)]
pub struct IssuerConfig {
    /// Public issuer URL
    pub issuer_url: IssuerUrl,

    /// If true, use an informer to watch for changes to the jwks Secret
    pub running_in_cluster: bool,

    /// HTTP port
    pub port: u16,

    /// Secret name containing jwks.json (when running in cluster)
    pub jwks_secret_name: String,

    /// File path to jwks.json
    pub jwks_file_path: String,
}

impl IssuerConfig {
    pub fn from_env() -> Result<Self> {
        let issuer_url = env::var("ISSUER_URL")
            .context("ISSUER_URL is required")
            .and_then(|s| IssuerUrl::new(s).context("invalid ISSUER_URL"))?;

        let running_in_cluster = env::var("RUNNING_IN_CLUSTER")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);

        let port = env::var("PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(5002);

        let jwks_secret_name = env::var("JWKS_SECRET_NAME").unwrap_or("kif-jwks".to_string());

        let jwks_file_path = env::var("JWKS_FILE_PATH").context("JWKS_FILE_PATH is required")?;

        Ok(Self {
            issuer_url,
            running_in_cluster,
            port,
            jwks_secret_name,
            jwks_file_path,
        })
    }
}
