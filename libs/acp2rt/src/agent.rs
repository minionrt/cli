use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_api::Client as AgentApiClient;
use agent_api::types::task::{Task, TaskComplete, TaskFailure, TaskFailureReason};
use agent_client_protocol::{
    Agent as AcpAgent, ClientCapabilities, ClientSideConnection, ContentBlock,
    FileSystemCapability, Implementation, InitializeRequest, NewSessionRequest, PromptRequest,
    ProtocolVersion, SessionId, TextContent,
};
use anyhow::{Context, Result};
use tokio::process::Command as TokioCommand;
use tokio::task::{JoinHandle, LocalSet};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::{ACPClient, AcpResult, AgentConfig};

pub struct Agent {
    api: AgentApiClient,
    api_token: String,
    command_factory: Arc<dyn Fn() -> TokioCommand + Send + Sync>,
    workspace_path: PathBuf,
}

impl Agent {
    pub fn new(config: AgentConfig) -> Self {
        let api_token = config.api_token;
        Self {
            api: AgentApiClient::new(config.api_base_url, api_token.clone()),
            api_token,
            command_factory: config.acp_command,
            workspace_path: config.workspace_path,
        }
    }

    pub async fn run_once(&self) -> Result<RunOutcome> {
        let task = self.api.get_task().await?;
        let workspace = self.setup_workspace(&task).await?;

        let outcome = {
            let local = LocalSet::new();
            local.run_until(self.run_prompt(task, workspace)).await
        };

        match outcome {
            Ok(outcome) => {
                self.commit_and_push(&outcome.task).await?;
                let description = format!("Completed task via ACP session {}", outcome.session_id);
                let payload = TaskComplete { description };
                self.api.complete_task(payload).await?;
                Ok(outcome)
            }
            Err(err) => {
                let description = format!("Task failed: {err}");
                let reason = Some(TaskFailureReason::TechnicalIssues);
                let payload = TaskFailure {
                    reason,
                    description,
                };
                let _ = self.api.fail_task(payload).await;
                Err(err)
            }
        }
    }

    async fn run_prompt(&self, task: Task, workspace: PathBuf) -> Result<RunOutcome> {
        let (connection, mut child, io_handle) = self.spawn_acp(&workspace).await?;
        let init = InitializeRequest::new(ProtocolVersion::LATEST)
            .client_capabilities(
                ClientCapabilities::new().fs(FileSystemCapability::new()
                    .read_text_file(true)
                    .write_text_file(true)),
            )
            .client_info(Implementation::new("acp2rt", env!("CARGO_PKG_VERSION")));
        let _ = connection.initialize(init).await?;

        let session = connection
            .new_session(NewSessionRequest::new(&workspace))
            .await?;
        let session_id = session.session_id;
        let prompt = vec![ContentBlock::Text(TextContent::new(
            task.description.clone(),
        ))];
        let response = connection
            .prompt(PromptRequest::new(session_id.clone(), prompt))
            .await?;

        if let Err(err) = child.kill().await
            && err.kind() != std::io::ErrorKind::InvalidInput
        {
            return Err(err).context("failed to stop ACP agent process");
        }
        if let Err(err) = io_handle.await? {
            return Err(err).context("ACP I/O task failed");
        }

        Ok(RunOutcome {
            task,
            workspace,
            session_id,
            prompt_response: response,
        })
    }

    async fn setup_workspace(&self, task: &Task) -> Result<PathBuf> {
        if self.workspace_path.exists() {
            anyhow::bail!(
                "workspace path already exists: {}",
                self.workspace_path.display()
            );
        }
        if let Some(parent) = self.workspace_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut git_url = task.git_repo_url.clone();
        git_url
            .set_username("x-access-token")
            .map_err(|_| anyhow::anyhow!("failed to set git URL username"))?;
        git_url
            .set_password(Some(self.api_token.as_str()))
            .map_err(|_| anyhow::anyhow!("failed to set git URL password"))?;

        self.run_git(&[
            "clone",
            "--branch",
            task.git_branch.as_str(),
            git_url.as_str(),
            self.workspace_path
                .to_str()
                .context("workspace path is not valid UTF-8")?,
        ])
        .await?;

        self.run_git(&[
            "-C",
            self.workspace_path
                .to_str()
                .context("workspace path is not valid UTF-8")?,
            "config",
            "user.name",
            task.git_user_name.as_str(),
        ])
        .await?;

        self.run_git(&[
            "-C",
            self.workspace_path
                .to_str()
                .context("workspace path is not valid UTF-8")?,
            "config",
            "user.email",
            task.git_user_email.as_str(),
        ])
        .await?;

        Ok(self.workspace_path.clone())
    }

    async fn commit_and_push(&self, task: &Task) -> Result<()> {
        let workspace = self
            .workspace_path
            .to_str()
            .context("workspace path is not valid UTF-8")?;

        self.run_git(&["-C", workspace, "add", "-A"]).await?;
        self.run_git(&[
            "-C",
            workspace,
            "commit",
            "--allow-empty",
            "-m",
            "Complete task",
        ])
        .await?;
        self.run_git(&["-C", workspace, "push", "origin", task.git_branch.as_str()])
            .await?;
        Ok(())
    }

    async fn run_git(&self, args: &[&str]) -> Result<()> {
        let output = TokioCommand::new("git").args(args).output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            anyhow::bail!(
                "git command failed (status={}): {}{}",
                output.status,
                stdout,
                stderr
            );
        }
        Ok(())
    }

    async fn spawn_acp(
        &self,
        workspace: &Path,
    ) -> Result<(
        ClientSideConnection,
        tokio::process::Child,
        JoinHandle<AcpResult<()>>,
    )> {
        let mut cmd = (self.command_factory)();
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .current_dir(workspace);

        let mut child = cmd.spawn().context("failed to spawn ACP agent")?;
        let stdin = child.stdin.take().context("ACP agent stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("ACP agent stdout unavailable")?;

        let client = ACPClient::new(workspace.to_path_buf());
        let (connection, io_task) =
            ClientSideConnection::new(client, stdin.compat_write(), stdout.compat(), |fut| {
                tokio::task::spawn_local(fut);
            });
        let io_handle = tokio::task::spawn_local(io_task);

        Ok((connection, child, io_handle))
    }
}

pub struct RunOutcome {
    pub task: Task,
    pub workspace: PathBuf,
    pub session_id: SessionId,
    pub prompt_response: agent_client_protocol::PromptResponse,
}
