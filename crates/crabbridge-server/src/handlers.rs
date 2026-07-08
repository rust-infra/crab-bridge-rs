use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    Json,
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;
use tracing::{debug, error, info, warn};

use crate::cache::ResponseCache;
use crate::metrics::BridgeMetrics;
use crate::state::{AppState, ProviderRuntime};
use crate::stream::{self, StreamArgs};
use crate::translate;
use crate::translate::TranslationOptions;
use crabbridge_core::provider::{ProviderKind, apply_upstream_headers, join_upstream_base};
use crabbridge_core::types::*;

const DEBUG_NAME_LIMIT: usize = 80;

fn record_http_outcome(
    metrics: &BridgeMetrics,
    provider: &str,
    route: &str,
    status: StatusCode,
    started: Instant,
    stream: bool,
) {
    metrics.record_request(
        provider,
        route,
        status.as_u16(),
        started.elapsed().as_millis() as u64,
        stream,
    );
}

/// Read `Authorization: Bearer …` from an incoming Codex request.
fn bearer_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = auth
        .strip_prefix("Bearer ")
        .or_else(|| auth.strip_prefix("bearer "))?
        .trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// Bearer token from Codex (`env_key` → `Authorization: Bearer …`).
fn upstream_api_key(headers: &HeaderMap) -> Option<Arc<String>> {
    bearer_token_from_headers(headers).map(Arc::new)
}

fn warn_missing_api_key(provider: &str, route: &str) {
    let kind = ProviderKind::from_route(provider).unwrap_or(ProviderKind::Custom);
    warn!(
        provider,
        route,
        env_key = kind.codex_env_key(),
        "missing upstream API key — pass Authorization: Bearer or set env_key in shell"
    );
}

fn upstream_api_key_or_warn(
    headers: &HeaderMap,
    provider: &str,
    route: &str,
) -> Option<Arc<String>> {
    let key = upstream_api_key(headers);
    if key.is_none() {
        warn_missing_api_key(provider, route);
    }
    key
}

pub async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

/// Codex probes `HEAD/GET {base_url}` for reachability (e.g. `http://127.0.0.1:11435/kimi/v1`).
pub async fn api_root(
    State(state): State<AppState>,
    provider: Option<Path<String>>,
) -> impl IntoResponse {
    let slug = provider
        .as_ref()
        .map(|p| p.0.as_str())
        .unwrap_or(state.default_provider.as_str());
    api_root_for_provider(&state, slug).await
}

async fn api_root_for_provider(state: &AppState, provider: &str) -> Response {
    if state.provider(provider).is_some() {
        StatusCode::OK.into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            format!("unknown provider: {provider}"),
        )
            .into_response()
    }
}

pub async fn handle_models(
    State(state): State<AppState>,
    provider: Option<Path<String>>,
    headers: HeaderMap,
) -> Response {
    let slug = provider
        .map(|p| p.0)
        .unwrap_or_else(|| state.default_provider.as_str().to_string());
    handle_models_inner(state, &slug, &headers).await
}

