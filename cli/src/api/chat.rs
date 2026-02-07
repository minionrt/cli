use std::sync::Arc;

use actix_web::{web, Error, HttpRequest, Scope};
use serde_json::Value;

use llm_proxy::{CompletionRequest, ForwardConfig, ProxyConfig};

use crate::context::Context;

pub fn scope() -> Scope {
    llm_proxy::scope(TheProxyConfig {})
}

#[derive(Clone)]
struct TheProxyConfig {}

impl ProxyConfig for TheProxyConfig {
    type Context = Arc<Context>;

    async fn extract_context(&self, req: &HttpRequest) -> Result<Self::Context, Error> {
        let ctx = req
            .app_data::<web::Data<Context>>()
            .expect("Context not found in app data");
        let ctx = ctx.clone().into_inner();

        Ok(ctx)
    }

    async fn forward(
        &self,
        ctx: &Self::Context,
        req: &CompletionRequest,
    ) -> Result<ForwardConfig, Error> {
        let Some(model) = req.model.as_ref() else {
            return Err(actix_web::error::ErrorBadRequest(
                "Missing model in request",
            ));
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
        response: Option<Value>,
    ) {
        // For now we just log raw request and response
        // Later we will need to come up with a proper feedback mechanism
        println!("Request: {request:?}\n\nResponse: {response:?}");
    }
}
