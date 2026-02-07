use axum::body::{Body, Bytes};
use axum::extract::{Extension, RawQuery};
use axum::http::header::{HeaderMap, CONTENT_TYPE};
use axum::http::StatusCode;
use axum::response::Response;
use reqwest::header::CONTENT_TYPE as REQ_CONTENT_TYPE;
use reqwest::Client;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::compression;
use crate::packet_line::advertisement::create_git_advertisement;
use crate::packet_line::errors::create_git_error_message;
use crate::packet_line::parse_commands::{parse_update_requests, RefModification};
use crate::{Forward, ForwardToLocal, ForwardToRemote, ProxyBehaivor, ProxyError, ProxyResult};

/// GET /info/refs?service=<service>
///
/// This endpoint is used by Git clients to discover available refs. In protocol v2,
/// the handshake is initiated here. We forward both push (git-receive-pack)
/// and fetch (git-upload-pack) info requests.
pub async fn info_refs_handler(
    Extension(behaivor): Extension<ProxyBehaivor>,
    RawQuery(query): RawQuery,
) -> Response {
    let Some(query) = query else {
        return response_plain(StatusCode::BAD_REQUEST, "Unsupported or missing service");
    };

    if !query.contains("service=git-receive-pack") && !query.contains("service=git-upload-pack") {
        return response_plain(StatusCode::BAD_REQUEST, "Unsupported or missing service");
    }

    match behaivor.forward {
        Forward::ForwardToRemote(ref forward) => remote_info_refs(forward, &query).await,
        Forward::ForwardToLocal(ref local) => local_info_refs(local, &query).await,
    }
}

async fn remote_info_refs(forward: &ForwardToRemote, query: &str) -> Response {
    let mut forward_url = forward.url.clone();
    {
        let mut segments = forward_url
            .path_segments_mut()
            .expect("Cannot modify URL segments");
        segments.push("info");
        segments.push("refs");
    }
    forward_url.set_query(Some(query));

    let client = Client::new();
    match client
        .get(forward_url)
        .basic_auth(
            forward.basic_auth_user.clone(),
            Some(forward.basic_auth_pass.clone()),
        )
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let content_type = resp
                .headers()
                .get(REQ_CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let body = resp.bytes().await.unwrap_or_else(|_| Bytes::new());
            let status =
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            response_with_bytes(status, &content_type, body)
        }
        Err(err) => {
            log::error!("Error forwarding info/refs: {:?}", err);
            response_plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Error forwarding request",
            )
        }
    }
}

async fn local_info_refs(forward: &ForwardToLocal, query: &str) -> Response {
    let (service, content_type) = if query.contains("service=git-receive-pack") {
        (
            "git-receive-pack",
            "application/x-git-receive-pack-advertisement",
        )
    } else if query.contains("service=git-upload-pack") {
        (
            "git-upload-pack",
            "application/x-git-upload-pack-advertisement",
        )
    } else {
        return response_plain(StatusCode::BAD_REQUEST, "Unsupported or missing service");
    };

    let output = match Command::new(service)
        .arg("--advertise-refs")
        .arg(&forward.path)
        .output()
        .await
    {
        Ok(o) => o,
        Err(err) => {
            log::error!("Error spawning {}: {:?}", service, err);
            return response_plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Error spawning {service}"),
            );
        }
    };

    if !output.status.success() {
        log::error!("Command {} exited with non-zero status", service);
        return response_plain(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Error processing info/refs",
        );
    }

    // Prepend the Git advertisement header.
    let advertisement = create_git_advertisement(service, &output.stdout);

    response_with_bytes(StatusCode::OK, content_type, advertisement)
}

