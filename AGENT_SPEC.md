# 🦀 CrabBridge - Agent Spec

> This document guides AI coding assistants (e.g. Cursor) to understand, maintain, or extend the CrabBridge project.

---

## 1. Project Overview

**Name**: CrabBridge  
**Description**: A lightweight Rust protocol-conversion proxy that bridges **Codex CLI** (OpenAI Responses API) to **DeepSeek** or **Kimi Code** Chat Completions APIs.  
**Goal**: Let Codex connect to a local bridge via `wire_api = "responses"`, while the bridge handles protocol translation, session continuity, tool-call mapping, and streaming SSE conversion.

**Core data flow**:

```
Codex CLI  ──Responses API──▶  CrabBridge  ──Chat Completions──▶  DeepSeek / Kimi
         /{provider}/v1/responses              /v1/chat/completions
              Authorization: Bearer <key from Codex env_key>
```

One `crabridge serve` process can host **multiple upstream providers**. Codex selects the upstream via `base_url` path:

| Codex `base_url` | Upstream |
|------------------|----------|
| `http://127.0.0.1:11435/deepseek/v1` | DeepSeek |
| `http://127.0.0.1:11435/kimi/v1` | Kimi Code |

Legacy `/v1/*` routes still work and map to `default_provider` from `crabbridge.toml`.

**Authentication**: Upstream API keys are **not** stored in bridge TOML. Codex sends `Authorization: Bearer …` on each request; the bridge forwards that token to the upstream for the matched route. Shell env vars (`DEEPSEEK_API_KEY`, `KIMI_API_KEY`) are for Codex's `env_key`, not for bridge config.

**Does not expose** `/v1/chat/completions`. Clients must speak the Responses API (Codex CLI is the primary target).

---

## 2. Tech Stack

| Category | Choice |
|----------|--------|
| Language | Rust (edition 2024) |
| Runtime | Tokio |
| Web framework | Axum 0.8 |
| HTTP client | Reqwest (rustls-tls, stream) |
| Serialization | Serde / Serde_json |
| CLI | Clap 4 (derive + env) |
| Session persistence | rusqlite (bundled, WAL) |
| Response cache | moka |
| Rate limiting | tower_governor |
| Logging | tracing + tracing-subscriber |
| Config | TOML (`crabbridge.toml`) |
| Errors | anyhow |
| CORS | tower-http CorsLayer |
| SSE parsing | eventsource-stream, async-stream |

**Does not use** `async-openai`. Responses / Chat Completions types are defined in `crates/crabbridge-core/src/types.rs`.

---

## 3. Project Structure

Cargo **workspace** with three crates. Shared logic lives in `crabbridge-core`; CLI and server are separate binaries with independent dependency trees.

```
crab-bridge-rs/
├── Cargo.toml                      # workspace root
├── crabbridge.example.toml
├── .gitignore
├── AGENT_SPEC.md
├── crates/
│   ├── crabbridge-core/            # shared library
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs            # Responses + Chat Completions type definitions
│   │       ├── provider.rs         # DeepSeek / Kimi presets, route slugs, model matching
│   │       ├── config.rs           # crabbridge.toml load + provider resolution
│   │       └── runtime.rs          # shared init + Tokio block_on
│   ├── crabbridge-cli/             # crabridge-cli binary + library
│   │   └── src/
│   │       ├── main.rs             # thin entry
│   │       ├── lib.rs
│   │       ├── cli.rs              # setup / print-codex-config handlers
│   │       ├── cli_opts.rs         # Clap for crabridge-cli (CrabridgeCli)
│   │       ├── codex_config.rs     # Fetches upstream /models, writes Codex snippets
│   │       └── setup.rs            # setup + setup --docker config checks
│   └── crabbridge-server/          # crabridge binary + library
│       ├── static/
│       │   └── admin.html          # embedded admin dashboard
│       ├── tests/
│       │   └── integration.rs      # mockito integration tests
│       └── src/
│           ├── main.rs             # thin entry
│           ├── lib.rs
│           ├── server.rs           # serve / prompt handlers
│           ├── opts.rs             # Clap for crabridge (BridgeCli)
│           ├── app.rs              # build_router()
│           ├── handlers.rs         # HTTP handlers (routed + legacy /v1)
│           ├── state.rs            # AppState + per-provider ProviderRuntime
│           ├── translate.rs        # Responses ↔ Chat conversion, model/tool mapping
│           ├── stream.rs           # Chat SSE → Responses SSE streaming translation
│           ├── session.rs          # Session store (previous_response_id, reasoning replay)
│           ├── session_sqlite.rs   # SQLite persistence (PK = response_id / reasoning key)
│           ├── upstream_request.rs # Upstream request body + tool denylist
│           ├── cache.rs            # Non-streaming response cache (moka)
│           ├── metrics.rs          # Runtime counters + Prometheus export
│           ├── admin.rs            # /admin dashboard + /metrics handlers
│           └── prompt.rs           # ResponsesSseParser (prompt subcommand streaming)
```

