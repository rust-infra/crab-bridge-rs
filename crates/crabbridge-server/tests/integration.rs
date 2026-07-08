use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use crabbridge_server::app::build_router;
use crabbridge_server::metrics::BridgeMetrics;
use crabbridge_server::session::SessionStore;
use crabbridge_server::state::{AppState, ProviderRuntime};
use crabbridge_server::upstream_request::UpstreamRequestConfig;
use reqwest::Client;
use serde_json::json;
use std::time::Instant;
use tokio::net::TcpListener;

const TEST_BEARER: &str = "test-key";

async fn spawn_test_server(
    mock_base_url: &str,
    provider_slug: &str,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");

    let mut providers = HashMap::new();
    providers.insert(
        provider_slug.to_string(),
        ProviderRuntime {
            upstream: mock_base_url
                .parse()
                .expect("mock upstream url must be valid"),
            default_max_tokens: None,
            default_temperature: None,
            model_map: None,
        },
    );

    let state = AppState {
        sessions: SessionStore::with_limits_and_ttl(
            64,
            16 * 1024 * 1024,
            Duration::from_secs(3600),
        ),
        client: Client::new(),
        providers: Arc::new(providers),
        upstream_request: Arc::new(UpstreamRequestConfig::default()),
        cache: None,
        metrics: BridgeMetrics::new(),
        started_at: Instant::now(),
    };

    let app = build_router(state, true);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("test server failed");
    });

    (addr, handle)
}

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let mock = mockito::Server::new_async().await;
    let (addr, _handle) = spawn_test_server(&mock.url(), "deepseek").await;

    let response: serde_json::Value = Client::new()
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .expect("send")
        .json()
        .await
        .expect("json");

    assert_eq!(response["status"], "ok");
}

