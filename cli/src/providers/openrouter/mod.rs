use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::Redirect;
use axum::routing::get;
use axum::Extension;
use axum::Router;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use once_cell::sync::Lazy;
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use url::Url;

use crate::config::Config;

/// OpenRouter’s authorization URL.
static OAUTH_AUTHORIZE_URL: Lazy<Url> =
    Lazy::new(|| Url::parse("https://openrouter.ai/auth").unwrap());

/// OpenRouter’s auth key endpoint.
static AUTH_KEY_URL: Lazy<Url> =
    Lazy::new(|| Url::parse("https://openrouter.ai/api/v1/auth/keys").unwrap());

/// Context for the auth flow
#[derive(Clone)]
pub struct Context {
    pub config: Config,
    pub code_verifier: String,
    pub web_base_url: Url,
}

struct OpenRouterState {
    context: Context,
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
}

/// Start a temporary web server for the OpenRouter auth flow.
pub async fn login_flow(config: Config) -> anyhow::Result<()> {
    let host = "127.0.0.1";
    let port = 3000;
    let bind_addr = format!("{host}:{port}");
    let web_base_url = Url::parse(&format!("http://{bind_addr}")).unwrap();

    println!("The login page should open in your default web browser.");
    println!(
        "If it doesn't, please visit: {}",
        web_base_url.join("/auth/openrouter").unwrap()
    );

    let context = Context {
        config,
        code_verifier: code_verifier(),
        web_base_url: web_base_url.clone(),
    };

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let state = Arc::new(OpenRouterState {
        context,
        shutdown_tx: Mutex::new(Some(shutdown_tx)),
    });

    let app = Router::new()
        .route("/auth/openrouter", get(openrouter_connect))
        .route("/auth/openrouter/auth-code", get(openrouter_auth_code))
        .layer(Extension(state.clone()));

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        let _ = shutdown_rx.await;
    });

    // Spawn a task to open the login flow URL in the browser after a brief delay.
    // This delay gives the server time to start.
    let login_url = web_base_url.join("/auth/openrouter").unwrap();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if let Err(err) = webbrowser::open(login_url.as_str()) {
            eprintln!("Failed to open browser: {err}");
        }
    });

    server.await?;

    Ok(())
}

/// This endpoint initiates the OpenRouter login flow by redirecting the user’s browser.
/// It uses the precomputed PKCE verifier from the context to derive the code challenge.
async fn openrouter_connect(Extension(state): Extension<Arc<OpenRouterState>>) -> Redirect {
    let code_challenge = code_challenge(&state.context.code_verifier);

    let mut location = OAUTH_AUTHORIZE_URL.clone();
    let callback_url = state
        .context
        .web_base_url
        .join("/auth/openrouter/auth-code")
        .unwrap();
    location
        .query_pairs_mut()
        .append_pair("callback_url", callback_url.as_str())
        .append_pair("code_challenge", code_challenge.as_str())
        .append_pair("code_challenge_method", "S256");

    Redirect::temporary(location.as_str())
}

/// Query parameters for the auth-code callback.
#[derive(Deserialize)]
struct AuthCodeQuery {
    code: String,
}

/// This endpoint is the callback for the OpenRouter auth flow. It receives the
/// authorization code, uses the stored PKCE verifier to request an auth key,
/// saves that key in the config file, and then notifies the user.
async fn openrouter_auth_code(
    Extension(state): Extension<Arc<OpenRouterState>>,
    Query(query): Query<AuthCodeQuery>,
) -> Result<String, (StatusCode, String)> {
    let code_verifier = &state.context.code_verifier;
    let key = match auth_key(&query.code, code_verifier).await {
        Ok(key) => key,
        Err(err) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get auth key: {err}"),
            ));
        }
    };

    // Update the configuration with the obtained key and save it.
    let mut config = state.context.config.clone();
    config.openrouter_key = Some(key);
    if config.llm_provider.is_none() {
        println!("OpenRouter is now your default LLM provider.");
        config.llm_provider = Some(crate::config::LLMProvider::OpenRouter);
    }
    if let Err(err) = config.save() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save config: {err}"),
        ));
    }

    println!();
    println!("Authentication successful.");
    println!("Your OpenRouter API key has been saved to the config file at:");
    println!(
        "{}",
        Config::filepath()
            .expect("Failed to get config file path")
            .to_string_lossy()
    );

    if let Some(tx) = state.shutdown_tx.lock().await.take() {
        tx.send(()).expect("Failed to send shutdown signal");
    }

    Ok("Authentication successful! You can close this window.".to_string())
}

/// Requests an auth key from OpenRouter by exchanging the authorization code
/// (and using the provided PKCE verifier) at the OpenRouter auth key endpoint.
pub async fn auth_key(code: &str, code_verifier: &str) -> reqwest::Result<String> {
    let request = AuthKeyRequest {
        code,
        code_verifier,
        code_challenge_method: "S256",
    };
    let response = reqwest::Client::new()
        .post(AUTH_KEY_URL.as_str())
        .json(&request)
        .send()
        .await?;
    let response = response.error_for_status()?;
    let response = response.json::<AuthKeyResponse>().await?;
    Ok(response.key)
}

/// Request payload to exchange the code for an auth key.
#[derive(Serialize)]
struct AuthKeyRequest<'a> {
    code: &'a str,
    code_verifier: &'a str,
    code_challenge_method: &'a str,
}

/// Response payload containing the auth key.
#[derive(Deserialize)]
struct AuthKeyResponse {
    key: String,
}

/// Generate a random PKCE code verifier of length exactly 128 Base64 characters (no padding).
///
/// We use 96 raw bytes because 96 * 4/3 = 128 (without padding).
pub fn code_verifier() -> String {
    let mut verifier_bytes = [0u8; 96];
    rand::rng().fill_bytes(&mut verifier_bytes);
    URL_SAFE_NO_PAD.encode(verifier_bytes)
}

/// Compute the PKCE code challenge from the code verifier as a Base64-url-encoded
/// (no padding) SHA-256 hash.
pub fn code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}