**Crate dependencies**: `crabbridge-cli` → `crabbridge-core`; `crabbridge-server` → `crabbridge-core`. The CLI crate does not depend on axum, rusqlite, or moka.

---

## 4. Module Design

### 4.1 CLI binaries

**`crabridge`** (`crabbridge-server::opts`):

| Subcommand | Description |
|------------|-------------|
| `serve` | Start the HTTP bridge server (`ServeArgs`) |
| `prompt` | Send a test request to `/{provider}/v1/responses` |

**`crabridge-cli`** (`crabbridge-cli::cli_opts`):

| Subcommand | Description |
|------------|-------------|
| `setup` | Write Codex config + optional `crabbridge.toml` |
| `setup --docker` | Read-only configuration check |
| `print-codex-config` | Print Codex `config.toml` snippet(s) |

**Global flags** (both binaries): `-c` / `--config PATH` (also `CRABRIDGE_CONFIG`).

**Setup flags**: `--provider deepseek\|kimi`, `--all-providers`, `--providers=kimi,deepseek`.

Priority: **CLI flags > environment variable > TOML file > built-in defaults**.

### 4.2 `crabbridge-core::config`

Loads `crabbridge.toml` and resolves the provider map for `serve`:

```toml
default_provider = "deepseek"

[providers.deepseek]
base_url = "https://api.deepseek.com/v1"   # optional override
model_map = "gpt-5.4:deepseek-v4-pro"      # optional

[providers.kimi]
base_url = "https://api.kimi.com/coding/v1"
model_map = "gpt-5.4:kimi-for-coding"
```

**Provider resolution rules**:

- Each `[providers.{slug}]` section enables that route (no API key required in TOML).
- If TOML has **no** `[providers.*]` sections, fall back to both built-in slugs (`deepseek`, `kimi`).
- Legacy `[provider]` + `[upstream]` still supported as a single-provider fallback.
- `default_provider` selects the legacy `/v1/*` route; defaults to first slug alphabetically if unset.

`ProviderSection` fields: `base_url`, `model_map` only (no `api_key`, no `model`).

### 4.3 `crabbridge-server::state`

```rust
pub struct ProviderRuntime {
    pub upstream: Url,
    pub default_max_tokens: Option<u32>,
    pub default_temperature: Option<f32>,
    pub model_map: Option<String>,  // per-provider CRABRIDGE_MODEL_MAP override
}

pub struct AppState {
    pub sessions: SessionStore,
    pub client: Client,
    pub providers: Arc<HashMap<String, ProviderRuntime>>,
    pub default_provider: Arc<String>,
    pub upstream_request: Arc<UpstreamRequestConfig>,
    pub cache: Option<SharedResponseCache>,
    pub metrics: Arc<BridgeMetrics>,
    pub started_at: Instant,
}
```

Default upstream model per route comes from `ProviderKind::default_model()`, not from TOML.

### 4.4 `crabbridge-server::handlers`

**Routes** (see `app.rs` / `build_router`):

| Handler | Endpoint | Description |
|---------|----------|-------------|
| `health` | `GET /health` | `{ "status": "ok" }` |
| `api_root` | `GET /{provider}/v1` | Codex reachability probe |
| `handle_models` | `GET /{provider}/v1/models` | Proxy upstream models + known_models fallback |
| `handle_responses` | `POST /{provider}/v1/responses` | Core translation path |
| legacy handlers | `GET/POST /v1/*` | → `default_provider` |
| `handle_fallback` | `*` | 404 |

**`handle_responses` flow**:

1. Resolve provider slug from path (`deepseek`, `kimi`, …)
2. Require `Authorization: Bearer` header (401 if missing)
3. Parse `ResponsesRequest`; load history via `SessionStore::get_history(response_id)` — **no provider in lookup key**
4. `translate::to_chat_request()` with `ProviderKind`, per-provider `model_map`, and `ProviderKind::default_model()`
5. POST upstream `/chat/completions` with the request Bearer token
6. Stream or blocking response back to Responses format
   - Streaming: upstream request is made before the HTTP response starts, so upstream errors return proper HTTP status codes (401/502/504) instead of an always-200 SSE stream

**`handle_models`**: Filters upstream model IDs to those matching the route provider; merges `known_models_for_upstream(base_url)`.

