# 🦀 CrabBridge - Agent Spec

> This document guides AI coding assistants (e.g. Cursor) to understand, maintain, or extend the CrabBridge project.

---

## 1. Project Overview

**Name**: CrabBridge  
**Description**: A lightweight Rust protocol-conversion proxy that bridges **Codex CLI** (OpenAI Responses API) requests to the **DeepSeek Chat Completions API**.  
**Goal**: Let Codex connect to a local bridge via `wire_api = "responses"`, while the bridge handles protocol translation, session continuity, tool-call mapping, and streaming SSE conversion.

**Core data flow**:

```
Codex CLI  ──Responses API──▶  CrabBridge  ──Chat Completions──▶  DeepSeek
              /v1/responses              /v1/chat/completions
```

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
| Config | dotenv |
| Errors | anyhow + thiserror |
| CORS | tower-http CorsLayer |
| SSE parsing | eventsource-stream, async-stream |

**Does not use** `async-openai`. Responses / Chat Completions types are defined in `src/types.rs`.

---

## 3. Project Structure

```
crab-bridge-rs/
├── Cargo.toml
├── .env.example
├── .gitignore
├── AGENT_SPEC.md
├── src/
│   ├── main.rs              # Entry: serve / prompt / print-codex-config
│   ├── lib.rs               # Module exports
│   ├── opts.rs              # Clap CLI (ServeArgs, etc.)
│   ├── handlers.rs          # HTTP handlers (/v1/responses, /v1/models, /health)
│   ├── state.rs             # AppState shared state
│   ├── types.rs             # Responses + Chat Completions type definitions
│   ├── translate.rs         # Responses ↔ Chat conversion, model/tool mapping
│   ├── stream.rs            # Chat SSE → Responses SSE streaming translation
│   ├── session.rs           # Session store (previous_response_id, reasoning replay)
│   ├── session_sqlite.rs    # SQLite persistence layer
│   ├── upstream_request.rs  # Upstream request body + tool denylist
│   ├── cache.rs             # Non-streaming response cache (moka)
│   ├── codex_config.rs      # print-codex-config Codex config.toml snippet
│   ├── prompt.rs            # ResponsesSseParser (prompt subcommand streaming)
│   └── error.rs             # Error types
└── tests/
    └── integration.rs       # mockito integration tests
```

---

## 4. Module Design

### 4.1 `src/opts.rs`

**Role**: Define CLI subcommands and arguments.

**Subcommands**:

| Subcommand | Description |
|------------|-------------|
| `serve` | Start the HTTP bridge server (args in `ServeArgs`) |
| `prompt` | Send a test request to local `/v1/responses` |
| `print-codex-config` | Print a Codex `config.toml` snippet |

**Key `ServeArgs` fields**: `api_key`, `base_url`, `model`, `bind_addr`, `max_tokens`, `temperature`, `log_level`, `cache_enabled`, `cache_ttl_secs`, `cache_max_entries`, `rate_limit_rps`, `max_sessions`, `session_ttl_hours`, `session_db`, `session_memory_only`.

All fields support the `env` attribute. Priority: **CLI > environment variable > default**.

### 4.2 `src/state.rs`

**Shared state**:

```rust
pub struct AppState {
    pub sessions: SessionStore,
    pub client: Client,
    pub upstream: Arc<Url>,           // e.g. https://api.deepseek.com/v1
    pub api_key: Arc<String>,
    pub default_model: Arc<String>,
    pub default_max_tokens: Option<u32>,
    pub default_temperature: Option<f32>,
    pub upstream_request: Arc<UpstreamRequestConfig>,
    pub cache: Option<SharedResponseCache>,
}
```

### 4.3 `src/handlers.rs`

**Route handlers**:

| Handler | Endpoint | Description |
|---------|----------|-------------|
| `health` | `GET /health` | Returns `{ "status": "ok" }` |
| `handle_models` | `GET /v1/models` | Proxies upstream model list |
| `handle_responses` | `POST /v1/responses` | Core: Responses → Chat → upstream → back to Responses |
| `handle_fallback` | `*` | 404 |

**`handle_responses` flow**:

1. Parse `ResponsesRequest` (JSON body)
2. Load prior messages from `SessionStore` via `previous_response_id`
3. Convert with `translate::to_chat_request()` → `ChatRequest`
4. Map model name via `translate::map_model_name()` (includes `CRABRIDGE_MODEL_MAP`)
5. Non-streaming: POST upstream `/chat/completions`, convert back with `translate::from_chat_response_with_tool_map()`; optionally write to cache
6. Streaming: delegate to `stream::handle_stream()`, translating Chat SSE to Responses SSE events in real time

