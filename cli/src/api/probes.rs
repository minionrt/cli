use axum::routing::get;
use axum::Router;

pub fn router() -> Router {
    Router::new()
        .route("/ready", get(readiness))
        .route("/healthz", get(healthz))
}

async fn readiness() -> &'static str {
    "Ready"
}

async fn healthz() -> &'static str {
    "Healthy"
}
