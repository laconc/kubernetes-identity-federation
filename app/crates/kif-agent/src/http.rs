use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axum::{Router, extract::State, http::StatusCode, routing::get};

#[derive(Clone)]
pub struct HttpState {
    pub ready: Arc<AtomicBool>,
}

pub fn router(state: HttpState) -> Router {
    Router::new()
        .route("/livez", get(livez))
        .route("/startupz", get(startupz))
        .with_state(state)
}

async fn livez() -> StatusCode {
    StatusCode::OK
}

async fn startupz(State(state): State<HttpState>) -> StatusCode {
    if state.ready.load(Ordering::Relaxed) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