/// POST /git-receive-pack
///
/// This endpoint is used by Git clients to push updates.
/// We first inspect the push commands to ensure that they only affect the allowed ref,
/// and if so we forward the entire request.
pub async fn git_receive_pack_handler(
    Extension(behaivor): Extension<ProxyBehaivor>,
    headers: HeaderMap,
    body: Bytes,
) -> ProxyResult<Response> {
    let body_bytes = decompress_if_gzip(body, &headers).await?;

    match parse_update_requests(&body_bytes) {
        Ok(refs) => {
            for r in refs {
                if r.ref_name() != behaivor.allowed_ref {
                    log::warn!("Push attempted to disallowed ref: {}", r.ref_name());
                    let error_body =
                        create_git_error_message("Push not allowed to modify this ref");
                    return Ok(response_with_bytes(
                        StatusCode::OK,
                        "application/x-git-receive-pack-result",
                        error_body,
                    ));
                }
                match r {
                    RefModification::Create { .. } => {
                        log::warn!("Push attempted to create ref: {}", r.ref_name());
                        let error_body =
                            create_git_error_message("Push not allowed to create this ref");
                        return Ok(response_with_bytes(
                            StatusCode::OK,
                            "application/x-git-receive-pack-result",
                            error_body,
                        ));
                    }
                    RefModification::Delete { .. } => {
                        log::warn!("Push attempted to delete ref: {}", r.ref_name());
                        let error_body =
                            create_git_error_message("Push not allowed to delete this ref");
                        return Ok(response_with_bytes(
                            StatusCode::OK,
                            "application/x-git-receive-pack-result",
                            error_body,
                        ));
                    }
                    RefModification::Update { .. } => {}
                }
            }
        }
        Err(e) => {
            log::error!("Error parsing push commands: {:?}", e);
            let error_body = create_git_error_message("Invalid push data");
            return Ok(response_with_bytes(
                StatusCode::OK,
                "application/x-git-receive-pack-result",
                error_body,
            ));
        }
    }

    let response = match behaivor.forward {
        Forward::ForwardToRemote(ref forward) => remote_git_receive_pack(forward, body_bytes).await,
        Forward::ForwardToLocal(ref local) => local_git_receive_pack(local, body_bytes).await,
    };

    Ok(response)
}

async fn remote_git_receive_pack(forward: &ForwardToRemote, body_bytes: Bytes) -> Response {
    let mut forward_url = forward.url.clone();
    {
        let mut segments = forward_url
            .path_segments_mut()
            .expect("Cannot modify URL segments");
        segments.push("git-receive-pack");
    }
    let client = Client::new();
    match client
        .post(forward_url)
        .basic_auth(
            forward.basic_auth_user.clone(),
            Some(forward.basic_auth_pass.clone()),
        )
        .header("Content-Type", "application/x-git-receive-pack-request")
        .body(body_bytes.clone())
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let content_type = resp
                .headers()
                .get(REQ_CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let resp_body = resp.bytes().await.unwrap_or_else(|_| Bytes::new());
            let status =
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            response_with_bytes(status, &content_type, resp_body)
        }
        Err(err) => {
            log::error!("Error forwarding git-receive-pack: {:?}", err);
            let error_body = create_git_error_message("Error forwarding push");
            response_with_bytes(
                StatusCode::OK,
                "application/x-git-receive-pack-result",
                error_body,
            )
        }
    }
}

async fn local_git_receive_pack(forward: &ForwardToLocal, body_bytes: Bytes) -> Response {
    let mut child = match Command::new("git-receive-pack")
        .arg("--stateless-rpc")
        .arg(&forward.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            log::error!("Error spawning git-receive-pack: {:?}", err);
            let error_body = create_git_error_message("Error spawning git-receive-pack");
            return response_with_bytes(
                StatusCode::OK,
                "application/x-git-receive-pack-result",
                error_body,
            );
        }
    };

    // Write the request body to the child’s stdin.
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(err) = stdin.write_all(&body_bytes).await {
            log::error!("Error writing to git-receive-pack stdin: {:?}", err);
        }
    }

    let output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(err) => {
            log::error!("Error waiting for git-receive-pack: {:?}", err);
            let error_body = create_git_error_message("Error processing push");
            return response_with_bytes(
                StatusCode::OK,
                "application/x-git-receive-pack-result",
                error_body,
            );
        }
    };

    if !output.status.success() {
        log::error!("git-receive-pack exited with non-zero status");
        let error_body = create_git_error_message("git-receive-pack failed");
        return response_with_bytes(
            StatusCode::OK,
            "application/x-git-receive-pack-result",
            error_body,
        );
    }

    response_with_bytes(
        StatusCode::OK,
        "application/x-git-receive-pack-result",
        Bytes::from(output.stdout),
    )
}

