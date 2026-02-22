use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use openidconnect::{core::{CoreProviderMetadata, CoreResponseType, CoreSubjectIdentifierType}, AuthUrl, EmptyAdditionalProviderMetadata, IssuerUrl, JsonWebKeySetUrl, ResponseTypes};
use openidconnect::core::CoreJwsSigningAlgorithm;
use crate::{config::IssuerConfig, jwks::JwksStore};

#[derive(Clone)]
pub struct AppState {
    pub cfg: IssuerConfig,
    pub jwks: JwksStore,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/.well-known/openid-configuration", get(openid_discovery_doc))
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
    let issuer = IssuerUrl::new(state.cfg.issuer_url.clone()).unwrap();
    let auth_url = AuthUrl::new(format!("{}/authorize", state.cfg.issuer_url)).unwrap();
    let jwks_uri = JsonWebKeySetUrl::new(format!("{}/jwks.json", state.cfg.issuer_url)).unwrap();

    let metadata = CoreProviderMetadata::new(
        issuer,
        auth_url,
        jwks_uri,
        vec![ResponseTypes::new(vec![CoreResponseType::IdToken])],
        vec![CoreSubjectIdentifierType::Pairwise],
        vec![CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256],
        EmptyAdditionalProviderMetadata {},
    );

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "public, max-age=3600".parse().unwrap());

    (headers, Json(metadata))
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

            let body = serde_json::to_string(&jwks).expect("JWKS serialization failed");
            (StatusCode::OK, headers, body).into_response()
        }
        None => (StatusCode::SERVICE_UNAVAILABLE).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::jwk::JwkSet;
    use reqwest::StatusCode as HttpStatus;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    fn test_cfg(base: &str) -> IssuerConfig {
        IssuerConfig {
            issuer_url: base.trim_end_matches('/').to_string(),
            running_in_cluster: false,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn serves_endpoints() {
        let jwks_json = r#"{"keys":[{"kty":"RSA","kid":"test-kid","e":"AQAB","n":"abc","use":"sig","alg":"RS256"}]}"#;
        let jwks: JwkSet = serde_json::from_str(jwks_json).unwrap();

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

        // discovery doc should include issuer and jwks_uri
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
