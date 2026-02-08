use std::sync::Arc;

use async_trait::async_trait;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use llm_proxy::{ProxyConfig, ProxyError, ProxyResult};

use llm_proxy::{CompletionRequest, ForwardConfig};

use crate::context::Context;

pub fn router(ctx: Arc<Context>) -> axum::Router {
    llm_proxy::scope(TheProxyConfig { ctx })
}

#[derive(Clone)]
struct TheProxyConfig {
    ctx: Arc<Context>,
}

#[async_trait]
impl ProxyConfig for TheProxyConfig {
    type Context = Arc<Context>;

    async fn extract_context(&self, _headers: &HeaderMap) -> ProxyResult<Self::Context> {
        Ok(self.ctx.clone())
    }

    async fn forward(
        &self,
        ctx: &Self::Context,
        req: &CompletionRequest,
    ) -> ProxyResult<ForwardConfig> {
        let Some(model) = req.model.as_ref() else {
            return Err(ProxyError::bad_request("Missing model in request"));
        };
        let (model_name, details) = &ctx.llm_router_table.details_for_model(model);

        Ok(ForwardConfig {
            api_key: details.api_key.clone(),
            target_url: details.api_chat_completions_endpoint.clone(),
            model: Some(model_name.clone()),
            extra_headers: build_header_map(details)?,
        })
    }

    async fn forward_responses(
        &self,
        ctx: &Self::Context,
        req: &serde_json::Value,
    ) -> ProxyResult<ForwardConfig> {
        let Some(model) = req.get("model").and_then(|v| v.as_str()) else {
            return Err(ProxyError::bad_request("Missing model in request"));
        };
        let (model_name, details) = &ctx.llm_router_table.details_for_model(model);

        Ok(ForwardConfig {
            api_key: details.api_key.clone(),
            target_url: details.api_responses_endpoint.clone(),
            model: Some(model_name.clone()),
            extra_headers: build_header_map(details)?,
        })
    }

    async fn forward_models(&self, ctx: &Self::Context) -> ProxyResult<ForwardConfig> {
        let details = ctx
            .llm_router_table
            .providers
            .get(&ctx.llm_router_table.default_provider)
            .expect("Default provider not found");

        Ok(ForwardConfig {
            api_key: details.api_key.clone(),
            target_url: details.api_models_endpoint.clone(),
            model: None,
            extra_headers: build_header_map(details)?,
        })
    }

    async fn inspect_interaction(
        &self,
        _ctx: &Self::Context,
        request: &CompletionRequest,
        response: Option<serde_json::Value>,
    ) {
        log::trace!("Request: {request:?}\n\nResponse: {response:?}");
    }

    async fn inspect_responses_interaction(
        &self,
        _ctx: &Self::Context,
        request: &serde_json::Value,
        response: Option<serde_json::Value>,
    ) {
        log::trace!("Request: {request:?}\n\nResponse: {response:?}");
    }
}

fn build_header_map(
    details: &crate::config::LLMProviderDetails,
) -> ProxyResult<HeaderMap<HeaderValue>> {
    let mut headers = HeaderMap::new();
    for (key, value) in &details.upstream_headers {
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| ProxyError::internal("Invalid upstream header name"))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| ProxyError::internal("Invalid upstream header value"))?;
        headers.insert(name, value);
    }
    Ok(headers)
}
