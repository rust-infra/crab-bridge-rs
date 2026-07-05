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
```

One `crabridge serve` process can host **multiple upstream providers**. Codex selects the upstream via `base_url` path:

| Codex `base_url` | Upstream |
|------------------|----------|
| `http://127.0.0.1:11435/deepseek/v1` | DeepSeek |
| `http://127.0.0.1:11435/kimi/v1` | Kimi Code |

Legacy `/v1/*` routes still work and map to `default_provider` from `crabbridge.toml`.

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
| Errors | anyhow + thiserror |
| CORS | tower-http CorsLayer |
| SSE parsing | eventsource-stream, async-stream |

**Does not use** `async-openai`. Responses / Chat Completions types are defined in `src/types.rs`.

---

## 3. Project Structure

```
crab-bridge-rs/
├── Cargo.toml
├── crabbridge.example.toml
├── .gitignore
├── AGENT_SPEC.md
├── src/
│   ├── main.rs              # Entry: serve / prompt / setup / print-codex-config
│   ├── lib.rs               # Module exports
│   ├── opts.rs              # Clap CLI (ServeArgs, SetupArgs, etc.)
│   ├── config.rs            # crabbridge.toml load + provider resolution
│   ├── provider.rs          # DeepSeek / Kimi presets, route slugs
│   ├── setup.rs             # setup + setup --docker config checks
│   ├── handlers.rs          # HTTP handlers (routed + legacy /v1)
│   ├── state.rs             # AppState + per-provider ProviderRuntime
│   ├── types.rs             # Responses + Chat Completions type definitions
│   ├── translate.rs         # Responses ↔ Chat conversion, model/tool mapping
│   ├── stream.rs            # Chat SSE → Responses SSE streaming translation
│   ├── session.rs           # Session store (previous_response_id, reasoning replay)
│   ├── session_sqlite.rs    # SQLite persistence layer (keyed by provider slug)
│   ├── upstream_request.rs  # Upstream request body + tool denylist
│   ├── cache.rs             # Non-streaming response cache (moka)
│   ├── codex_config.rs      # print-codex-config / setup catalog generation
│   ├── prompt.rs            # ResponsesSseParser (prompt subcommand streaming)
│   └── error.rs             # Error types
└── tests/
    └── integration.rs       # mockito integration tests
```

---

## 4. Module Design

### 4.1 `src/opts.rs`

**Subcommands**:

| Subcommand | Description |
|------------|-------------|
| `serve` | Start the HTTP bridge server (`ServeArgs`) |
| `prompt` | Send a test request to `/{provider}/v1/responses` |
| `setup` | Write Codex config + optional `crabbridge.toml` |
| `setup --docker` | Read-only configuration check |
| `print-codex-config` | Print Codex `config.toml` snippet(s) |

**Key flags**: `--provider deepseek|kimi`, `--all-providers`, `--config crabbridge.toml`.

Priority: **CLI > environment variable > TOML file > defaults**.

### 4.2 `src/config.rs`

Loads `crabbridge.toml` and resolves the provider map for `serve`:

```toml
default_provider = "deepseek"

[providers.deepseek]
base_url = "https://api.deepseek.com/v1"
model_map = "gpt-5.4:deepseek-v4-pro"

[providers.kimi]
base_url = "https://api.kimi.com/coding/v1"
model_map = "gpt-5.4:kimi-for-coding"
```

Provider sections support `base_url` (upstream endpoint override) and `model_map` (per-provider model mapping). The legacy `[upstream]` section (`base_url`, `api_key`, `model`) is still supported as a global fallback.

If no TOML is present but `DEEPSEEK_API_KEY` and/or `KIMI_API_KEY` are set, providers are auto-discovered from env.

### 4.3 `src/state.rs`

```rust
pub struct ProviderRuntime {
    pub upstream: Url,
    pub api_key: Arc<String>,
    pub default_model: Arc<String>,
    pub model_map: Option<String>,  // per-provider CRABRIDGE_MODEL_MAP override
    // ...
}

pub struct AppState {
    pub sessions: SessionStore,
    pub client: Client,
    pub providers: Arc<HashMap<String, ProviderRuntime>>,
    pub default_provider: Arc<String>,
    // ...
}
```

### 4.4 `src/handlers.rs`

**Routes** (see `main.rs` router):

| Handler | Endpoint | Description |
|---------|----------|-------------|
| `health` | `GET /health` | `{ "status": "ok" }` |
| `api_root_routed` | `GET /{provider}/v1` | Codex reachability probe |
| `handle_models_routed` | `GET /{provider}/v1/models` | Proxy upstream models + known_models fallback |
| `handle_responses_routed` | `POST /{provider}/v1/responses` | Core translation path |
| `*_default` | `GET/POST /v1/*` | Legacy → `default_provider` |
| `handle_fallback` | `*` | 404 |

**`handle_responses` flow**:

1. Resolve provider slug from path (`deepseek`, `kimi`, …)
2. Parse `ResponsesRequest`; load history via `SessionStore::get_history(provider, id)`
3. `translate::to_chat_request()` with per-provider `default_model` and `model_map`
4. POST upstream `/chat/completions` with that provider's API key
5. Stream or blocking response back to Responses format
   - For streaming, the upstream request is made before the HTTP response starts, so upstream errors return proper HTTP status codes (401/502/504) instead of an always-200 SSE stream

