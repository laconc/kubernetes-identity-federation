use std::sync::Arc;

use anyhow::bail;
use jsonwebtoken::jwk::JwkSet;
use tokio::sync::RwLock;

/// Store for the JWKS data.
#[derive(Clone, Default)]
pub struct JwksStore {
    inner: Arc<RwLock<Option<JwkSet>>>,
}

impl JwksStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn is_loaded(&self) -> bool {
        self.inner.read().await.is_some()
    }

    pub async fn get(&self) -> Option<JwkSet> {
        self.inner.read().await.clone()
    }

    pub async fn set(&self, jwks: JwkSet) {
        *self.inner.write().await = Some(jwks);
    }
}

pub fn parse_jwks(bytes: Vec<u8>) -> anyhow::Result<JwkSet> {
    let jwks_json = str::from_utf8(bytes.as_slice())?;
    let jwks: JwkSet = serde_json::from_str(jwks_json)?;

    if jwks.keys.is_empty() {
        bail!("attempted to set empty JWKS; ignoring");
    }
    Ok(jwks)
}
