//! Local admin dashboard and Prometheus metrics (`/admin`, `/metrics`).

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use serde::Serialize;

use crate::cache::CacheStats;
use crate::metrics::MetricsSnapshot;
use crate::session::{SessionDetail, SessionStats};
use crate::state::AppState;

const ADMIN_HTML: &str = include_str!("../static/admin.html");

#[derive(Debug, Default, serde::Deserialize)]
pub struct MetricsQuery {
    /// Force response format: `html` (browser view) or `text` (Prometheus scrape).
    pub format: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderOverview {
    pub slug: String,
    pub upstream: String,
    pub default_model: String,
}

#[derive(Debug, Serialize)]
pub struct OverviewResponse {
    pub version: &'static str,
    pub default_provider: String,
    pub providers: Vec<ProviderOverview>,
    pub metrics: MetricsSnapshot,
    pub sessions: SessionStats,
    pub cache: CacheStats,
}

pub async fn dashboard_page() -> Html<&'static str> {
    Html(ADMIN_HTML)
}

pub async fn overview(State(state): State<AppState>) -> Json<OverviewResponse> {
    Json(build_overview(&state))
}

pub async fn session_detail(
    State(state): State<AppState>,
    Path(response_id): Path<String>,
) -> Result<Json<SessionDetail>, StatusCode> {
    state
        .sessions
        .get_session(&response_id)
        .ok_or(StatusCode::NOT_FOUND)
        .map(Json)
}

pub async fn prometheus_metrics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<MetricsQuery>,
) -> Response {
    let body = state.metrics.to_prometheus(state.started_at);
    if body.is_empty() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "metrics export produced empty body",
        )
            .into_response();
    }

    let force_text = query.format.as_deref() == Some("text");
    let force_html = query.format.as_deref() == Some("html");
    let want_html = !force_text && (force_html || prefers_html(&headers));

    if want_html {
        let html = metrics_html_page(&body);
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html,
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

fn prefers_html(headers: &HeaderMap) -> bool {
    let Some(accept) = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let accept = accept.to_ascii_lowercase();
    if accept.contains("application/openmetrics-text") {
        return false;
    }
    if accept.contains("text/plain") && !accept.contains("text/html") {
        return false;
    }
    accept.contains("text/html")
}

fn html_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn metrics_html_page(prometheus_body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>CrabBridge Metrics</title>
  <style>
    :root {{ color-scheme: dark; }}
    body {{
      margin: 0;
      font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
      background: #0f1419;
      color: #e7ecf3;
      line-height: 1.5;
    }}
    header {{
      padding: 1rem 1.25rem;
      border-bottom: 1px solid #2a3648;
      display: flex;
      gap: 1rem;
      flex-wrap: wrap;
      align-items: center;
      font-family: ui-sans-serif, system-ui, sans-serif;
    }}
    h1 {{ margin: 0; font-size: 1.125rem; }}
    .sub {{ color: #8b9cb3; font-size: 0.875rem; }}
    main {{ padding: 1rem 1.25rem; }}
    pre {{
      margin: 0;
      padding: 1rem;
      background: #1a2332;
      border: 1px solid #2a3648;
      border-radius: 10px;
      white-space: pre-wrap;
      word-break: break-word;
      overflow-x: auto;
    }}
    a {{ color: #ff6b35; }}
  </style>
</head>
<body>
  <header>
    <div>
      <h1>CrabBridge Metrics</h1>
      <div class="sub">Prometheus text exposition · refreshes every 3s</div>
    </div>
    <div class="sub">
      <a href="/admin">Admin dashboard</a>
      · <a href="/metrics?format=text">Raw text for scrapers</a>
    </div>
  </header>
  <main>
    <pre id="metrics-body">{body}</pre>
  </main>
  <script>
    async function refresh() {{
      try {{
        const res = await fetch("/metrics?format=text");
        if (!res.ok) throw new Error(res.statusText);
        document.getElementById("metrics-body").textContent = await res.text();
      }} catch (err) {{
        document.getElementById("metrics-body").textContent = "Failed to load metrics: " + err;
      }}
    }}
    setInterval(refresh, 3000);
  </script>
</body>
</html>"#,
        body = html_escape(prometheus_body)
    )
}

fn build_overview(state: &AppState) -> OverviewResponse {
    let providers: Vec<ProviderOverview> = state
        .providers
        .iter()
        .map(|(slug, runtime)| {
            let kind = crabbridge_core::provider::ProviderKind::from_route(slug)
                .unwrap_or(crabbridge_core::provider::ProviderKind::Custom);
            ProviderOverview {
                slug: slug.clone(),
                upstream: runtime.upstream.to_string(),
                default_model: kind.default_model().to_string(),
            }
        })
        .collect();

    OverviewResponse {
        version: env!("CARGO_PKG_VERSION"),
        default_provider: state.default_provider.as_str().to_string(),
        providers,
        metrics: state.metrics.snapshot(state.started_at),
        sessions: state.sessions.stats(),
        cache: state
            .cache
            .as_ref()
            .map(|cache| cache.stats())
            .unwrap_or(CacheStats {
                enabled: false,
                entry_count: 0,
                weighted_size_bytes: 0,
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use reqwest::Client;

    use crate::metrics::BridgeMetrics;
    use crate::session::SessionStore;
    use crate::state::{AppState, ProviderRuntime};
    use crate::upstream_request::UpstreamRequestConfig;

    fn test_state() -> AppState {
        let mut providers = HashMap::new();
        providers.insert(
            "deepseek".to_string(),
            ProviderRuntime {
                upstream: "https://api.deepseek.com/v1".parse().unwrap(),
                default_max_tokens: None,
                default_temperature: None,
                model_map: None,
            },
        );
        AppState {
            sessions: SessionStore::with_limits_and_ttl(64, 1024 * 1024, Duration::from_secs(3600)),
            client: Client::new(),
            providers: Arc::new(providers),
            default_provider: Arc::new("deepseek".to_string()),
            upstream_request: Arc::new(UpstreamRequestConfig::default()),
            cache: None,
            metrics: BridgeMetrics::new(),
            started_at: Instant::now(),
        }
    }

    #[test]
    fn overview_contains_version_and_providers() {
        let overview = build_overview(&test_state());
        assert_eq!(overview.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(overview.default_provider, "deepseek");
        assert_eq!(overview.providers.len(), 1);
        assert!(!overview.cache.enabled);
    }

    #[test]
    fn prefers_html_for_browser_accept() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
                .parse()
                .unwrap(),
        );
        assert!(prefers_html(&headers));
    }

    #[test]
    fn prefers_text_for_prometheus_accept() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            "application/openmetrics-text;version=1.0.0,text/plain;version=0.0.4,*/*;q=0.1"
                .parse()
                .unwrap(),
        );
        assert!(!prefers_html(&headers));
    }

    #[test]
    fn metrics_html_page_includes_prometheus_body() {
        let page = metrics_html_page("crabbridge_uptime_seconds 42\n");
        assert!(page.contains("crabbridge_uptime_seconds 42"));
        assert!(page.contains("CrabBridge Metrics"));
    }
}
