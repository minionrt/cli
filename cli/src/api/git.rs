use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;

use git_proxy::{BasicAuth, ForwardToLocal, ProxyBehaivor, ProxyError};

use crate::api::AppState;

/// Validator function for Basic authentication
pub async fn basic_auth_validator(
    mut req: Request<Body>,
    credentials: BasicAuth,
) -> Result<Request<Body>, ProxyError> {
    let state = req
        .extensions()
        .get::<Arc<AppState>>()
        .cloned()
        .ok_or_else(|| ProxyError::internal("Missing app state"))?;

    let password = credentials.password();

    if password == state.ctx.agent_api_key {
        req.extensions_mut().insert(ProxyBehaivor {
            allowed_ref: format!("refs/heads/{}", state.ctx.git_branch.clone()),
            forward: ForwardToLocal {
                path: state.ctx.git_repo_path.clone(),
            }
            .into(),
        });
        Ok(req)
    } else {
        Err(ProxyError::unauthorized("Invalid username or password"))
    }
}
