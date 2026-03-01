use std::sync::Arc;

use anyhow::{Result, bail};
use openidconnect::core::CoreJsonWebKeySet;
use tokio::sync::RwLock;

/// Store for the JWKS data.
#[derive(Clone, Default)]
pub struct JwksStore {
    inner: Arc<RwLock<Option<CoreJsonWebKeySet>>>,
}

impl JwksStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn is_loaded(&self) -> bool {
        self.inner.read().await.is_some()
    }

    pub async fn get(&self) -> Option<CoreJsonWebKeySet> {
        self.inner.read().await.clone()
    }

    pub async fn set(&self, jwks: CoreJsonWebKeySet) {
        *self.inner.write().await = Some(jwks);
    }
}

pub fn parse_jwks(bytes: Vec<u8>) -> Result<CoreJsonWebKeySet> {
    let jwks_json = str::from_utf8(bytes.as_slice())?;
    let jwks: CoreJsonWebKeySet = serde_json::from_str(jwks_json)?;

    if jwks.keys().is_empty() {
        bail!("attempted to set empty JWKS; ignoring");
    }
    Ok(jwks)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Standard RSA test key from RFC 7517 Appendix C (public parameters only)
    const VALID_JWKS: &str = r#"{"keys":[{"kty":"RSA","kid":"test-1","use":"sig","alg":"RS256","n":"0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw","e":"AQAB"}]}"#;

    #[test]
    fn parse_jwks_valid() {
        let result = parse_jwks(VALID_JWKS.as_bytes().to_vec());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().keys().len(), 1);
    }

    #[test]
    fn parse_jwks_empty_keys_rejected() {
        let empty = r#"{"keys":[]}"#;
        let result = parse_jwks(empty.as_bytes().to_vec());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty JWKS"));
    }

    #[test]
    fn parse_jwks_invalid_json_rejected() {
        let result = parse_jwks(b"not json at all".to_vec());
        assert!(result.is_err());
    }
}
