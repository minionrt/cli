use std::env;

use actix_web::dev::ServiceRequest;
use actix_web::error::ErrorUnauthorized;
use actix_web::{App, Error, HttpMessage, HttpServer};
use actix_web_httpauth::extractors::basic::BasicAuth;

use git_proxy::{scope, ForwardToRemote, ProxyBehaivor};

// Hard-coded Basic Auth credentials for demonstration.
const USER: &str = "my-user";
const TOKEN: &str = "my-token";
const ALLOWED_REF: &str = "refs/heads/allow";

/// Validate BasicAuth credentials and, if valid, store a `ProxyBehaivor` in the request extensions.
async fn basic_auth_validator(
    req: ServiceRequest,
    credentials: BasicAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let user = credentials.user_id();
    let pass = credentials.password().unwrap_or("");

    if user == USER && pass == TOKEN {
        let raw_repo_url = env::var("FORWARD_REPO").expect("FORWARD_REPO must be set");
        let url = raw_repo_url
            .parse()
            .expect("FORWARD_REPO must be a valid URL");

        let auth_user = "x-access-token".to_string();
        let auth_pass = env::var("FORWARD_TOKEN").expect("FORWARD_TOKEN must be set");

        req.extensions_mut().insert(ProxyBehaivor {
            allowed_ref: ALLOWED_REF.to_string(),
            forward: ForwardToRemote {
                url,
                basic_auth_user: auth_user,
                basic_auth_pass: auth_pass,
            }
            .into(),
        });

        Ok(req)
    } else {
        Err((ErrorUnauthorized("Invalid username or password"), req))
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    let bind_addr = "127.0.0.1:8080";
    println!("Starting Git proxy on http://{}", bind_addr);

    HttpServer::new(move || App::new().service(scope("", basic_auth_validator)))
        .bind(bind_addr)?
        .run()
        .await
}
