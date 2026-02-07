//! This example creates a temporary local git repository and serves it over HTTP with Basic Auth.

use std::path::PathBuf;

use actix_web::dev::ServiceRequest;
use actix_web::error::ErrorUnauthorized;
use actix_web::{App, Error, HttpMessage, HttpServer};
use actix_web_httpauth::extractors::basic::BasicAuth;
use git_proxy::{scope, ForwardToLocal, ProxyBehaivor};

use git2::Repository;
use tempfile::TempDir;

// Hard-coded Basic Auth credentials for demonstration.
const USER: &str = "my-user";
const TOKEN: &str = "my-token";
const ALLOWED_REF: &str = "refs/heads/allow";

#[derive(Clone)]
struct AppConfig {
    repo_path: PathBuf,
}

/// Validate BasicAuth credentials and, if valid, store a `ProxyBehaivor` in the request extensions.
async fn basic_auth_validator(
    req: ServiceRequest,
    credentials: BasicAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let user = credentials.user_id();
    let pass = credentials.password().unwrap_or("");

    if user == USER && pass == TOKEN {
        let app_config = req.app_data::<AppConfig>().expect("AppConfig not found");
        req.extensions_mut().insert(ProxyBehaivor {
            allowed_ref: ALLOWED_REF.to_string(),
            forward: ForwardToLocal {
                path: app_config.repo_path.clone(),
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
    let temp_dir = TempDir::new().expect("Failed to create temporary directory");
    let repo_path = temp_dir.path();
    println!("Temporary git repository at: {:?}", repo_path);

    Repository::init(repo_path).expect("Failed to initialize git repository");

    let app_config = AppConfig {
        repo_path: repo_path.to_owned(),
    };

    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();
    let bind_addr = "127.0.0.1:8080";
    println!("Starting Git proxy on http://{}", bind_addr);

    HttpServer::new(move || {
        App::new()
            .app_data(app_config.clone())
            .service(scope("", basic_auth_validator))
    })
    .bind(bind_addr)?
    .run()
    .await
}