async fn handle_models_inner(state: AppState, provider: &str, headers: &HeaderMap) -> Response {
    let started = Instant::now();
    let Some(runtime) = state.provider(provider) else {
        let response = (
            StatusCode::NOT_FOUND,
            format!("unknown provider: {provider}"),
        )
            .into_response();
        record_http_outcome(
            &state.metrics,
            provider,
            "models",
            StatusCode::NOT_FOUND,
            started,
            false,
        );
        return response;
    };
    info!(provider, "GET /{provider}/v1/models");
    let kind = ProviderKind::from_route(provider).unwrap_or(ProviderKind::Custom);
    let api_key = upstream_api_key_or_warn(headers, provider, "models").unwrap_or_default();
    let url = format!("{}models", join_upstream_base(&runtime.upstream));
    let builder = apply_upstream_headers(state.client.get(&url), kind, api_key.as_str());

    let upstream_body: Option<serde_json::Value> = match builder.send().await {
        Ok(r) if r.status().is_success() => match r.json::<serde_json::Value>().await {
            Ok(b) => Some(b),
            Err(e) => {
                warn!("upstream models: parse error: {e}");
                None
            }
        },
        Ok(r) => {
            warn!("upstream models: status {}", r.status());
            None
        }
        Err(e) => {
            warn!("upstream models: request error: {e}");
            None
        }
    };

    let mut list = upstream_body
        .as_ref()
        .and_then(|b| b.get("data").or_else(|| b.get("models")))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut seen: HashSet<String> = list
        .iter()
        .filter_map(|item| item.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();

    list.retain(|item| {
        item.get("id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| kind.model_matches_provider(id))
    });
    seen.retain(|id| kind.model_matches_provider(id));

    for known in kind.known_models_for_upstream(runtime.upstream.as_str()) {
        if seen.insert((*known).to_string()) {
            list.push(json!({
                "id": known,
                "object": "model",
                "owned_by": "crabbridge",
            }));
        }
    }
    let default_model = kind.default_model();
    if seen.insert(default_model.to_string()) {
        list.push(json!({
            "id": default_model,
            "object": "model",
            "owned_by": "crabbridge",
        }));
    }

    let response = Json(json!({
        "object": "list",
        "data": list.clone(),
        "models": list,
    }))
    .into_response();
    record_http_outcome(
        &state.metrics,
        provider,
        "models",
        StatusCode::OK,
        started,
        false,
    );
    let models = serde_json::to_string(&list).unwrap();
    info!(
        provider,
        has_api_key = !api_key.is_empty(),
        "models={models}"
    );
    response
}

pub async fn handle_fallback(req: Request) -> Response {
    warn!("unhandled {} {}", req.method(), req.uri().path());
    (StatusCode::NOT_FOUND, "not found").into_response()
}

pub async fn handle_responses(
    State(state): State<AppState>,
    provider: Option<Path<String>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let slug = provider
        .map(|p| p.0)
        .unwrap_or_else(|| state.default_provider.as_str().to_string());
    handle_responses_for_provider(state, &slug, &headers, body).await
}

async fn handle_responses_for_provider(
    state: AppState,
    provider: &str,
    headers: &HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let Some(runtime) = state.provider(provider) else {
        let response = (
            StatusCode::NOT_FOUND,
            format!("unknown provider: {provider}"),
        )
            .into_response();
        record_http_outcome(
            &state.metrics,
            provider,
            "responses",
            StatusCode::NOT_FOUND,
            Instant::now(),
            false,
        );
        return response;
    };

    let started = Instant::now();
    info!(
        provider,
        bytes = body.len(),
        "POST /{provider}/v1/responses"
    );
    let req: ResponsesRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            error!(
                elapsed_ms = started.elapsed().as_millis() as u64,
                "JSON parse error: {e}"
            );
            error!(
                "body prefix: {}",
                String::from_utf8_lossy(&body[..body.len().min(200)])
            );
            return {
                let response = (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()).into_response();
                record_http_outcome(
                    &state.metrics,
                    provider,
                    "responses",
                    StatusCode::UNPROCESSABLE_ENTITY,
                    started,
                    false,
                );
                response
            };
        }
    };
    let input_items = match &req.input {
        ResponsesInput::Messages(v) => v.len(),
        _ => 1,
    };
    info!(
        provider,
        model = %req.model,
        stream = req.stream,
        input_items,
        tools = req.tools.len(),
        request_bytes = body.len(),
        prev_resp = ?req.previous_response_id,
        "responses request"
    );
    debug!(
        "→ response tools={}",
        summarize_debug_names(response_tool_debug_names(&req.tools))
    );

    handle_responses_inner(state, provider, runtime, headers, req, started).await
}

