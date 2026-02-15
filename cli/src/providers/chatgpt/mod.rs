use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::Redirect;
use axum::routing::get;
use axum::Extension;
use axum::Router;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use once_cell::sync::Lazy;
use rand::Rng as _;
use serde::Deserialize;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::{oneshot, Mutex};
use url::Url;

use crate::config::{Config, LLMProvider};

const DEFAULT_AUTH_PORT: u16 = 1455;
const REFRESH_INTERVAL_DAYS: i64 = 8;

// Matches Codex's OAuth client id.
const CHATGPT_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

static OAUTH_AUTHORIZE_URL: Lazy<Url> =
    Lazy::new(|| Url::parse("https://auth.openai.com/oauth/authorize").unwrap());
static OAUTH_TOKEN_URL: Lazy<Url> =
    Lazy::new(|| Url::parse("https://auth.openai.com/oauth/token").unwrap());

#[derive(Clone)]
struct Context {
    config: Config,
    state: String,
    code_verifier: String,
    web_base_url: Url,
}

struct ChatGptState {
    context: Context,
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
}

#[derive(Deserialize)]
struct AuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize)]
struct RefreshResponse {
    id_token: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct IdClaims {
    #[serde(rename = "https://api.openai.com/auth", default)]
    auth: Option<AuthClaims>,
}

#[derive(Deserialize)]
struct AuthClaims {
    #[serde(default)]
    chatgpt_account_id: Option<String>,
}

#[derive(Serialize)]
struct RefreshRequest<'a> {
    client_id: &'a str,
    grant_type: &'a str,
    refresh_token: &'a str,
    scope: &'a str,
}

pub async fn login_flow(config: Config) -> anyhow::Result<()> {
    let listener = bind_listener(DEFAULT_AUTH_PORT).await?;
    let bind_addr = listener.local_addr()?;
    let web_base_url = Url::parse(&format!("http://localhost:{}", bind_addr.port())).unwrap();

    println!("ChatGPT login should open in your default web browser.");
    println!(
        "If it doesn't, please visit: {}",
        web_base_url.join("/auth/chatgpt").unwrap()
    );

    let context = Context {
        config,
        state: csrf_state(),
        code_verifier: code_verifier(),
        web_base_url: web_base_url.clone(),
    };

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let state = Arc::new(ChatGptState {
        context,
        shutdown_tx: Mutex::new(Some(shutdown_tx)),
    });

    let app = Router::new()
        .route("/auth/chatgpt", get(chatgpt_connect))
        .route("/auth/callback", get(chatgpt_callback))
        .layer(Extension(state.clone()));

    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        let _ = shutdown_rx.await;
    });

    let login_url = web_base_url.join("/auth/chatgpt").unwrap();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if let Err(err) = webbrowser::open(login_url.as_str()) {
            eprintln!("Failed to open browser: {err}");
        }
    });

    server.await?;
    Ok(())
}

pub async fn refresh_if_needed(config: &mut Config) -> anyhow::Result<()> {
    if !matches!(config.llm_provider, Some(LLMProvider::ChatGpt)) {
        return Ok(());
    }

    let Some(refresh_token) = config.chatgpt_refresh_token.clone() else {
        return Ok(());
    };

    if let Some(last_refresh) = config.chatgpt_last_refresh_unix_secs {
        let age_secs = now_unix_secs().saturating_sub(last_refresh);
        if age_secs < REFRESH_INTERVAL_DAYS * 24 * 60 * 60 {
            return Ok(());
        }
    }

    let response = reqwest::Client::new()
        .post(OAUTH_TOKEN_URL.as_str())
        .header("Content-Type", "application/json")
        .json(&RefreshRequest {
            client_id: CHATGPT_OAUTH_CLIENT_ID,
            grant_type: "refresh_token",
            refresh_token: &refresh_token,
            scope: "openid profile email",
        })
        .send()
        .await?
        .error_for_status()?
        .json::<RefreshResponse>()
        .await?;

    if let Some(id_token) = response.id_token {
        config.chatgpt_account_id = extract_account_id_from_id_token(&id_token);
        config.chatgpt_id_token = Some(id_token);
    }
    if let Some(access_token) = response.access_token {
        config.chatgpt_access_token = Some(access_token);
    }
    if let Some(refresh_token) = response.refresh_token {
        config.chatgpt_refresh_token = Some(refresh_token);
    }
    config.chatgpt_last_refresh_unix_secs = Some(now_unix_secs());
    config.save()?;

    Ok(())
}

