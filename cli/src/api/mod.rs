use std::net::TcpListener;

use actix_web::{middleware, web, App, HttpServer};
use actix_web_httpauth::middleware::HttpAuthentication;
use tokio::sync::{oneshot, Mutex};

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

pub async fn run_server(listener: TcpListener, ctx: Context) -> anyhow::Result<TaskOutcome> {
    let ctx = web::Data::new(ctx);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<TaskOutcome>();
    let shutdown_tx = web::Data::new(Mutex::new(Some(shutdown_tx)));

    let server = HttpServer::new(move || {
        let bearer_auth = HttpAuthentication::bearer(auth::bearer_auth_validator);

        App::new()
            .app_data(ctx.clone())
            .app_data(shutdown_tx.clone())
            .service(git_proxy::scope(
                "/api/agent/git",
                git::basic_auth_validator,
            ))
            .service(
                web::scope("/api")
                    .wrap(bearer_auth)
                    .service(agent::scope())
                    .service(chat::scope()),
            )
            .service(probes::readiness)
            .service(probes::healthz)
            .wrap(middleware::NormalizePath::new(
                middleware::TrailingSlash::Trim,
            ))
            .wrap(middleware::Logger::default())
    });

    let server = server
        .listen(listener)
        .map_err(|e| anyhow::anyhow!(e))?
        .run();

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
