use reqwest::Url;

use crate::result::Result;
use crate::types::task::*;

#[derive(Debug, Clone)]
pub struct Client {
    base_url: Url,
    token: String,
    client: reqwest::Client,
}

impl Client {
    pub fn new(base_url: Url, token: String) -> Self {
        Self {
            base_url,
            token,
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_task(&self) -> Result<Task> {
        let url = self.base_url.join("agent/task")?;
        let response = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()?
            .json::<Task>()
            .await?;
        Ok(response)
    }

    pub async fn complete_task(&self, task_complete: TaskComplete) -> Result<()> {
        let url = self.base_url.join("agent/task/complete")?;
        self.client
            .post(url)
            .bearer_auth(&self.token)
            .json(&task_complete)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn fail_task(&self, task_fail: TaskFailure) -> Result<()> {
        let url = self.base_url.join("agent/task/fail")?;
        self.client
            .post(url)
            .bearer_auth(&self.token)
            .json(&task_fail)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}
