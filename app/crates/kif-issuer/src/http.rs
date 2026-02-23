use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use openidconnect::core::CoreJwsSigningAlgorithm;
use openidconnect::{
    AuthUrl, EmptyAdditionalProviderMetadata, JsonWebKeySetUrl, ResponseTypes,
    core::{CoreProviderMetadata, CoreResponseType, CoreSubjectIdentifierType},
};
use tracing::error;

use crate::config::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(openid_discovery_doc),
        )
        .route("/jwks.json", get(get_jwks))
        .route("/startupz", get(startupz))
        .route("/livez", get(livez))
        .with_state(state)
}

async fn livez() -> impl IntoResponse {
    StatusCode::OK
}

async fn startupz(State(state): State<AppState>) -> impl IntoResponse {
    if state.jwks.is_loaded().await {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

// Serve the OpenID Connect discovery doc.
async fn openid_discovery_doc(State(state): State<AppState>) -> impl IntoResponse {
    let err_resp = || (StatusCode::INTERNAL_SERVER_ERROR).into_response();

    let auth_url = match AuthUrl::new(format!("{}/authorize", state.cfg.issuer_url)) {
        Ok(url) => url,
        Err(e) => {
            error!("invalid issuer URL: {}", e);
            return err_resp();
        }
    };

    let jwks_uri = match JsonWebKeySetUrl::new(format!("{}/jwks.json", state.cfg.issuer_url)) {
        Ok(uri) => uri,
        Err(e) => {
            error!("invalid JWKS URI: {}", e);
            return err_resp();
        }
    };

    let metadata = CoreProviderMetadata::new(
        state.cfg.issuer_url.clone(),
        auth_url,
        jwks_uri,
        vec![ResponseTypes::new(vec![CoreResponseType::IdToken])],
        vec![CoreSubjectIdentifierType::Pairwise],
        vec![CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256],
        EmptyAdditionalProviderMetadata {},
    );

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert(
        header::CACHE_CONTROL,
        "public, max-age=3600".parse().unwrap(),
    );

    (headers, Json(metadata)).into_response()
}

// Serve the JWKS data.
async fn get_jwks(State(state): State<AppState>) -> impl IntoResponse {
    match state.jwks.get().await {
        Some(jwks) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
            headers.insert(
                header::CACHE_CONTROL,
                "public, max-age=300".parse().unwrap(),
            );

            let body = match serde_json::to_string(&jwks) {
                Ok(body) => body,
                Err(e) => {
                    error!("failed to serialize JWKS: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR).into_response();
                }
            };
            (StatusCode::OK, headers, body).into_response()
        }
        None => (StatusCode::SERVICE_UNAVAILABLE).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IssuerConfig;
    use crate::jwks::JwksStore;
    use openidconnect::{IssuerUrl, core::CoreJsonWebKeySet};
    use reqwest::StatusCode as HttpStatus;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    fn test_cfg(base: &str) -> IssuerConfig {
        IssuerConfig {
            issuer_url: IssuerUrl::new(base.to_string()).unwrap(),
            running_in_cluster: false,
            port: 0,
            jwks_secret_name: "".to_string(),
            jwks_file_path: "".to_string(),
        }
    }

    #[tokio::test]
    async fn serves_endpoints() {
        let jwks_json = r#"{"keys":[{"kty":"RSA","kid":"test-kid","e":"AQAB","n":"abc","use":"sig","alg":"RS256"}]}"#;
        let jwks: CoreJsonWebKeySet = serde_json::from_str(jwks_json).unwrap();

        let jwks_store = JwksStore::new();
        jwks_store.set(jwks).await;

        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let base = format!("http://{}", addr);

        let state = AppState {
            cfg: test_cfg(&base),
            jwks: jwks_store,
        };
        let app = router(state);

        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = reqwest::Client::new();

        // startupz should be OK
        let r = client
            .get(format!("{}/startupz", base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), HttpStatus::OK);

        // jwks.json should include kid
        let r = client
            .get(format!("{}/jwks.json", base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), HttpStatus::OK);
        let body = r.text().await.unwrap();
        assert!(body.contains(r#""kid":"test-kid""#));

        // Discovery doc should include issuer and jwks_uri
        let r = client
            .get(format!("{}/.well-known/openid-configuration", base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), HttpStatus::OK);
        let disc: serde_json::Value = r.json().await.unwrap();
        assert_eq!(disc["issuer"].as_str().unwrap(), base);
        assert_eq!(
            disc["jwks_uri"].as_str().unwrap(),
            format!("{}/jwks.json", base)
        );

        server.abort();
    }
}
