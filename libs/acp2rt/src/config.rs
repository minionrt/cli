use std::path::PathBuf;
use std::sync::Arc;

use tokio::process::Command as TokioCommand;
use url::Url;

pub struct AgentConfig {
    pub acp_command: Arc<dyn Fn() -> TokioCommand + Send + Sync>,
    pub api_base_url: Url,
    pub api_token: String,
    pub workspace_path: PathBuf,
}

impl AgentConfig {
    pub fn new(
        acp_command: impl Fn() -> TokioCommand + Send + Sync + 'static,
        api_base_url: Url,
        api_token: impl Into<String>,
        workspace_path: impl Into<PathBuf>,
    ) -> Self {
        let workspace_path = workspace_path.into();
        let workspace_path = if workspace_path.is_absolute() {
            workspace_path
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(workspace_path)
        };
        Self {
            acp_command: Arc::new(acp_command),
            api_base_url,
            api_token: api_token.into(),
            workspace_path,
        }
    }
}
