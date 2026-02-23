use axum::{Router, extract::State, http::StatusCode, routing::get};
use tokio::sync::watch;

#[derive(Clone)]
pub struct HttpState {
    pub ready_rx: watch::Receiver<bool>,
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
    if *state.ready_rx.borrow() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
