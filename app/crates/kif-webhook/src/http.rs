use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router, http::StatusCode, routing::get};
use k8s_openapi::api::core::v1::Pod;
use kube::core::admission::AdmissionReview;
use tracing::warn;

use crate::admission::AppState;

pub fn health_router() -> Router {
    Router::new()
        .route("/livez", get(ok))
        .route("/startupz", get(ok))
}

async fn ok() -> StatusCode {
    StatusCode::OK
}

pub fn admission_router(state: AppState) -> Router {
    Router::new()
        .route("/mutate", post(mutate_handler))
        .with_state(state)
}

async fn mutate_handler(
    State(state): State<AppState>,
    Json(review): Json<AdmissionReview<Pod>>,
) -> impl IntoResponse {
    match crate::admission::handle(review, state).await {
        Ok(resp) => (StatusCode::OK, Json(resp)).into_response(),
        Err(e) => {
            warn!(error=?e, "admission handler error");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
