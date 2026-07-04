use std::collections::HashSet;
use std::time::Instant;

use axum::{
    Json,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use reqwest::Url;
use serde_json::json;
use tracing::{debug, error, info, warn};

use crate::cache::ResponseCache;
use crate::provider::ProviderKind;
use crate::state::{AppState, ProviderRuntime};
use crate::stream::{self, StreamArgs};
use crate::translate;
use crate::types::*;

const DEBUG_NAME_LIMIT: usize = 80;

pub fn join_base(url: &Url) -> String {
    let s = url.as_str();
    if s.ends_with('/') {
        s.to_string()
    } else {
        format!("{s}/")
    }
}

pub async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

/// Codex probes `HEAD/GET {base_url}` for reachability (e.g. `http://127.0.0.1:11435/kimi/v1`).
pub async fn api_root_default(State(state): State<AppState>) -> impl IntoResponse {
    api_root_for_provider(&state, state.default_provider.as_str()).await
}

pub async fn api_root_routed(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> impl IntoResponse {
    api_root_for_provider(&state, &provider).await
}

async fn api_root_for_provider(state: &AppState, provider: &str) -> Response {
    if state.provider(provider).is_some() {
        StatusCode::OK.into_response()
    } else {
        (StatusCode::NOT_FOUND, format!("unknown provider: {provider}")).into_response()
    }
}

pub async fn handle_models_default(State(state): State<AppState>) -> Response {
    let provider = state.default_provider.clone();
    handle_models_inner(state, provider.as_str()).await
}

pub async fn handle_models_routed(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Response {
    handle_models_inner(state, &provider).await
}

async fn handle_models_inner(state: AppState, provider: &str) -> Response {
    let Some(runtime) = state.provider(provider) else {
        return (StatusCode::NOT_FOUND, format!("unknown provider: {provider}")).into_response();
    };
    info!(provider, "GET /{provider}/v1/models");
    let url = format!("{}models", join_base(&runtime.upstream));
    let mut builder = state.client.get(&url);
    if !runtime.api_key.is_empty() {
        builder = builder.bearer_auth(runtime.api_key.as_str());
    }

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

    let kind = ProviderKind::from_route(provider).unwrap_or(ProviderKind::Custom);
    for known in kind.known_models() {
        if seen.insert((*known).to_string()) {
            list.push(json!({
                "id": known,
                "object": "model",
                "owned_by": "crabbridge",
            }));
        }
    }
    let default_model = runtime.default_model.as_str();
    if seen.insert(default_model.to_string()) {
        list.push(json!({
            "id": default_model,
            "object": "model",
            "owned_by": "crabbridge",
        }));
    }

    Json(json!({
        "object": "list",
        "data": list.clone(),
        "models": list,
    }))
    .into_response()
}

pub async fn handle_fallback(req: Request) -> Response {
    warn!("unhandled {} {}", req.method(), req.uri().path());
    (StatusCode::NOT_FOUND, "not found").into_response()
}

pub async fn handle_responses_default(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Response {
    let provider = state.default_provider.clone();
    handle_responses_for_provider(state, provider.as_str(), body).await
}

pub async fn handle_responses_routed(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    handle_responses_for_provider(state, &provider, body).await
}

async fn handle_responses_for_provider(
    state: AppState,
    provider: &str,
    body: axum::body::Bytes,
) -> Response {
    let Some(runtime) = state.provider(provider) else {
        return (StatusCode::NOT_FOUND, format!("unknown provider: {provider}")).into_response();
    };

    let started = Instant::now();
    info!(provider, bytes = body.len(), "POST /{provider}/v1/responses");
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
            return (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()).into_response();
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

    handle_responses_inner(state, provider, runtime, req, started).await
}

async fn handle_responses_inner(
    state: AppState,
    provider: &str,
    runtime: ProviderRuntime,
    req: ResponsesRequest,
    started: Instant,
) -> Response {
    let mut history = req
        .previous_response_id
        .as_deref()
        .map(|id| state.sessions.get_history(provider, id))
        .unwrap_or_default();
    let history_messages = history.len();
    if should_isolate_spawn_child_request(&req, &history) {
        debug!("isolating spawned child request from parent response history");
        history.clear();
    }

    let model = req.model.clone();
    let namespace_tools = translate::namespace_tool_map(&req.tools);
    let mut chat_req = translate::to_chat_request(
        &req,
        history,
        &state.sessions,
        provider,
        runtime.default_model.as_str(),
        runtime.model_map.as_deref(),
        runtime.default_max_tokens,
        runtime.default_temperature,
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
    let url = format!("{}chat/completions", join_base(&runtime.upstream));

    if req.stream {
        let response_id = state.sessions.new_id();
        chat_req.stream = true;
        let request_messages = chat_req.messages.clone();
        stream::translate_stream(StreamArgs {
            client: state.client,
            url,
            api_key: runtime.api_key,
            chat_req,
            upstream_request: state.upstream_request,
            response_id,
            provider: provider.to_string(),
            sessions: state.sessions,
            request_messages,
            namespace_tools,
            model,
            started,
        })
        .into_response()
    } else {
        chat_req.stream = false;
        handle_blocking(
            state,
            provider,
            runtime,
            chat_req,
            url,
            model,
            namespace_tools,
            started,
        )
        .await
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

async fn handle_blocking(
    state: AppState,
    provider: &str,
    runtime: ProviderRuntime,
    chat_req: ChatRequest,
    url: String,
    model: String,
    namespace_tools: translate::NamespaceToolMap,
    started: Instant,
) -> Response {
    let messages = chat_req.messages.len();
    let tools = chat_req.tools.len();

    if let Some(cache) = &state.cache
        && let Ok(body) = state.upstream_request.request_body(&chat_req)
    {
        let key = ResponseCache::cache_key(provider, &body);
        if let Some(cached) = cache.get(&key).await {
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
    }

    let mut builder = state
        .client
        .post(&url)
        .header("Content-Type", "application/json");

    if !runtime.api_key.is_empty() {
        builder = builder.bearer_auth(runtime.api_key.as_str());
    }

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
    let request_bytes = serde_json::to_vec(&upstream_body).map(|b| b.len()).unwrap_or(0);

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
                let response_id = state.sessions.save(provider, full_history);

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
                    let key = ResponseCache::cache_key(provider, &body);
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
