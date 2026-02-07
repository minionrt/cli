use actix_web::{Error, HttpRequest};
use url::Url;

use crate::requests::CompletionRequest;

#[allow(async_fn_in_trait)]
pub trait ProxyConfig: Send + Sync + 'static {
    /// The type of context extracted from the incoming request.
    type Context;

    /// Extract any necessary context from the incoming request.
    async fn extract_context(&self, req: &HttpRequest) -> Result<Self::Context, Error>;

    /// Configure how to forward a `CompletionRequest`.
    async fn forward(
        &self,
        ctx: &Self::Context,
        req: &CompletionRequest,
    ) -> Result<ForwardConfig, Error>;

    /// Optionally handle the interaction after the reqest has been forwarded.
    /// In a streaming scenario, the response will be `None`.
    async fn inspect_interaction(
        &self,
        ctx: &Self::Context,
        req: &CompletionRequest,
        response: Option<serde_json::Value>,
    );
}

/// How to forward a `CompletionRequest`
pub struct ForwardConfig {
    pub api_key: String,
    pub target_url: Url,
    pub model: Option<String>,
}