### 4.5 `src/translate.rs`

**Model mapping** (`map_model_name`):
- Per-provider `model_map` from TOML (`[providers.*.model_map]`) or global `[advanced].model_map`
- Names containing `deepseek` / `kimi` / `moonshot` pass through
- Other Codex names fall back to the provider's configured default model

### 4.6 `src/session.rs` + `src/session_sqlite.rs`

Sessions are keyed by **`(provider_slug, response_id)`** so DeepSeek and Kimi histories do not collide.

SQLite tables: `sessions`, `reasoning`, `turn_reasoning` — composite PK `(provider, …)`.

### 4.7 `src/setup.rs`

- `setup --all-providers`: writes both Codex entries + multi-provider `crabbridge.toml`
- `setup --provider kimi`: single provider only
- `setup --docker`: validates Codex config, catalogs, env keys, and `GET /{slug}/v1` reachability

Codex provider names: `crabbridge-deepseek`, `crabbridge-kimi` (see `ProviderKind::codex_provider_name`).

### 4.8 Other modules

- **`cache.rs`**: Non-streaming response cache (moka)
- **`upstream_request.rs`**: Upstream JSON body + `CRABRIDGE_TOOL_DENYLIST`
- **`codex_config.rs`**: Fetches upstream `/models`, writes `~/.codex/crabbridge-models-{slug}.json`
- **`prompt.rs`**: `ResponsesSseParser` for CLI streaming output

---

## 5. API Endpoints

### 5.1 `POST /{provider}/v1/responses`

Primary Codex entry point. Same JSON as OpenAI Responses API.

### 5.2 `GET /{provider}/v1/models`

Proxies upstream `/models`. On failure, merges provider `known_models` and configured default model so Codex always has catalog entries.

### 5.3 `GET /{provider}/v1`

Empty 200 — Codex reachability probe.

### 5.4 Legacy `/v1/*`

Maps to `default_provider` from config.

### 5.5 `GET /health`

```json
{ "status": "ok" }
```

---

## 6. Configuration

### 6.1 TOML (`crabbridge.toml`)

Search order: `--config` / `CRABRIDGE_CONFIG` → `./crabbridge.toml` → `~/.config/crabbridge/config.toml`.

The config file is loaded and applied to environment variables **before** the full CLI is parsed, so `env = ...` defaults in Clap flags also honor TOML values.

See `crabbridge.example.toml` for the full schema (`[server]`, `[session]`, `[cache]`, `[advanced]`).

### 6.2 Environment variables

| Variable | Description |
|----------|-------------|
| `DEEPSEEK_API_KEY` | DeepSeek upstream key |
| `KIMI_API_KEY` | Kimi Code upstream key |
| `CRABRIDGE_CONFIG` | Path to `crabbridge.toml` |
| `CRABRIDGE_{SLUG}_API_KEY` | Per-provider override from TOML |
| `CRABRIDGE_{SLUG}_BASE_URL` | Per-provider base URL override |
| `CRABRIDGE_{SLUG}_MODEL_MAP` | Per-provider model map |
| `UPSTREAM_BASE_URL` | Global fallback base URL (also set by legacy `[upstream] base_url`) |
| `BRIDGE_ADDR` | Listen address (default `127.0.0.1:11435`) |
| `SESSION_DB` | SQLite path (default `data/crabbridge.db`) |
| `CRABRIDGE_MODEL_MAP` | Global model map (`gpt-5.4:deepseek-v4-pro,…`) |
| `CRABRIDGE_TOOL_DENYLIST` | Comma-separated tools to block |

Empty string values are treated as unset, so a `CRABRIDGE_*` env var with `=""` will not override a TOML value.

Codex still needs `DEEPSEEK_API_KEY` / `KIMI_API_KEY` in the **shell** (`env_key` in Codex config) — separate from bridge TOML.

---

## 7. CLI

```bash
crabridge serve
crabridge serve --config crabbridge.toml
crabridge setup --all-providers
crabridge setup --docker
crabridge prompt "Hello" --provider deepseek
crabridge print-codex-config --provider kimi
crabridge print-codex-config --all-providers
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

Switch providers in Codex by changing `model_provider`, `model`, and `model_catalog_json`.

---

## 9. Build & Run

```bash
cargo build --release
cargo run -- serve          # reads crabbridge.toml
cargo test
cargo clippy --all-targets -- -D warnings
```

---

## 10. Testing Strategy

### 10.1 Unit tests

Conversion, sessions (per-provider isolation), config resolution, setup checks, provider routing.

### 10.2 Integration tests (`tests/integration.rs`)

Uses mockito; exercises `/deepseek/v1/responses`, `/deepseek/v1/models`, and legacy `/v1/responses`.

---

## 11. Design Constraints & Notes

- **Routing**: Provider is determined by URL path, not by guessing from model name.
- **Protocol**: Responses API inbound only; outbound is always Chat Completions.
- **Security**: API keys via env or TOML only; never hardcode.
- **Sessions**: Scoped by provider slug in SQLite and memory.
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
thiserror = "2"
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
