use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router, http::StatusCode, routing::get};
use k8s_openapi::api::core::v1::Pod;
use kube::core::admission::AdmissionReview;
use tracing::warn;

use crate::admission::AppState;

pub fn health_router(ready: Arc<AtomicBool>) -> Router {
    Router::new()
        .route("/livez", get(livez))
        .route("/startupz", get(startupz))
        .with_state(ready)
}

async fn livez() -> StatusCode {
    StatusCode::OK
}

async fn startupz(State(ready): State<Arc<AtomicBool>>) -> StatusCode {
    if ready.load(Ordering::Relaxed) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

pub fn admission_router(state: AppState) -> Router {
    Router::new()
        .route("/mutate", post(mutate))
        .with_state(state)
}

async fn mutate(
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
