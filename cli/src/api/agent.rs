use std::io::{self, Write};

use actix_web::Scope;
use actix_web::{get, post, web, HttpResponse};
use serde::Deserialize;
use tokio::sync::{oneshot, Mutex};

use agent_api::types::task::*;

use crate::api::TaskOutcome;
use crate::context::Context;

#[derive(Deserialize)]
pub struct InquiryPayload {
    pub inquiry: String,
}

pub fn scope() -> Scope {
    Scope::new("/agent")
        .service(task_info)
        .service(task_complete)
        .service(task_fail)
        .service(inquiry)
}

#[get("/task")]
pub async fn task_info(ctx: web::Data<Context>) -> HttpResponse {
    let response = Task {
        status: TaskStatus::Running,
        description: ctx.task_description.clone(),
        git_user_name: ctx.git_user_name.clone(),
        git_user_email: ctx.git_user_email.clone(),
        git_repo_url: ctx.git_repo_url.clone(),
        git_branch: ctx.git_branch.clone(),
    };

    HttpResponse::Ok().json(response)
}

#[post("/task/complete")]
pub async fn task_complete(
    body: web::Json<TaskComplete>,
    shutdown_tx: web::Data<Mutex<Option<oneshot::Sender<TaskOutcome>>>>,
) -> HttpResponse {
    let body = body.into_inner();
    println!("Task completed");
    println!("{}", body.description);

    let tx = shutdown_tx
        .lock()
        .await
        .take()
        .expect("Failed to acquire lock for shutdown signal");
    tx.send(TaskOutcome::Completed)
        .expect("Failed to send shutdown signal");

    HttpResponse::Ok().finish()
}

#[post("/task/fail")]
pub async fn task_fail(
    body: web::Json<TaskFailure>,
    shutdown_tx: web::Data<Mutex<Option<oneshot::Sender<TaskOutcome>>>>,
) -> HttpResponse {
    println!("Task failed");
    println!("{}", body.description);

    let tx = shutdown_tx
        .lock()
        .await
        .take()
        .expect("Failed to acquire lock for shutdown signal");
    tx.send(TaskOutcome::Failure)
        .expect("Failed to send shutdown signal");

    HttpResponse::Ok().finish()
}
/// Send an inquiry to the user and await its answer.
/// Agents use this endpoint to request clarification on their tasks.
#[post("/inquiry")]
pub async fn inquiry(request: web::Json<InquiryPayload>) -> HttpResponse {
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

    HttpResponse::Ok().json(answer)
}
