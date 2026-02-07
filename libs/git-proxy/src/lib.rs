//! A simple Git proxy integration for Axum that forwards Git requests to a Git server.
//! It supports the Git v2 wire protocol via the smart HTTP transfer protocol.
//! In other words, most modern Git clients should work with this proxy over HTTP.
//! For authentication, currently only HTTP Basic Authentication is supported, both for the proxy itself and for the upstream Git server.
//!
//! # Usage Example
//!
//! For a basic usage example, see the `basic` example in the `examples` directory.
//!
//! # How it Works
//!
//! 1. Client requests (e.g. `git clone`, `git push`, `git fetch`) are sent to
//!    your Axum server at the path defined in [`scope`].
//! 2. An optional Basic Authentication check (the validator you provide) runs,
//!    ensuring the request is authorized to access the proxy.
//!    This check needs to supply a [`ProxyBehaivor`] instance to the request extensions
//!    which will tell the proxy how to forward the Git requests.
//! 3. The proxy inspects the request body of push requests to apply any configured restrictions.
//!    Currently, push requests are restricted to a single specific ref (e.g. branch) configured by `allowed_ref`.
//!    deletion and creation of refs is forbidden.
//! 4. The proxy routes the request to the corresponding Git endpoints (`info/refs`,
//!    `git-receive-pack`, `git-upload-pack`) and relays the response from the upstream server back to the client.
//!
//! # References
//!
//! For more details on the Git HTTP protocol and the wire protocol v2, see:
//!
//! - [Git HTTP protocol documentation](https://git-scm.com/docs/http-protocol)
//! - [Git wire protocol v2 documentation](https://git-scm.com/docs/protocol-v2)

use std::future::Future;
use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::{from_fn, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use headers::authorization::Basic;
use headers::{Authorization, HeaderMapExt};
use url::Url;

mod compression;
mod packet_line;
mod routes;

use routes::{git_receive_pack_handler, git_upload_pack_handler, info_refs_handler};

pub use headers::authorization::Basic as BasicAuth;

/// What the proxy should do with the request.
///
/// # Usage
///
/// In your authentication validator function, supply an instance of this struct to the Axum request extensions.
/// For usage examples, see the `examples` directory.
#[derive(Clone)]
pub struct ProxyBehaivor {
    /// A reference (e.g., "refs/heads/main") indicating which ref/branch is allowed to be updated during a push operation.
    /// Pushes to other refs will be denied.
    pub allowed_ref: String,
    /// How to forward the request.
    pub forward: Forward,
}

/// How to forward the request.
#[derive(Clone)]
pub enum Forward {
    /// Forward the request to another Git server.
    ForwardToRemote(ForwardToRemote),
    /// Forward the request to a local Git repository.
    ForwardToLocal(ForwardToLocal),
}

impl From<ForwardToRemote> for Forward {
    fn from(f: ForwardToRemote) -> Self {
        Forward::ForwardToRemote(f)
    }
}

impl From<ForwardToLocal> for Forward {
    fn from(f: ForwardToLocal) -> Self {
        Forward::ForwardToLocal(f)
    }
}

/// Forward Git requests to another server.
#[derive(Clone)]
pub struct ForwardToRemote {
    /// The upstream Git server's URL to which Git commands are forwarded.
    pub url: Url,
    /// The username used for Basic Authentication with the upstream server.
    pub basic_auth_user: String,
    /// The password used for Basic Authentication with the upstream server.
    pub basic_auth_pass: String,
}

/// Forward Git requests to a local Git repository.
#[derive(Clone)]
pub struct ForwardToLocal {
    pub path: PathBuf,
}

/// Create an Axum `Router` configured to handle the v2 wire protocol over the Git smart HTTP transfer protocol.
///
/// This function sets up the necessary routes (`info/refs`, `git-receive-pack`,
/// and `git-upload-pack`) under the given `path`, and applies a Basic
/// Authentication middleware using the provided validator function.
///
/// # Invariant
///
/// The `basic_auth_validator` function **MUST** insert a `ProxyBehaivor` instance into the request extensions.
/// **Otherwise the proxy will panic!**
///
/// # Arguments
///
/// * `path`                 - The base path under which the Git routes will be mounted (e.g., "/git").
/// * `basic_auth_validator` - A function that validates Basic Authentication credentials for each request.
/// # Returns
///
/// A `Router` containing the configured Git routes and middleware,
/// ready to be nested within an Axum `Router`.
pub fn scope<O, F>(path: &str, basic_auth_validator: F) -> Router
where
    F: Fn(Request<Body>, BasicAuth) -> O + Clone + Send + Sync + 'static,
    O: Future<Output = Result<Request<Body>, ProxyError>> + Send + 'static,
{
    let validator = basic_auth_validator.clone();

    let routes = Router::new()
        .route("/info/refs", get(info_refs_handler))
        .route("/git-receive-pack", post(git_receive_pack_handler))
        .route("/git-upload-pack", post(git_upload_pack_handler))
        .route_layer(from_fn(move |req, next| {
            let validator = validator.clone();
            async move { basic_auth_middleware(req, next, validator).await }
        }));

    if path.is_empty() || path == "/" {
        routes
    } else {
        Router::new().nest(path, routes)
    }
}

async fn basic_auth_middleware<F, O>(
    req: Request<Body>,
    next: Next,
    validator: F,
) -> Result<Response, ProxyError>
where
    F: Fn(Request<Body>, BasicAuth) -> O + Clone + Send + Sync + 'static,
    O: Future<Output = Result<Request<Body>, ProxyError>> + Send + 'static,
{
    let auth = req
        .headers()
        .typed_get::<Authorization<Basic>>()
        .ok_or_else(|| ProxyError::unauthorized("Missing Authorization header"))?;

    let req = validator(req, auth.0).await?;
    Ok(next.run(req).await)
}

#[derive(Debug)]
pub struct ProxyError {
    status: StatusCode,
    message: Option<String>,
}

impl ProxyError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: Some(message.into()),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: Some(message.into()),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: Some(message.into()),
        }
    }
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let mut builder = Response::builder().status(self.status);
        if self.status == StatusCode::UNAUTHORIZED {
            builder = builder.header(axum::http::header::WWW_AUTHENTICATE, "Basic");
        }
        let body = self.message.unwrap_or_default();
        builder.body(Body::from(body)).unwrap()
    }
}

pub type ProxyResult<T> = Result<T, ProxyError>;
