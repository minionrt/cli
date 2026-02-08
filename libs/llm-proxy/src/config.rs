use async_trait::async_trait;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use url::Url;

use crate::requests::CompletionRequest;

#[async_trait]
pub trait ProxyConfig: Send + Sync + 'static {
    /// The type of context extracted from the incoming request.
    type Context: Clone + Send + Sync + 'static;

    /// Extract any necessary context from the incoming request.
    async fn extract_context(&self, headers: &HeaderMap) -> ProxyResult<Self::Context>;

    /// Configure how to forward a `CompletionRequest`.
    async fn forward(
        &self,
        ctx: &Self::Context,
        req: &CompletionRequest,
    ) -> ProxyResult<ForwardConfig>;

    /// Configure how to forward an OpenAI Responses API request.
    async fn forward_responses(
        &self,
        _ctx: &Self::Context,
        _req: &serde_json::Value,
    ) -> ProxyResult<ForwardConfig> {
        Err(ProxyError::bad_request(
            "Responses API forwarding is not configured",
        ))
    }

    /// Configure how to forward an OpenAI Models API request.
    async fn forward_models(&self, _ctx: &Self::Context) -> ProxyResult<ForwardConfig> {
        Err(ProxyError::bad_request(
            "Models API forwarding is not configured",
        ))
    }

    /// Optionally handle the interaction after the reqest has been forwarded.
    /// In a streaming scenario, the response will be `None`.
    async fn inspect_interaction(
        &self,
        ctx: &Self::Context,
        req: &CompletionRequest,
        response: Option<serde_json::Value>,
    );

    /// Optionally handle OpenAI Responses API interactions after forwarding.
    /// In a streaming scenario, the response will be `None`.
    async fn inspect_responses_interaction(
        &self,
        _ctx: &Self::Context,
        _req: &serde_json::Value,
        _response: Option<serde_json::Value>,
    ) {
    }
}

#[derive(Debug)]
pub struct ProxyError {
    status: StatusCode,
    message: Option<String>,
}

impl ProxyError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: Some(message.into()),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: Some(message.into()),
        }
    }
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        match self.message {
            Some(message) => (self.status, message).into_response(),
            None => self.status.into_response(),
        }
    }
}

pub type ProxyResult<T> = Result<T, ProxyError>;

/// How to forward a `CompletionRequest`
pub struct ForwardConfig {
    pub api_key: String,
    pub target_url: Url,
    pub model: Option<String>,
    pub extra_headers: HeaderMap,
}
