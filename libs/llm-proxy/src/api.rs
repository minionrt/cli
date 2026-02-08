use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Json;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use url::Url;
use uuid::Uuid;

use crate::config::{ForwardConfig, ProxyConfig, ProxyError, ProxyResult};
use crate::requests::CompletionRequest;

const ROUNDTRIP_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Create a `reqwest` client with lenient timeouts.
fn create_reqwest_client() -> Client {
    reqwest::Client::builder()
        .timeout(ROUNDTRIP_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .expect("Failed to build streaming HTTP client")
}

pub fn scope<C>(config: C) -> Router
where
    C: ProxyConfig + Clone + Send + Sync + 'static,
{
    let config = Arc::new(config);
    Router::new()
        .nest(
            "/chat",
            Router::new().route(
                "/completions",
                post({
                    let config = config.clone();
                    move |headers: HeaderMap, Json(body): Json<CompletionRequest>| {
                        let config = config.clone();
                        async move { completions(config, headers, body).await }
                    }
                }),
            ),
        )
        .route(
            "/responses",
            post({
                let config = config.clone();
                move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
                    let config = config.clone();
                    async move { responses(config, headers, body).await }
                }
            }),
        )
}

async fn completions<C: ProxyConfig + Clone + Send + Sync + 'static>(
    config: Arc<C>,
    headers: HeaderMap,
    body: CompletionRequest,
) -> ProxyResult<Response> {
    let ctx = config.extract_context(&headers).await?;
    let mut request_payload = body;

    let ForwardConfig {
        api_key,
        target_url,
        model,
    } = config.forward(&ctx, &request_payload).await?;
    request_payload.model = model.or(request_payload.model);

    if request_payload.stream.unwrap_or(false) {
        config
            .inspect_interaction(&ctx, &request_payload, None)
            .await;
        Ok(forward_stream_request(&api_key, target_url, &request_payload).await)
    } else {
        let (mut resp, mut response_json) =
            forward_non_stream_request(&api_key, target_url, &request_payload).await?;
        if let Some(response_json) = &mut response_json {
            patch_response_id(response_json);
            let body = response_json.to_string();
            *resp.body_mut() = Body::from(body);
        }
        config
            .inspect_interaction(&ctx, &request_payload, response_json)
            .await;

        Ok(resp)
    }
}

async fn responses<C: ProxyConfig + Clone + Send + Sync + 'static>(
    config: Arc<C>,
    headers: HeaderMap,
    body: serde_json::Value,
) -> ProxyResult<Response> {
    let ctx = config.extract_context(&headers).await?;
    let mut request_payload = body;

    let ForwardConfig {
        api_key,
        target_url,
        model,
    } = config.forward_responses(&ctx, &request_payload).await?;

    if let Some(model) = model {
        set_model(&mut request_payload, model)?;
    }

    if request_payload
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        config
            .inspect_responses_interaction(&ctx, &request_payload, None)
            .await;
        Ok(forward_stream_request(&api_key, target_url, &request_payload).await)
    } else {
        let (mut resp, mut response_json) =
            forward_non_stream_request(&api_key, target_url, &request_payload).await?;
        if let Some(response_json) = &mut response_json {
            patch_response_id(response_json);
            let body = response_json.to_string();
            *resp.body_mut() = Body::from(body);
        }
        config
            .inspect_responses_interaction(&ctx, &request_payload, response_json)
            .await;

        Ok(resp)
    }
}

/// Forward a non-streaming request.
async fn forward_non_stream_request(
    api_key: &str,
    target_url: Url,
    request_payload: &(impl Serialize + ?Sized),
) -> ProxyResult<(Response, Option<serde_json::Value>)> {
    let client = create_reqwest_client();
    let req_builder = client
        .post(target_url)
        .bearer_auth(api_key)
        .header("Content-Type", "application/json")
        .json(&request_payload);

    let resp = req_builder.send().await.map_err(|err| {
        log::error!("Failed to send request: {:?}", err);
        ProxyError::internal("Failed to send request")
    })?;

    let status = resp.status();
    let text_body = resp.text().await.map_err(|err| {
        log::error!("Failed to read response body: {:?}", err);
        ProxyError::internal("Failed to read response body")
    })?;

    if status.is_success() {
        let response_json = serde_json::from_str(&text_body).ok();
        let response = Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(text_body))
            .unwrap();
        Ok((response, response_json))
    } else if status.is_client_error() {
        Err(ProxyError::bad_request(text_body))
    } else {
        log::error!("Upstream error: status={} body={}", status, text_body);
        Err(ProxyError::internal("Upstream error"))
    }
}

/// Forward a streaming (SSE) request.
async fn forward_stream_request(
    api_key: &str,
    target_url: Url,
    request_payload: &(impl Serialize + ?Sized),
) -> Response {
    let client = create_reqwest_client();
    let req_builder = client
        .post(target_url)
        .bearer_auth(api_key)
        .header("Content-Type", "application/json")
        .json(&request_payload);

    let resp = match req_builder.send().await {
        Ok(r) => r,
        Err(err) => {
            log::error!("Failed to send SSE request: {:?}", err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let status = resp.status();
    if !status.is_success() {
        let text_body = match resp.text().await {
            Ok(b) => b,
            Err(e) => {
                log::error!("Failed to read SSE error body: {:?}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
        return if status.is_client_error() {
            (StatusCode::BAD_REQUEST, text_body).into_response()
        } else {
            log::error!("Upstream SSE error: status={} body={}", status, text_body);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        };
    }

    let byte_stream = resp.bytes_stream().map(|chunk| match chunk {
        Ok(c) => Ok(c),
        Err(err) => {
            log::error!("Error reading SSE chunk: {:?}", err);
            Err(std::io::Error::other(err))
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/event-stream")
        .header(CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(byte_stream))
        .unwrap()
}

/// Some providers that offer a mostly but not fully OpenAI-compatible APIs
/// One example is Google Gemini's API, which does not include the `id` field in the response.
/// Since clients may expect this field to be present when deserializing the response, we patch the response
/// by adding the `id` field with a dummy value.
fn patch_response_id(response_json: &mut serde_json::Value) {
    if let Some(obj) = response_json.as_object_mut() {
        if obj.get("id").is_none() {
            obj.insert(
                "id".to_string(),
                serde_json::Value::String(Uuid::new_v4().to_string()),
            );
        }
    }
}

fn set_model(request_payload: &mut serde_json::Value, model: String) -> ProxyResult<()> {
    let Some(request_obj) = request_payload.as_object_mut() else {
        return Err(ProxyError::bad_request(
            "Request body must be a JSON object",
        ));
    };
    request_obj.insert("model".to_string(), serde_json::Value::String(model));
    Ok(())
}
