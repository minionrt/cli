use std::io::{self, Write};
use std::path::{Path, PathBuf};

use agent_client_protocol::{
    Client, ContentBlock, ReadTextFileRequest, ReadTextFileResponse, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome,
    SessionNotification, SessionUpdate, WriteTextFileRequest, WriteTextFileResponse,
};
use async_trait::async_trait;

use crate::AcpResult;

#[derive(Debug, Clone)]
pub struct ACPClient {
    root: PathBuf,
}

impl ACPClient {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        }
    }
}

#[async_trait(?Send)]
impl Client for ACPClient {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> AcpResult<RequestPermissionResponse> {
        let outcome = args
            .options
            .first()
            .map(|option| {
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                    option.option_id.clone(),
                ))
            })
            .unwrap_or(RequestPermissionOutcome::Cancelled);
        Ok(RequestPermissionResponse::new(outcome))
    }

    async fn session_notification(&self, args: SessionNotification) -> AcpResult<()> {
        self.forward_session_notification(&args);
        Ok(())
    }

    async fn read_text_file(&self, args: ReadTextFileRequest) -> AcpResult<ReadTextFileResponse> {
        let path = self.resolve_path(&args.path);
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(anyhow::Error::from)?;
        let sliced = slice_lines(content, args.line, args.limit);
        Ok(ReadTextFileResponse::new(sliced))
    }

    async fn write_text_file(
        &self,
        args: WriteTextFileRequest,
    ) -> AcpResult<WriteTextFileResponse> {
        let path = self.resolve_path(&args.path);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(anyhow::Error::from)?;
        }
        tokio::fs::write(&path, args.content)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(WriteTextFileResponse::new())
    }
}

impl ACPClient {
    fn forward_session_notification(&self, notification: &SessionNotification) {
        match &notification.update {
            SessionUpdate::UserMessageChunk(chunk) | SessionUpdate::AgentMessageChunk(chunk) => {
                forward_content_to_stdout(&chunk.content);
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                forward_content_to_stderr(&chunk.content);
            }
            update => {
                eprintln!(
                    "[acp2rt] session update {:?}: {:?}",
                    notification.session_id, update
                );
            }
        }
    }
}

fn forward_content_to_stdout(content: &ContentBlock) {
    match content {
        ContentBlock::Text(text) => {
            print!("{}", text.text);
            let _ = io::stdout().flush();
        }
        other => {
            println!("{other:?}");
        }
    }
}

fn forward_content_to_stderr(content: &ContentBlock) {
    match content {
        ContentBlock::Text(text) => {
            eprint!("{}", text.text);
            let _ = io::stderr().flush();
        }
        other => {
            eprintln!("{other:?}");
        }
    }
}

fn slice_lines(content: String, line: Option<u32>, limit: Option<u32>) -> String {
    let start = line.unwrap_or(1).saturating_sub(1) as usize;
    let limit = limit.map(|val| val as usize);
    let lines: Vec<&str> = content.lines().collect();
    if start >= lines.len() {
        return String::new();
    }
    let end = match limit {
        Some(max) => std::cmp::min(start + max, lines.len()),
        None => lines.len(),
    };
    lines[start..end].join("\n")
}