### 4.5 `crabbridge-server::translate`

**Model mapping** (`map_model_name`):

- Per-provider `model_map` from TOML (`[providers.*.model_map]`) or global `[advanced].model_map` / `CRABRIDGE_MODEL_MAP`
- Explicit map pairs take precedence
- Upstream model IDs **pass through only when they match the active provider** (`ProviderKind::model_matches_provider`)
- Other Codex names fall back to `ProviderKind::default_model()` for that route

Examples:

| Route | Codex `model` | Upstream `model` |
|-------|---------------|------------------|
| `/kimi/v1` | `deepseek-v4-pro` | `kimi-for-coding` |
| `/kimi/v1` | `kimi-for-coding` | `kimi-for-coding` |
| `/deepseek/v1` | `gpt-5.4` + map | mapped target |

### 4.6 `crabbridge-server::session` + `session_sqlite`

Sessions are keyed by **`response_id` only** for lookup. Codex can switch `model_provider` mid-conversation and still resume via `previous_response_id`.

SQLite schema:

| Table | Primary key | Notes |
|-------|-------------|-------|
| `sessions` | `response_id` | `provider` column updated on write (indexed, not used for lookup) |
| `reasoning` | `key` (call_id) | `provider` indexed |
| `turn_reasoning` | `key` (content hash) | `provider` indexed |

Write paths pass `provider` slug to update the indexed column: `save_with_id(provider, id, messages)`, `store_reasoning(provider, …)`.

No schema migration code — greenfield `init_schema()` on open.

### 4.7 `crabbridge-cli::setup`

Invoked by **`crabridge-cli setup`**:

- `setup --all-providers`: writes both Codex entries + multi-provider `crabbridge.toml` (empty `[providers.*]` stubs)
- `setup --providers=kimi,deepseek`: same, explicit slug list
- `setup --provider kimi`: single provider only
- `setup --docker`: validates Codex config, catalogs, env keys, and `GET /{slug}/v1` reachability

Codex provider names: `crabbridge-deepseek`, `crabbridge-kimi` (see `ProviderKind::codex_provider_name`).

### 4.8 `crabbridge-core::provider`

| Slug | Default upstream | Default model | Codex `env_key` |
|------|------------------|---------------|-----------------|
| `deepseek` | `https://api.deepseek.com/v1` | `deepseek-v4-pro` | `DEEPSEEK_API_KEY` |
| `kimi` | `https://api.kimi.com/coding/v1` | `kimi-for-coding` | `KIMI_API_KEY` |

Kimi Code (`api.kimi.com/coding/v1`) and Moonshot Open Platform (`api.moonshot.ai/v1`) are separate systems — different keys and model IDs.

### 4.9 `crabbridge-server::metrics` + `admin`

**Metrics** (`BridgeMetrics` in `AppState`):

- Request/error/stream/cache counters per provider
- `GET /metrics` — Prometheus text format
- `GET /admin/api/overview` — JSON snapshot for the dashboard

**Admin UI**:

- `GET /admin` — embedded HTML dashboard (`crates/crabbridge-server/static/admin.html`), polls overview every 3s
- No authentication (intended for localhost-only use)
- Disable via `[admin] enabled = false` in TOML

Handlers record metrics via `record_http_outcome()` in `handlers.rs`.

### 4.10 Other modules

- **`crabbridge-server::cache`**: Non-streaming response cache (moka); cache key includes provider + Bearer token hash
- **`crabbridge-server::upstream_request`**: Upstream JSON body + `CRABRIDGE_TOOL_DENYLIST`
- **`crabbridge-cli::codex_config`**: Fetches upstream `/models`, writes `~/.codex/crabbridge-models-{slug}.json`
- **`crabbridge-server::prompt`**: `ResponsesSseParser` for CLI streaming output

---

## 5. API Endpoints

### 5.1 `POST /{provider}/v1/responses`

Primary Codex entry point. Same JSON as OpenAI Responses API. Requires `Authorization: Bearer`.

### 5.2 `GET /{provider}/v1/models`

Proxies upstream `/models`. On failure, merges provider `known_models` and preset default model.

### 5.3 `GET /{provider}/v1`

Empty 200 — Codex reachability probe.

### 5.4 `GET /health`

```json
{ "status": "ok" }
```

### 5.5 Admin & metrics

| Route | Description |
|-------|-------------|
| `GET /admin` | Embedded HTML dashboard |
| `GET /admin/api/overview` | JSON: metrics, sessions, cache, providers |
| `GET /metrics` | Prometheus exposition |

Disabled when `[admin] enabled = false`.

