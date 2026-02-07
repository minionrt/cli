use actix_web::dev::ServiceRequest;
use actix_web::error::{Error, ErrorUnauthorized};
use actix_web::web;
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::context::Context;

/// Validator function for Bearer authentication
pub async fn bearer_auth_validator(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let ctx = req
        .app_data::<web::Data<Context>>()
        .expect("Context not found in app data");

    if credentials.token() == ctx.agent_api_key {
        Ok(req)
    } else {
        Err((ErrorUnauthorized("Invalid API key"), req))
    }
}