#[tokio::test]
async fn non_stream_response_is_translated() {
    let mut mock = mockito::Server::new_async().await;
    let _mock = mock
        .mock("POST", "/v1/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"hi there"}}]}"#)
        .create_async()
        .await;

    let (addr, _handle) = spawn_test_server(&format!("{}/v1", mock.url()), "deepseek").await;

    let response = Client::new()
        .post(format!("http://{addr}/deepseek/v1/responses"))
        .header("Authorization", format!("Bearer {TEST_BEARER}"))
        .json(&json!({
            "model": "gpt-5.4",
            "input": "hello",
            "stream": false
        }))
        .send()
        .await
        .expect("send");

    let status = response.status();
    let body = response.text().await.expect("body");
    assert!(status.is_success(), "status {status} body {body}");

    let response: serde_json::Value = serde_json::from_str(&body).expect("json");

    assert_eq!(response["object"], "response");
    assert_eq!(response["output"][0]["content"][0]["text"], "hi there");
}

#[tokio::test]
async fn responses_without_authorization_returns_401() {
    let mock = mockito::Server::new_async().await;
    let (addr, _handle) = spawn_test_server(&format!("{}/v1", mock.url()), "deepseek").await;

    let response = Client::new()
        .post(format!("http://{addr}/deepseek/v1/responses"))
        .json(&json!({
            "model": "gpt-5.4",
            "input": "hello",
            "stream": false
        }))
        .send()
        .await
        .expect("send");

    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn stream_response_is_translated() {
    let mut mock = mockito::Server::new_async().await;
    let _mock = mock
        .mock("POST", "/v1/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body("data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\ndata: [DONE]\n\n")
        .create_async()
        .await;

    let (addr, _handle) = spawn_test_server(&format!("{}/v1", mock.url()), "deepseek").await;

    let body = Client::new()
        .post(format!("http://{addr}/deepseek/v1/responses"))
        .header("Authorization", format!("Bearer {TEST_BEARER}"))
        .json(&json!({
            "model": "gpt-5.4",
            "input": "hello",
            "stream": true
        }))
        .send()
        .await
        .expect("send")
        .text()
        .await
        .expect("text");

    assert!(body.contains("response.output_text.delta"));
    assert!(body.contains("response.completed"));
}

#[tokio::test]
async fn stream_without_done_completes_when_content_received() {
    let mut mock = mockito::Server::new_async().await;
    let _mock = mock
        .mock("POST", "/v1/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body("data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n")
        .create_async()
        .await;

    let (addr, _handle) = spawn_test_server(&format!("{}/v1", mock.url()), "deepseek").await;

    let body = Client::new()
        .post(format!("http://{addr}/deepseek/v1/responses"))
        .header("Authorization", format!("Bearer {TEST_BEARER}"))
        .json(&json!({
            "model": "gpt-5.4",
            "input": "hello",
            "stream": true
        }))
        .send()
        .await
        .expect("send")
        .text()
        .await
        .expect("text");

    assert!(body.contains("response.output_text.delta"));
    assert!(body.contains("response.completed"));
}

#[tokio::test]
async fn models_endpoint_proxies_upstream() {
    let mut mock = mockito::Server::new_async().await;
    let _mock = mock
        .mock("GET", "/v1/models")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":[{"id":"deepseek-chat"}]}"#)
        .create_async()
        .await;

    let (addr, _handle) = spawn_test_server(&format!("{}/v1", mock.url()), "deepseek").await;

    let response: serde_json::Value = Client::new()
        .get(format!("http://{addr}/deepseek/v1/models"))
        .header("Authorization", format!("Bearer {TEST_BEARER}"))
        .send()
        .await
        .expect("send")
        .json()
        .await
        .expect("json");

    assert_eq!(response["data"][0]["id"], "deepseek-chat");
    assert_eq!(response["models"][0]["id"], "deepseek-chat");
}

#[tokio::test]
async fn kimi_models_endpoint_proxies_upstream() {
    let mut mock = mockito::Server::new_async().await;
    let _mock = mock
        .mock("GET", "/v1/models")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"data":[{"id":"kimi-k2-for-coding"}]}"#)
        .create_async()
        .await;

    let (addr, _handle) = spawn_test_server(&format!("{}/v1", mock.url()), "kimi").await;

    let response: serde_json::Value = Client::new()
        .get(format!("http://{addr}/kimi/v1/models"))
        .header("Authorization", format!("Bearer {TEST_BEARER}"))
        .send()
        .await
        .expect("send")
        .json()
        .await
        .expect("json");

    assert_eq!(response["data"][0]["id"], "kimi-k2-for-coding");
    assert_eq!(response["models"][0]["id"], "kimi-k2-for-coding");
}

#[tokio::test]
async fn kimi_upstream_requests_include_user_agent() {
    use crabbridge_core::provider::KIMI_UPSTREAM_USER_AGENT;

    let mut mock = mockito::Server::new_async().await;
    let _mock = mock
        .mock("POST", "/v1/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .match_header("user-agent", KIMI_UPSTREAM_USER_AGENT)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"kimi ok"}}]}"#)
        .create_async()
        .await;

    let (addr, _handle) = spawn_test_server(&format!("{}/v1", mock.url()), "kimi").await;

    let response = Client::new()
        .post(format!("http://{addr}/kimi/v1/responses"))
        .header("Authorization", format!("Bearer {TEST_BEARER}"))
        .json(&json!({
            "model": "kimi-for-coding",
            "input": "hello",
            "stream": false
        }))
        .send()
        .await
        .expect("send")
        .text()
        .await
        .expect("body");

    let body: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(body["output"][0]["content"][0]["text"], "kimi ok");
}

#[tokio::test]
async fn client_authorization_header_is_forwarded_to_upstream() {
    let mut mock = mockito::Server::new_async().await;
    let _mock = mock
        .mock("POST", "/v1/chat/completions")
        .match_header("authorization", "Bearer codex-client-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"forwarded"}}]}"#)
        .create_async()
        .await;

    let (addr, _handle) = spawn_test_server(&format!("{}/v1", mock.url()), "deepseek").await;

    let response = Client::new()
        .post(format!("http://{addr}/deepseek/v1/responses"))
        .header("Authorization", "Bearer codex-client-key")
        .json(&json!({
            "model": "gpt-5.4",
            "input": "hello",
            "stream": false
        }))
        .send()
        .await
        .expect("send")
        .text()
        .await
        .expect("body");

    let body: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(body["output"][0]["content"][0]["text"], "forwarded");
}

#[tokio::test]
async fn admin_dashboard_and_metrics_endpoints() {
    let mock = mockito::Server::new_async().await;
    let (addr, _handle) = spawn_test_server(&mock.url(), "deepseek").await;

    let client = Client::new();
    let admin = client
        .get(format!("http://{addr}/admin"))
        .send()
        .await
        .expect("admin page");
    assert!(admin.status().is_success());
    assert!(
        admin
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("text/html"))
    );

    let overview: serde_json::Value = client
        .get(format!("http://{addr}/admin/api/overview"))
        .send()
        .await
        .expect("overview")
        .json()
        .await
        .expect("json");
    assert!(overview.get("version").is_some());
    assert!(overview["metrics"]["uptime_secs"].is_number());
    assert!(overview["providers"].is_array());

    let metrics = client
        .get(format!("http://{addr}/metrics"))
        .send()
        .await
        .expect("metrics")
        .text()
        .await
        .expect("body");
    assert!(!metrics.is_empty());
    assert!(metrics.contains("crabbridge_uptime_seconds"));
    assert!(metrics.contains("crabbridge_requests_total"));
    assert!(metrics.contains("crabbridge_request_duration_ms_sum"));

    let metrics_html = client
        .get(format!("http://{addr}/metrics"))
        .header("Accept", "text/html")
        .send()
        .await
        .expect("metrics html")
        .text()
        .await
        .expect("html body");
    assert!(metrics_html.contains("CrabBridge Metrics"));
    assert!(metrics_html.contains("crabbridge_uptime_seconds"));
}

#[tokio::test]
async fn admin_session_detail_endpoint_returns_messages() {
    let mut mock = mockito::Server::new_async().await;
    let _mock = mock
        .mock("POST", "/v1/chat/completions")
        .match_header("authorization", "Bearer test-key")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"hi there"}}]}"#)
        .create_async()
        .await;

    let (addr, _handle) = spawn_test_server(&format!("{}/v1", mock.url()), "deepseek").await;

    let client = Client::new();
    let response = client
        .post(format!("http://{addr}/deepseek/v1/responses"))
        .header("Authorization", format!("Bearer {TEST_BEARER}"))
        .json(&json!({
            "model": "gpt-5.4",
            "input": "hello",
            "stream": false
        }))
        .send()
        .await
        .expect("send");

    assert!(response.status().is_success());
    let body: serde_json::Value = response.json().await.expect("json");
    let response_id = body["id"].as_str().expect("response id");

    let session: serde_json::Value = client
        .get(format!("http://{addr}/admin/api/sessions/{response_id}"))
        .send()
        .await
        .expect("session detail")
        .json()
        .await
        .expect("json");

    assert_eq!(session["response_id"], response_id);
    assert_eq!(session["provider"], "deepseek");
    assert!(session["messages"].is_array());
    assert_eq!(session["messages"].as_array().unwrap().len(), 2);
    assert_eq!(session["messages"][0]["role"], "user");
    assert_eq!(session["messages"][1]["role"], "assistant");
}

#[tokio::test]
async fn admin_session_detail_endpoint_returns_404_for_unknown() {
    let mock = mockito::Server::new_async().await;
    let (addr, _handle) = spawn_test_server(&mock.url(), "deepseek").await;

    let response = Client::new()
        .get(format!("http://{addr}/admin/api/sessions/resp_missing"))
        .send()
        .await
        .expect("send");

    assert_eq!(response.status(), 404);
}
