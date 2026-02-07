use std::sync::Arc;

use async_trait::async_trait;
use axum::http::HeaderMap;
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
        })
    }

    async fn inspect_interaction(
        &self,
        _ctx: &Self::Context,
        request: &CompletionRequest,
        response: Option<serde_json::Value>,
    ) {
        // For now we just log raw request and response
        // Later we will need to come up with a proper feedback mechanism
        println!("Request: {request:?}\n\nResponse: {response:?}");
    }
}