async fn handle_responses_inner(
    state: AppState,
    provider: &str,
    runtime: ProviderRuntime,
    headers: &HeaderMap,
    req: ResponsesRequest,
    started: Instant,
) -> Response {
    let mut history = req
        .previous_response_id
        .as_deref()
        .map(|id| state.sessions.get_history(id))
        .unwrap_or_default();
    let history_messages = history.len();
    if should_isolate_spawn_child_request(&req, &history) {
        debug!("isolating spawned child request from parent response history");
        history.clear();
    }

    let model = req.model.clone();
    let namespace_tools = translate::namespace_tool_map(&req.tools);
    let kind = ProviderKind::from_route(provider).unwrap_or(ProviderKind::Custom);
    let mut chat_req = translate::to_chat_request(
        &req,
        history,
        &state.sessions,
        TranslationOptions {
            provider: kind,
            default_model: kind.default_model(),
            model_map: runtime.model_map.as_deref(),
            default_max_tokens: runtime.default_max_tokens,
            default_temperature: runtime.default_temperature,
        },
    );
    info!(
        provider,
        client_model = %model,
        upstream_model = %chat_req.model,
        history_messages,
        messages = chat_req.messages.len(),
        tools = chat_req.tools.len(),
        stream = req.stream,
        "forwarding to upstream chat/completions"
    );
    debug!(
        "→ upstream tools={}",
        summarize_debug_names(chat_tool_debug_names(&chat_req.tools))
    );
    let url = format!("{}chat/completions", join_upstream_base(&runtime.upstream));
    let Some(api_key) = upstream_api_key_or_warn(headers, provider, "responses") else {
        let response = (
            StatusCode::UNAUTHORIZED,
            "missing Authorization: Bearer token (set Codex env_key in shell)",
        )
            .into_response();
        record_http_outcome(
            &state.metrics,
            provider,
            "responses",
            StatusCode::UNAUTHORIZED,
            started,
            false,
        );
        return response;
    };

    if req.stream {
        let response_id = state.sessions.new_id();
        chat_req.stream = true;
        let request_messages = chat_req.messages.clone();
        let args = StreamArgs {
            client: state.client,
            url,
            api_key,
            chat_req,
            upstream_request: state.upstream_request,
            response_id,
            provider: provider.to_string(),
            sessions: state.sessions,
            request_messages,
            namespace_tools,
            model,
            started,
            metrics: state.metrics.clone(),
        };
        match stream::prepare_upstream(&args).await {
            Ok(upstream) => {
                record_http_outcome(
                    &state.metrics,
                    provider,
                    "responses",
                    StatusCode::OK,
                    started,
                    true,
                );
                stream::translate_stream(args, upstream).into_response()
            }
            Err(stream::UpstreamError::BodyError(msg)) => {
                let response = (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response();
                record_http_outcome(
                    &state.metrics,
                    provider,
                    "responses",
                    StatusCode::INTERNAL_SERVER_ERROR,
                    started,
                    true,
                );
                response
            }
            Err(stream::UpstreamError::ConnectionError(msg)) => {
                let response = (StatusCode::BAD_GATEWAY, msg).into_response();
                record_http_outcome(
                    &state.metrics,
                    provider,
                    "responses",
                    StatusCode::BAD_GATEWAY,
                    started,
                    true,
                );
                response
            }
            Err(stream::UpstreamError::UpstreamError { status, body }) => {
                let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
                let response = (status, body).into_response();
                record_http_outcome(&state.metrics, provider, "responses", status, started, true);
                response
            }
        }
    } else {
        chat_req.stream = false;
        let metrics = state.metrics.clone();
        let response = handle_blocking(BlockingArgs {
            state,
            provider: provider.to_string(),
            api_key,
            chat_req,
            url,
            model,
            namespace_tools,
            started,
        })
        .await;
        let status = response.status();
        record_http_outcome(&metrics, provider, "responses", status, started, false);
        response
    }
}

fn summarize_debug_names(names: Vec<String>) -> String {
    if names.is_empty() {
        return "(none)".to_string();
    }

    let total = names.len();
    let mut shown = names
        .into_iter()
        .take(DEBUG_NAME_LIMIT)
        .collect::<Vec<_>>()
        .join(", ");
    if total > DEBUG_NAME_LIMIT {
        shown.push_str(&format!(", ... (+{} more)", total - DEBUG_NAME_LIMIT));
    }
    shown
}

fn response_tool_debug_names(tools: &[serde_json::Value]) -> Vec<String> {
    let mut names = Vec::new();
    for tool in tools {
        match tool.get("type").and_then(serde_json::Value::as_str) {
            Some("function") => {
                if let Some(name) = tool
                    .get("name")
                    .or_else(|| tool.get("function").and_then(|f| f.get("name")))
                    .and_then(serde_json::Value::as_str)
                {
                    names.push(name.to_string());
                }
            }
            Some("namespace") => {
                let namespace = tool
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                if let Some(subs) = tool.get("tools").and_then(serde_json::Value::as_array) {
                    for sub in subs {
                        if sub.get("type").and_then(serde_json::Value::as_str) == Some("function")
                            && let Some(name) = sub.get("name").and_then(serde_json::Value::as_str)
                        {
                            names.push(translate::chat_function_name_for_namespace_tool(
                                namespace, name,
                            ));
                        }
                    }
                }
            }
            Some(kind) => names.push(format!("<{kind}>")),
            None => {}
        }
    }
    names
}

fn chat_tool_debug_names(tools: &[serde_json::Value]) -> Vec<String> {
    tools
        .iter()
        .filter_map(|tool| {
            tool.get("function")
                .and_then(|f| f.get("name"))
                .and_then(serde_json::Value::as_str)
                .or_else(|| tool.get("name").and_then(serde_json::Value::as_str))
                .map(String::from)
        })
        .collect()
}

fn chat_response_tool_call_debug_names(chat_resp: &ChatResponse) -> Vec<String> {
    chat_resp
        .choices
        .iter()
        .flat_map(|choice| choice.message.tool_calls.iter())
        .flatten()
        .filter_map(|tool_call| {
            tool_call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(serde_json::Value::as_str)
                .map(String::from)
        })
        .collect()
}

fn should_isolate_spawn_child_request(req: &ResponsesRequest, history: &[ChatMessage]) -> bool {
    let Some(input_text) = isolated_user_text(&req.input) else {
        return false;
    };
    let completed_tool_calls: HashSet<&str> = history
        .iter()
        .filter_map(|msg| msg.tool_call_id.as_deref())
        .collect();
    let pending_spawns = history
        .iter()
        .flat_map(|msg| msg.tool_calls.as_deref().unwrap_or(&[]))
        .filter(|call| {
            let call_id = call.get("id").and_then(serde_json::Value::as_str);
            call_id.is_none_or(|id| !completed_tool_calls.contains(id))
        })
        .filter_map(parse_spawn_agent_call)
        .collect::<Vec<_>>();

    if pending_spawns
        .iter()
        .any(|spawn| spawn.message.as_deref() == Some(input_text))
    {
        return true;
    }

    pending_spawns.len() == 1 && pending_spawns[0].is_v2_encrypted_candidate()
}

fn isolated_user_text(input: &ResponsesInput) -> Option<&str> {
    match input {
        ResponsesInput::Text(text) => Some(text.as_str()),
        ResponsesInput::Messages(items) => {
            if items.len() != 1 {
                return None;
            }
            let item = &items[0];
            if item.get("type").and_then(serde_json::Value::as_str) != Some("message")
                || item.get("role").and_then(serde_json::Value::as_str) != Some("user")
            {
                return None;
            }
            match item.get("content") {
                Some(serde_json::Value::String(text)) => Some(text.as_str()),
                Some(serde_json::Value::Array(parts)) if parts.len() == 1 => {
                    parts[0].get("text").and_then(serde_json::Value::as_str)
                }
                _ => None,
            }
        }
    }
}

struct SpawnAgentCall {
    message: Option<String>,
    fork_turns: Option<String>,
}

impl SpawnAgentCall {
    fn is_v2_encrypted_candidate(&self) -> bool {
        self.fork_turns.is_some()
            && self
                .message
                .as_deref()
                .is_some_and(|message| !message.is_empty())
    }
}

fn parse_spawn_agent_call(call: &serde_json::Value) -> Option<SpawnAgentCall> {
    if call
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(serde_json::Value::as_str)
        != Some("spawn_agent")
    {
        return None;
    }
    let arguments = call
        .get("function")
        .and_then(|function| function.get("arguments"))
        .and_then(serde_json::Value::as_str)?;
    let arguments: serde_json::Value = serde_json::from_str(arguments).ok()?;
    Some(SpawnAgentCall {
        message: arguments
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map(String::from),
        fork_turns: arguments
            .get("fork_turns")
            .and_then(serde_json::Value::as_str)
            .map(String::from),
    })
}

struct BlockingArgs {
    state: AppState,
    provider: String,
    api_key: Arc<String>,
    chat_req: ChatRequest,
    url: String,
    model: String,
    namespace_tools: translate::NamespaceToolMap,
    started: Instant,
}

async fn handle_blocking(args: BlockingArgs) -> Response {
    let BlockingArgs {
        state,
        provider,
        api_key,
        chat_req,
        url,
        model,
        namespace_tools,
        started,
    } = args;
    let messages = chat_req.messages.len();
    let tools = chat_req.tools.len();

    if let Some(cache) = &state.cache
        && let Ok(body) = state.upstream_request.request_body(&chat_req)
    {
        let key = ResponseCache::cache_key(&provider, &url, &api_key, &body);
        if let Some(cached) = cache.get(&key).await {
            state.metrics.record_cache_hit();
            info!(
                provider,
                cache_key = %key,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "returning cached responses payload"
            );
            return (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                cached,
            )
                .into_response();
        }
        state.metrics.record_cache_miss();
    }

    let kind = ProviderKind::from_route(&provider).unwrap_or(ProviderKind::Custom);
    let builder = apply_upstream_headers(
        state
            .client
            .post(&url)
            .header("Content-Type", "application/json"),
        kind,
        api_key.as_str(),
    );

    let upstream_body = match state.upstream_request.request_body(&chat_req) {
        Ok(body) => body,
        Err(e) => {
            error!(
                elapsed_ms = started.elapsed().as_millis() as u64,
                "upstream request body error: {e}"
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };
    let request_bytes = serde_json::to_vec(&upstream_body)
        .map(|b| b.len())
        .unwrap_or(0);

    info!(
        model = %chat_req.model,
        messages,
        tools,
        request_bytes,
        "upstream non-stream request"
    );
    match builder.json(&upstream_body).send().await {
        Err(e) => {
            error!(
                elapsed_ms = started.elapsed().as_millis() as u64,
                "upstream error: {e}"
            );
            (StatusCode::BAD_GATEWAY, e.to_string()).into_response()
        }
        Ok(r) if !r.status().is_success() => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            error!(
                status = %status,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "upstream error: {body}"
            );
            (
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
                body,
            )
                .into_response()
        }
        Ok(r) => match r.json::<ChatResponse>().await {
            Err(e) => {
                error!(
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "parse error: {e}"
                );
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
            }
            Ok(chat_resp) => {
                debug!(
                    "← upstream function_calls={}",
                    summarize_debug_names(chat_response_tool_call_debug_names(&chat_resp))
                );
                let usage = chat_resp.usage.clone().unwrap_or_default();
                let assistant_msg = chat_resp
                    .choices
                    .first()
                    .map(|c| c.message.clone())
                    .unwrap_or_else(|| ChatMessage {
                        role: "assistant".into(),
                        content: Some(serde_json::Value::String(String::new())),
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                let text_bytes = assistant_msg
                    .content
                    .as_ref()
                    .and_then(|c| c.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                let reasoning_bytes = assistant_msg
                    .reasoning_content
                    .as_ref()
                    .map(|s| s.len())
                    .unwrap_or(0);
                let tool_calls = assistant_msg
                    .tool_calls
                    .as_ref()
                    .map(|t| t.len())
                    .unwrap_or(0);

                let mut full_history = chat_req.messages.clone();
                full_history.push(assistant_msg);
                let response_id = state.sessions.save(&provider, full_history);

                let (resp, _) = if namespace_tools.is_empty() {
                    translate::from_chat_response(response_id.clone(), &model, chat_resp)
                } else {
                    translate::from_chat_response_with_tool_map(
                        response_id.clone(),
                        &model,
                        chat_resp,
                        &namespace_tools,
                    )
                };

                if let Some(cache) = &state.cache
                    && let Ok(body) = state.upstream_request.request_body(&chat_req)
                {
                    let key = ResponseCache::cache_key(&provider, &url, &api_key, &body);
                    if let Ok(bytes) = serde_json::to_vec(&resp) {
                        cache.insert(key, bytes::Bytes::from(bytes)).await;
                    }
                }

                info!(
                    provider,
                    response_id = %response_id,
                    model = %model,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    prompt_tokens = usage.prompt_tokens,
                    completion_tokens = usage.completion_tokens,
                    total_tokens = usage.total_tokens,
                    cache_hit_tokens = usage.cache_hit(),
                    cache_miss_tokens = usage.cache_miss(),
                    cache_hit_rate = format!("{:.1}%", usage.cache_hit_rate()),
                    text_bytes,
                    reasoning_bytes,
                    tool_calls,
                    "responses non-stream completed"
                );
                Json(resp).into_response()
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn bearer_token_from_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-kimi-from-codex"),
        );
        assert_eq!(
            bearer_token_from_headers(&headers).as_deref(),
            Some("sk-kimi-from-codex")
        );
    }

    #[test]
    fn upstream_api_key_reads_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sk-kimi-from-codex"),
        );
        assert_eq!(
            upstream_api_key(&headers).as_deref().map(|k| k.as_str()),
            Some("sk-kimi-from-codex")
        );
        assert!(upstream_api_key(&HeaderMap::new()).is_none());
    }
}