### 5.6 Legacy `/v1/*`

Maps to `default_provider` from config.

---

## 6. Configuration

### 6.1 TOML (`crabbridge.toml`)

Search order: `--config` / `-c` / `CRABRIDGE_CONFIG` → `./crabbridge.toml` → `~/.config/crabbridge/config.toml`.

Config is loaded before Clap runs via [`crabbridge_core::config::explicit_config_before_cli`], then all subcommands share the same path from [`crabbridge_core::config::explicit_config_from_cli`] after parsing.

See `crabbridge.example.toml` for the full schema (`[server]`, `[session]`, `[cache]`, `[advanced]`).

### 6.2 Environment variables

| Variable | Description |
|----------|-------------|
| `DEEPSEEK_API_KEY` | DeepSeek key (Codex `env_key`; forwarded as Bearer) |
| `KIMI_API_KEY` | Kimi Code key (same) |
| `CRABRIDGE_CONFIG` | Path to `crabbridge.toml` |
| `CRABRIDGE_{SLUG}_BASE_URL` | Per-route upstream URL override |
| `CRABRIDGE_DEFAULT_PROVIDER` | Legacy `/v1/*` default slug |
| `BRIDGE_ADDR` | Listen address (default `127.0.0.1:11435`) |
| `SESSION_DB` | SQLite path (default `data/crabbridge.db`) |
| `CRABRIDGE_MODEL_MAP` | Global model map |
| `CRABRIDGE_TOOL_DENYLIST` | Comma-separated tools to block |

Empty string env values are treated as unset where applicable.

---

## 7. CLI

```bash
# crabridge — server
crabridge serve
crabridge serve --config crabbridge.toml
crabridge prompt "Hello" --provider deepseek

# crabridge-cli — Codex setup
crabridge-cli setup --all-providers
crabridge-cli setup --providers=kimi,deepseek
crabridge-cli setup --docker
crabridge-cli print-codex-config --provider kimi
crabridge-cli print-codex-config --all-providers
```

---

## 8. Codex Integration

Example (multi-provider):

```toml
model_provider = "crabbridge-deepseek"
model = "deepseek-v4-pro"
model_catalog_json = "/Users/you/.codex/crabbridge-models-deepseek.json"

[model_providers.crabbridge-deepseek]
name = "crabbridge-deepseek"
base_url = "http://127.0.0.1:11435/deepseek/v1"
wire_api = "responses"
env_key = "DEEPSEEK_API_KEY"

[model_providers.crabbridge-kimi]
name = "crabbridge-kimi"
base_url = "http://127.0.0.1:11435/kimi/v1"
wire_api = "responses"
env_key = "KIMI_API_KEY"
```

Switch providers in Codex by changing `model_provider` and `model`.

---

## 9. Build & Run

```bash
cargo build --workspace --release
cargo run --bin crabridge -- serve
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

---

## 10. Testing Strategy

### 10.1 Unit tests

Conversion, sessions (response_id keying, cross-provider resume), config resolution, setup checks, provider-aware model mapping.

### 10.2 Integration tests (`crates/crabbridge-server/tests/integration.rs`)

Uses mockito; exercises `/deepseek/v1/responses`, `/deepseek/v1/models`, and legacy `/v1/responses`.

---

## 11. Design Constraints & Notes

- **Routing**: Provider is determined by URL path, not by guessing from model name.
- **Protocol**: Responses API inbound only; outbound is always Chat Completions.
- **Security**: Upstream keys via Codex Bearer header; never hardcode keys in TOML.
- **Sessions**: Lookup by `response_id`; `provider` column is metadata only.
- **Multi-provider TOML**: Only slugs with a `[providers.{slug}]` section (or no config → both built-ins) are served.
- **CORS**: `CorsLayer::permissive()`.

---

## 12. Dependency Versions

```
axum = "0.8"
reqwest = "0.12"          # features: json, stream, rustls-tls
serde = "1"
serde_json = "1"
tokio = "1"               # features: full
clap = "4"                # features: derive, env
tower-http = "0.6"        # features: cors
tower_governor = "0.8"
tracing = "0.1"
tracing-subscriber = "0.3"  # features: env-filter
toml = "0.8"
toml_edit = "0.22"
anyhow = "1"
futures-util = "0.3"
moka = "0.12"             # features: future
bytes = "1"
uuid = "1"                # features: v4
eventsource-stream = "0.2"
async-stream = "0.3"
rusqlite = "0.32"         # features: bundled
```

---

**End of Agent Spec**. Use this document to understand CrabBridge's path-routed, multi-provider Responses bridge and maintain or extend the codebase correctly.
