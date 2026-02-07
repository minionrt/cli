use std::net::TcpListener;
use std::sync::Arc;

use axum::middleware;
use axum::Extension;
use axum::Router;
use tokio::sync::{oneshot, Mutex};
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::trace::TraceLayer;

use crate::context::Context;

mod agent;
mod auth;
mod chat;
mod git;
mod probes;

#[derive(Debug, PartialEq)]
pub enum TaskOutcome {
    Completed,
    Failure,
}

pub struct AppState {
    pub ctx: Arc<Context>,
    pub shutdown_tx: Mutex<Option<oneshot::Sender<TaskOutcome>>>,
    pub server_shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
}

pub async fn run_server(listener: TcpListener, ctx: Context) -> anyhow::Result<TaskOutcome> {
    let ctx = Arc::new(ctx);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<TaskOutcome>();
    let (server_shutdown_tx, server_shutdown_rx) = oneshot::channel::<()>();

    let state = Arc::new(AppState {
        ctx: ctx.clone(),
        shutdown_tx: Mutex::new(Some(shutdown_tx)),
        server_shutdown_tx: Mutex::new(Some(server_shutdown_tx)),
    });

    let api_router = Router::new()
        .merge(agent::router())
        .merge(chat::router(ctx.clone()))
        .route_layer(middleware::from_fn(auth::bearer_auth_middleware));

    let app = Router::new()
        .merge(git_proxy::scope(
            "/api/agent/git",
            git::basic_auth_validator,
        ))
        .nest("/api", api_router)
        .merge(probes::router())
        .layer(Extension(state))
        .layer(NormalizePathLayer::trim_trailing_slash())
        .layer(TraceLayer::new_for_http());

    listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(listener)?;

    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        let _ = server_shutdown_rx.await;
    });

    tokio::select! {
        res = server => res.map_err(|e| anyhow::anyhow!(e)).map(|()| TaskOutcome::Failure),
        outcome = shutdown_rx => outcome.map_err(|e| anyhow::anyhow!(e)),
    }
}

pub async fn wait_until_ready(base_url: &str) -> Result<(), reqwest::Error> {
    loop {
        match reqwest::get(format!("{base_url}/ready")).await {
            Ok(res) if res.status().is_success() => return Ok(()),
            _ => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
        }
    }
}