/// POST /git-upload-pack
///
/// This endpoint is used by Git clients to fetch objects (clone or fetch).
/// Unlike push, no ref restrictions are needed, so we simply forward the request.
pub async fn git_upload_pack_handler(
    Extension(behaivor): Extension<ProxyBehaivor>,
    headers: HeaderMap,
    body: Bytes,
) -> ProxyResult<Response> {
    let body_bytes = decompress_if_gzip(body, &headers).await?;

    let response = match behaivor.forward {
        Forward::ForwardToRemote(ref forward) => remote_git_upload_pack(forward, body_bytes).await,
        Forward::ForwardToLocal(ref local) => local_git_upload_pack(local, body_bytes).await,
    };

    Ok(response)
}

async fn remote_git_upload_pack(forward: &ForwardToRemote, body_bytes: Bytes) -> Response {
    let mut forward_url = forward.url.clone();
    {
        let mut segments = forward_url
            .path_segments_mut()
            .expect("Cannot modify URL segments");
        segments.push("git-upload-pack");
    }
    let client = Client::new();
    match client
        .post(forward_url)
        .basic_auth(
            forward.basic_auth_user.clone(),
            Some(forward.basic_auth_pass.clone()),
        )
        .header("Content-Type", "application/x-git-upload-pack-request")
        .body(body_bytes.clone())
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let content_type = resp
                .headers()
                .get(REQ_CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let resp_body = resp.bytes().await.unwrap_or_else(|_| Bytes::new());
            let status =
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            response_with_bytes(status, &content_type, resp_body)
        }
        Err(err) => {
            log::error!("Error forwarding git-upload-pack: {:?}", err);
            response_plain(StatusCode::INTERNAL_SERVER_ERROR, "Error forwarding fetch")
        }
    }
}

async fn local_git_upload_pack(forward: &ForwardToLocal, body_bytes: Bytes) -> Response {
    let mut child = match Command::new("git-upload-pack")
        .arg("--stateless-rpc")
        .arg(&forward.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            log::error!("Error spawning git-upload-pack: {:?}", err);
            return response_plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Error spawning git-upload-pack",
            );
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(err) = stdin.write_all(&body_bytes).await {
            log::error!("Error writing to git-upload-pack stdin: {:?}", err);
        }
    }

    let output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(err) => {
            log::error!("Error waiting for git-upload-pack: {:?}", err);
            return response_plain(StatusCode::INTERNAL_SERVER_ERROR, "Error processing fetch");
        }
    };

    if !output.status.success() {
        log::error!("git-upload-pack exited with non-zero status");
        return response_plain(StatusCode::INTERNAL_SERVER_ERROR, "git-upload-pack failed");
    }

    response_with_bytes(
        StatusCode::OK,
        "application/x-git-upload-pack-result",
        Bytes::from(output.stdout),
    )
}

/// Decompress the request body if it is encoded with gzip.
async fn decompress_if_gzip(body_bytes: Bytes, headers: &HeaderMap) -> ProxyResult<Bytes> {
    if let Some(encoding) = headers.get("Content-Encoding") {
        let encoding_str = encoding.to_str().unwrap_or("").trim();
        if encoding_str.eq_ignore_ascii_case("gzip") {
            let decompressed = compression::decompress_gzip(&body_bytes)
                .await
                .map_err(|e| {
                    log::error!("Error decompressing body: {:?}", e);
                    ProxyError::bad_request("Decompression failed")
                })?;
            return Ok(Bytes::from(decompressed));
        } else {
            log::error!("Unsupported Content-Encoding: {:?}", encoding);
        }
    }
    Ok(body_bytes)
}

fn response_plain(status: StatusCode, body: &str) -> Response {
    Response::builder()
        .status(status)
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn response_with_bytes(status: StatusCode, content_type: &str, body: Bytes) -> Response {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, content_type)
        .body(Body::from(body))
        .unwrap()
}