### 4.4 `src/translate.rs`

**Role**: Bidirectional protocol conversion.

**Inbound (Responses → Chat)**:
- `input` text or message array → `messages[]`
- `instructions` / `system` → system message
- `function_call` / `function_call_output` / `reasoning` output items → matching Chat roles and tool_calls
- Namespace tool names ↔ Chat function names (`NamespaceToolMap`)
- Replay `reasoning_content` from `SessionStore` (by call_id or turn fingerprint)

**Outbound (Chat → Responses)**:
- Assistant message → message / function_call items in `output`
- `reasoning_content` → reasoning summary item
- Usage field mapping

**Model mapping** (`map_model_name`):
- Read `CRABRIDGE_MODEL_MAP` env var, format `source:target,source2:target2`
- Model names containing `deepseek` pass through unchanged
- Other Codex model names (e.g. `gpt-5.4`) fall back to `DEEPSEEK_MODEL`

### 4.5 `src/stream.rs`

**Role**: Translate DeepSeek Chat Completions SSE into Responses API SSE events.

**Output event types** (partial):
- `response.output_item.added` / `response.output_item.done`
- `response.output_text.delta`
- `response.reasoning_summary_text.delta`
- `response.function_call_arguments.delta`
- `response.completed`

After the stream ends, persist the turn to `SessionStore` (including reasoning and tool_calls).

### 4.6 `src/session.rs` + `src/session_sqlite.rs`

**Role**: Maintain multi-turn conversation state for Codex `previous_response_id` continuity.

**In-memory indexes**:
- `response_id → messages[]` — full conversation history
- `call_id → reasoning_content` — reasoning tied to tool calls
- `fingerprint → reasoning_content` — reasoning replay for text-only assistant turns

**Limits and cleanup**:
- `MAX_SESSIONS` (default 256), `DEFAULT_MAX_SESSION_BYTES` (512 MB), TTL (default 7 days)
- Background hourly `cleanup()` evicts expired entries

**SQLite persistence** (`session_sqlite.rs`):
- Default path `data/crabbridge.db` (WAL mode, auto-creates parent directory)
- Tables: `sessions`, `reasoning`, `turn_reasoning`
- On startup, `load_sqlite_index()` restores in-memory indexes; writes use upsert
- `SESSION_MEMORY_ONLY=true` skips SQLite (memory only)

**Constructors**:
```rust
SessionStore::with_limits_and_ttl(max, bytes, ttl)                // memory only
SessionStore::with_sqlite_limits_and_ttl(path, max, bytes, ttl) // memory + SQLite
```

### 4.7 `src/types.rs`

Self-contained Responses API and Chat Completions request/response types (no external OpenAI crate).

Key types: `ResponsesRequest`, `ResponsesInput`, `ResponsesResponse`, `ChatRequest`, `ChatMessage`, `ChatResponse`, `ChatStreamChunk`, etc.

### 4.8 `src/cache.rs`

Non-streaming response cache (moka), keyed by hash of upstream request body. Controlled by `CACHE_ENABLED`.

### 4.9 `src/upstream_request.rs`

Builds JSON body sent to DeepSeek; supports `CRABRIDGE_TOOL_DENYLIST` to filter specific tools.

### 4.10 `src/codex_config.rs`

`print-codex-config` subcommand: queries upstream model list and prints a Codex `config.toml` snippet (`wire_api = "responses"`, `base_url = "http://127.0.0.1:11435/v1"`, etc.).

### 4.11 `src/prompt.rs`

`ResponsesSseParser`: parses `response.output_text.delta` events from Responses SSE for `prompt --stream` text output.

### 4.12 `src/main.rs`

**`run_serve` flow**:
1. Initialize tracing
2. Validate upstream URL
3. Create `SessionStore` (SQLite or memory-only)
4. Optionally create `ResponseCache`
5. Build Axum router with CORS and optional rate limit (`tower_governor`)
6. Start background session cleanup task
7. `axum::serve` on `bind_addr`

---

## 5. API Endpoints

### 5.1 `POST /v1/responses`

**Request**: OpenAI Responses API format (as sent by Codex CLI).

```json
{
  "model": "gpt-5.4",
  "input": "Hello",
  "stream": false,
  "previous_response_id": "resp_xxx"
}
```

**Response**:
- Non-streaming: Responses JSON (`object: "response"`, `output: [...]`)
- Streaming: Responses SSE event stream

**Errors**: JSON `{ "error": { "message": "...", "type": "..." } }`

### 5.2 `GET /v1/models`

Proxies DeepSeek `/v1/models` and returns the model list.

### 5.3 `GET /health`

