use rand::{distr::Alphanumeric, RngExt as _};
use url::Url;

use crate::config::LLMRouterTable;

pub struct Context {
    /// LLM API configuration and secrets.
    pub llm_router_table: LLMRouterTable,
    /// Randomly generated key supplied to the agent.
    pub agent_api_key: String,
    /// The user's task description.
    pub task_description: String,
    /// The git username to use for commits.
    /// This is *not* the username of the user, but a machine-generated username.
    pub git_user_name: String,
    /// The git email to use for commits.
    /// This is *not* the email of the user, but a machine-generated email.
    pub git_user_email: String,
    /// The git repository URL for the agent to clone.
    /// Valid inside the agent's container.
    pub git_repo_url: Url,
    /// The git branch to clone.
    pub git_branch: String,
    /// The path to the git repository on the host machine.
    pub git_repo_path: std::path::PathBuf,
}

/// Generate a random API key.
pub fn random_key() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}
