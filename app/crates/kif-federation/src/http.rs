use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::{AppState, jwt, k8s};

const TOKEN_TTL_SECONDS: u64 = 3600;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/livez", get(livez))
        .route("/v1/mint", post(mint))
        .route("/v1/rotate-keys", post(rotate_keys)) // NOT IMPLEMENTED
        .with_state(state)
}

async fn livez() -> impl IntoResponse {
    StatusCode::OK
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MintRequest {
    pub namespace: String,
    pub service_account_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MintResponse {
    pub aws: Option<MintedToken>,
    // future: azure, gcp
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MintedToken {
    pub token: String,
    pub expires_in_seconds: u64,
}

async fn mint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<MintRequest>,
) -> impl IntoResponse {
    // Extract bearer token from Authorization header
    let bearer = match headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        Some(v) if v.starts_with("Bearer ") => &v["Bearer ".len()..],
        _ => return (StatusCode::UNAUTHORIZED, "missing bearer token\n").into_response(),
    };

    // TokenReview: authenticate the caller
    let subject = match k8s::token_review(&state.client, bearer).await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                format!("token review failed: {e}\n"),
            )
                .into_response();
        }
    };

    let (ns, sa) = match k8s::parse_service_account_username(&subject.username) {
        Some(x) => x,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                "not a valid ServiceAccount token\n",
            )
                .into_response();
        }
    };

    // Check that the subject of the token matches the requested namespace and service account
    if ns != req.namespace || sa != req.service_account_name {
        return (
            StatusCode::FORBIDDEN,
            format!(
                "subject mismatch: token is {ns}/{sa} requested {}/{}\n",
                req.namespace, req.service_account_name
            ),
        )
            .into_response();
    }

    // Get the ResolvedCloudRoleBinding, which contains the information needed to mint a token for this request
    let resolved_binding =
        match k8s::get_resolved_binding(&state.client, &req.namespace, &req.service_account_name)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::NOT_FOUND,
                    format!(
                        "no ResolvedCloudRoleBinding found for {}/{}: {e}\n",
                        req.namespace, req.service_account_name
                    ),
                )
                    .into_response();
            }
        };

    let aws_cfg = match resolved_binding.spec.providers.aws {
        Some(a) => a,
        None => return (StatusCode::BAD_REQUEST, "no AWS provider configured\n").into_response(),
    };

    let audience = aws_cfg.audience.unwrap_or("sts.amazonaws.com".to_string());

    // Determine whether to include provenance information in the token
    let include_provenance = resolved_binding
        .spec
        .attributes
        .as_ref()
        .and_then(|a| a.include_provenance)
        .unwrap_or(true);

    let extra_attrs = resolved_binding
        .spec
        .attributes
        .as_ref()
        .and_then(|a| a.extra.clone());

    let claims = jwt::AwsClaims::new(
        &state.issuer_url,
        &subject.username,
        &audience,
        TOKEN_TTL_SECONDS,
        include_provenance,
        &req.namespace,
        &req.service_account_name,
        extra_attrs,
    );

    let signing = state.signing.read().await.clone();
    let token = match jwt::sign_rs256(&signing, &claims) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to sign token: {e}\n"),
            )
                .into_response();
        }
    };

    let resp = MintResponse {
        aws: Some(MintedToken {
            token,
            expires_in_seconds: TOKEN_TTL_SECONDS,
        }),
    };

    (StatusCode::OK, Json(resp)).into_response()
}

// Rotate keys outside the standard rotation period, such as in response to a security incident.
async fn rotate_keys(State(_state): State<AppState>) -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}
