//! This example creates a temporary local git repository and serves it over HTTP with Basic Auth.

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use axum::Extension;
use axum::Router;
use git_proxy::{scope, BasicAuth, ForwardToLocal, ProxyBehaivor, ProxyError};

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
    mut req: Request<Body>,
    credentials: BasicAuth,
) -> Result<Request<Body>, ProxyError> {
    let user = credentials.username();
    let pass = credentials.password();

    if user == USER && pass == TOKEN {
        let app_config = req
            .extensions()
            .get::<Arc<AppConfig>>()
            .cloned()
            .expect("AppConfig not found");
        req.extensions_mut().insert(ProxyBehaivor {
            allowed_ref: ALLOWED_REF.to_string(),
            forward: ForwardToLocal {
                path: app_config.repo_path.clone(),
            }
            .into(),
        });
        Ok(req)
    } else {
        Err(ProxyError::unauthorized("Invalid username or password"))
    }
}

#[tokio::main]
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

    let app = Router::new()
        .merge(scope("", basic_auth_validator))
        .layer(Extension(Arc::new(app_config)));

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await
}
