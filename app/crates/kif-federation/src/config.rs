use std::env;
use std::sync::Arc;

use anyhow::{Context, Result};
use kube::Client;
use openidconnect::IssuerUrl;
use tokio::sync::RwLock;

use crate::{jwt, k8s};
use kif_api::shared;

const RSA_BITS: usize = 3072;

#[derive(Clone)]
pub struct AppState {
    pub client: Client,

    /// Public issuer URL
    pub issuer_url: IssuerUrl,

    /// HTTP port
    pub port: u16,

    /// The signing material is updated by the k8s watcher and read by the HTTP handlers
    pub signing: Arc<RwLock<jwt::SigningMaterial>>,
}

impl AppState {
    pub async fn from_env() -> Result<Self> {
        let issuer_url = env::var("ISSUER_URL")
            .context("ISSUER_URL is required")
            .and_then(|s| IssuerUrl::new(s).context("invalid ISSUER_URL"))?;

        let port = env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5001);

        let signing_secret_name =
            env::var("SIGNING_SECRET_NAME").unwrap_or_else(|_| "kif-signing".to_string());
        let jwks_secret_name =
            env::var("JWKS_SECRET_NAME").unwrap_or_else(|_| "kif-jwks".to_string());

        let namespace = shared::pod_namespace()?;
        let client = Client::try_default().await?;

        // Ensure the signing and jwks secrets exist and are properly initialized.
        // This will create them if they don't exist, or fix them if they're
        // misconfigured, like a missing key.
        let signing_material = k8s::ensure_signing_and_jwks(
            &client,
            &namespace,
            &signing_secret_name,
            &jwks_secret_name,
            RSA_BITS,
        )
        .await
        .context("failed to ensure signing/jwks secrets")?;

        Ok(Self {
            client,
            issuer_url,
            port,
            signing: Arc::new(RwLock::new(signing_material)),
        })
    }
}
