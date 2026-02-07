use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Deserialize, Serialize)]
pub struct Task {
    pub status: TaskStatus,
    pub description: String,
    pub git_user_name: String,
    pub git_user_email: String,
    pub git_repo_url: Url,
    pub git_branch: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Deserialize, Serialize)]
pub struct TaskComplete {
    pub description: String,
}

#[derive(Deserialize, Serialize)]
pub struct TaskFailure {
    pub reason: Option<TaskFailureReason>,
    pub description: String,
}

#[derive(Deserialize, Serialize, Copy, Clone)]
pub enum TaskFailureReason {
    /// The agent failed to complete the task due to technical problems unrelated to the task itself
    TechnicalIssues,
    /// The agent failed to complete the task due to a problem with the task itself, e.g. because
    /// the task in unclear or impossible to complete
    TaskIssues,
    /// There were no fundamental technical issues and the task was valid, but the agent still failed
    /// to complete the task because it did not succeed at task-specific problem-solving.
    ProblemSolving,
}