async fn chatgpt_connect(Extension(state): Extension<Arc<ChatGptState>>) -> Redirect {
    let authorize_url = build_authorize_url(&state.context);
    Redirect::temporary(authorize_url.as_str())
}

async fn chatgpt_callback(
    Extension(state): Extension<Arc<ChatGptState>>,
    Query(query): Query<AuthCallbackQuery>,
) -> Result<String, (StatusCode, String)> {
    if let Some(error) = query.error {
        let detail = query.error_description.unwrap_or_default();
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Login failed: {error} {detail}"),
        ));
    }

    if query.state.as_deref() != Some(state.context.state.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "State mismatch".to_string()));
    }

    let Some(code) = query.code else {
        return Err((
            StatusCode::BAD_REQUEST,
            "Missing authorization code".to_string(),
        ));
    };

    let callback_url = state.context.web_base_url.join("/auth/callback").unwrap();
    let tokens =
        exchange_code_for_tokens(&code, &state.context.code_verifier, callback_url.as_str())
            .await
            .map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to exchange authorization code: {err}"),
                )
            })?;

    let mut config = state.context.config.clone();
    config.chatgpt_account_id = extract_account_id_from_id_token(&tokens.id_token);
    config.chatgpt_id_token = Some(tokens.id_token);
    config.chatgpt_access_token = Some(tokens.access_token);
    config.chatgpt_refresh_token = Some(tokens.refresh_token);
    config.chatgpt_last_refresh_unix_secs = Some(now_unix_secs());
    if config.llm_provider.is_none() {
        println!("ChatGPT is now your default LLM provider.");
        config.llm_provider = Some(LLMProvider::ChatGpt);
    }
    config.save().map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save config: {err}"),
        )
    })?;

    println!();
    println!("Authentication successful.");
    println!("Your ChatGPT tokens have been saved to the config file at:");
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

fn build_authorize_url(context: &Context) -> Url {
    let code_challenge = code_challenge(&context.code_verifier);
    let callback_url = context.web_base_url.join("/auth/callback").unwrap();

    let mut location = OAUTH_AUTHORIZE_URL.clone();
    location
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CHATGPT_OAUTH_CLIENT_ID)
        .append_pair("scope", "openid profile email offline_access")
        .append_pair("code_challenge", code_challenge.as_str())
        .append_pair("code_challenge_method", "S256")
        .append_pair("redirect_uri", callback_url.as_str())
        .append_pair("state", context.state.as_str());
    location
}

async fn exchange_code_for_tokens(
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> reqwest::Result<TokenResponse> {
    reqwest::Client::new()
        .post(OAUTH_TOKEN_URL.as_str())
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
            urlencoding::encode(code),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(CHATGPT_OAUTH_CLIENT_ID),
            urlencoding::encode(code_verifier)
        ))
        .send()
        .await?
        .error_for_status()?
        .json::<TokenResponse>()
        .await
}

async fn bind_listener(start_port: u16) -> anyhow::Result<tokio::net::TcpListener> {
    for port in start_port..=u16::MAX {
        if let Ok(listener) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            return Ok(listener);
        }
    }

    Err(anyhow::anyhow!(
        "Failed to bind local auth server to a free port"
    ))
}

fn csrf_state() -> String {
    let mut state_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut state_bytes);
    URL_SAFE_NO_PAD.encode(state_bytes)
}

fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn code_verifier() -> String {
    let mut verifier_bytes = [0u8; 96];
    rand::rng().fill_bytes(&mut verifier_bytes);
    URL_SAFE_NO_PAD.encode(verifier_bytes)
}

fn code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

fn extract_account_id_from_id_token(id_token: &str) -> Option<String> {
    // JWT format is "header.payload.signature"; we only need payload claims.
    let payload_b64 = id_token.split('.').nth(1)?;
    let payload = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let claims: IdClaims = serde_json::from_slice(&payload).ok()?;
    claims.auth.and_then(|auth| auth.chatgpt_account_id)
}
