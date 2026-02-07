use axum::body::Body;
use axum::http::Request;
use axum::Router;
use git_proxy::{scope, BasicAuth, ForwardToRemote, ProxyBehaivor, ProxyError};
use std::env;

// Hard-coded Basic Auth credentials for demonstration.
const USER: &str = "my-user";
const TOKEN: &str = "my-token";
const ALLOWED_REF: &str = "refs/heads/allow";

/// Validate BasicAuth credentials and, if valid, store a `ProxyBehaivor` in the request extensions.
async fn basic_auth_validator(
    mut req: Request<Body>,
    credentials: BasicAuth,
) -> Result<Request<Body>, ProxyError> {
    let user = credentials.username();
    let pass = credentials.password();

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
        Err(ProxyError::unauthorized("Invalid username or password"))
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    let bind_addr = "127.0.0.1:8080";
    println!("Starting Git proxy on http://{}", bind_addr);

    let app = Router::new().merge(scope("", basic_auth_validator));

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await
}
