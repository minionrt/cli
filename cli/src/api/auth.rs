use std::sync::Arc;

use axum::body::Body;
use axum::http::header::AUTHORIZATION;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::api::AppState;

/// Middleware for Bearer authentication.
pub async fn bearer_auth_middleware(
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let state = req
        .extensions()
        .get::<Arc<AppState>>()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let auth_header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    let token = auth_header
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");

    if token == state.ctx.agent_api_key {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