```json
{ "status": "ok" }
```

### 5.4 Unimplemented endpoints

`/v1/chat/completions` **does not exist**. Unknown paths return 404.

---

## 6. Configuration & Environment Variables

### 6.1 Required

| Variable | Description |
|----------|-------------|
| `DEEPSEEK_API_KEY` | DeepSeek API key |

### 6.2 Core

| Variable | Description | Default |
|----------|-------------|---------|
| `DEEPSEEK_BASE_URL` | Upstream API base URL | `https://api.deepseek.com/v1` |
| `DEEPSEEK_MODEL` | Default DeepSeek model | `deepseek-chat` |
| `BRIDGE_ADDR` | Bind address | `127.0.0.1:11435` |
| `MAX_TOKENS` | Default max_tokens | none |
| `TEMPERATURE` | Default temperature | none |
| `LOG_LEVEL` | Log level | `info` |

### 6.3 Session storage

| Variable | Description | Default |
|----------|-------------|---------|
| `MAX_SESSIONS` | Max session count | `256` |
| `SESSION_TTL_HOURS` | Session TTL (hours) | `168` (7 days) |
| `SESSION_DB` | SQLite database path | `data/crabbridge.db` |
| `SESSION_MEMORY_ONLY` | Disable SQLite, memory only | `false` |

### 6.4 Optional features

| Variable | Description | Default |
|----------|-------------|---------|
| `CRABRIDGE_MODEL_MAP` | Codex→DeepSeek model mapping | none |
| `CRABRIDGE_TOOL_DENYLIST` | Tool names to block (comma-separated) | none |
| `CACHE_ENABLED` | Enable response cache | `false` |
| `CACHE_TTL_SECS` | Cache TTL (seconds) | `300` |
| `CACHE_MAX_ENTRIES` | Max cache entries | `1000` |
| `RATE_LIMIT_RPS` | Global rate limit (RPS; 0 = disabled) | `0` |

---

## 7. CLI

```bash
crabridge [COMMAND] [OPTIONS]
```

### 7.1 `serve`

```bash
crabridge serve --api-key sk-xxx --model deepseek-chat -b 127.0.0.1:11435
SESSION_DB=/tmp/crab.db crabridge serve
SESSION_MEMORY_ONLY=true crabridge serve   # memory only, no persistence
```

### 7.2 `prompt`

```bash
crabridge prompt "Hello"
crabridge prompt "Hello" --stream
```

Sends a test request to `http://{BRIDGE_ADDR}/v1/responses`.

### 7.3 `print-codex-config`

```bash
crabridge print-codex-config --api-key sk-xxx
```

Prints a config snippet ready to paste into Codex `config.toml`.

---

## 8. Codex Integration

Example Codex CLI config (generated by `print-codex-config`):

```toml
[model_providers.crabbridge]
name = "crabbridge"
base_url = "http://127.0.0.1:11435/v1"
wire_api = "responses"
env_key = "DEEPSEEK_API_KEY"
```

Startup order:
1. Run `crabridge serve` (ensure API key is configured)
2. Use the `crabbridge` provider in Codex

---

## 9. Build & Run

```bash
# Build
cargo build --release

# Run (reads .env)
cargo run -- serve

# Test
cargo test
cargo clippy --all-targets -- -D warnings
```

---

## 10. Testing Strategy

### 10.1 Unit tests

In-module `#[cfg(test)]` blocks covering:
- Responses ↔ Chat conversion (`translate.rs`)
- Session store and SQLite round-trip (`session.rs`, `session_sqlite.rs`)
- SSE parsing (`prompt.rs`)
- Model mapping, tool name splitting, namespace mapping

### 10.2 Integration tests (`tests/integration.rs`)

Uses mockito to mock the DeepSeek upstream:
- `GET /health`
- `POST /v1/responses` non-streaming translation
- `POST /v1/responses` streaming SSE translation
- `GET /v1/models` proxy

Integration tests use in-memory `SessionStore` only (no SQLite file dependency).

---

## 11. Design Constraints & Notes

- **Protocol**: Responses API inbound only; outbound is always Chat Completions. No Chat Completions passthrough.
- **Security**: API keys via env or CLI only; never hardcode.
- **Performance**: Async I/O; Reqwest connection pooling; SQLite WAL mode.
- **Error handling**: Upstream errors forwarded or wrapped as Responses-style JSON; avoid panics.
- **CORS**: `CorsLayer::permissive()`.
- **Data directory**: `data/` is listed in `.gitignore`.

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
dotenv = "0.15"
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

**End of Agent Spec**. Use this document to understand CrabBridge's Responses-only architecture and maintain or extend the codebase correctly.
