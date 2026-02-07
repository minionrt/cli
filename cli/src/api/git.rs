use actix_web::dev::ServiceRequest;
use actix_web::error::{Error, ErrorUnauthorized};
use actix_web::web;
use actix_web::HttpMessage;
use actix_web_httpauth::extractors::basic::BasicAuth;

use git_proxy::{ForwardToLocal, ProxyBehaivor};

use crate::context::Context;

/// Validator function for Basic authentication
pub async fn basic_auth_validator(
    req: ServiceRequest,
    credentials: BasicAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let ctx = req
        .app_data::<web::Data<Context>>()
        .expect("Context not found in app data");

    let password = credentials.password().unwrap_or("");

    if password == ctx.agent_api_key {
        req.extensions_mut().insert(ProxyBehaivor {
            allowed_ref: format!("refs/heads/{}", ctx.git_branch.clone()),
            forward: ForwardToLocal {
                path: ctx.git_repo_path.clone(),
            }
            .into(),
        });
        Ok(req)
    } else {
        Err((ErrorUnauthorized("Invalid username or password"), req))
    }
}
