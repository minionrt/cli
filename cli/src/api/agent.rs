use std::io::{self, Write};
use std::sync::Arc;

use axum::extract::Json;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Extension;
use axum::Router;
use serde::Deserialize;

use agent_api::types::task::*;

use crate::api::{AppState, TaskOutcome};

#[derive(Deserialize)]
pub struct InquiryPayload {
    pub inquiry: String,
}

pub fn router() -> Router {
    Router::new()
        .route("/agent/task", get(task_info))
        .route("/agent/task/complete", post(task_complete))
        .route("/agent/task/fail", post(task_fail))
        .route("/agent/inquiry", post(inquiry))
}

pub async fn task_info(Extension(state): Extension<Arc<AppState>>) -> Json<Task> {
    let response = Task {
        status: TaskStatus::Running,
        description: state.ctx.task_description.clone(),
        git_user_name: state.ctx.git_user_name.clone(),
        git_user_email: state.ctx.git_user_email.clone(),
        git_repo_url: state.ctx.git_repo_url.clone(),
        git_branch: state.ctx.git_branch.clone(),
    };

    Json(response)
}

pub async fn task_complete(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<TaskComplete>,
) -> StatusCode {
    println!("Task completed");
    println!("{}", body.description);

    if let Some(tx) = state.shutdown_tx.lock().await.take() {
        tx.send(TaskOutcome::Completed)
            .expect("Failed to send shutdown signal");
    }

    if let Some(tx) = state.server_shutdown_tx.lock().await.take() {
        tx.send(()).expect("Failed to send server shutdown signal");
    }

    StatusCode::OK
}

pub async fn task_fail(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<TaskFailure>,
) -> StatusCode {
    println!("Task failed");
    println!("{}", body.description);

    if let Some(tx) = state.shutdown_tx.lock().await.take() {
        tx.send(TaskOutcome::Failure)
            .expect("Failed to send shutdown signal");
    }

    if let Some(tx) = state.server_shutdown_tx.lock().await.take() {
        tx.send(()).expect("Failed to send server shutdown signal");
    }

    StatusCode::OK
}

/// Send an inquiry to the user and await its answer.
/// Agents use this endpoint to request clarification on their tasks.
pub async fn inquiry(Json(request): Json<InquiryPayload>) -> Json<String> {
    let question = request.inquiry.clone();

    println!("Agent is asking: {question}");

    let answer = (tokio::task::spawn_blocking(move || {
        print!("Your answer: ");
        io::stdout().flush().ok();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            input.trim().to_string()
        } else {
            String::new()
        }
    })
    .await)
        .unwrap_or_default();

    Json(answer)
}
