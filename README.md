# 🦀 CrabBridge

A lightweight Rust proxy that lets **Codex CLI** talk to **DeepSeek** via the OpenAI Responses API.

CrabBridge accepts Responses API requests from Codex, converts them to DeepSeek Chat Completions, and translates responses (including streaming SSE) back to the Responses format. Multi-turn conversations are persisted in SQLite so sessions survive restarts.

```
Codex CLI  ──Responses API──▶  CrabBridge  ──Chat Completions──▶  DeepSeek
              /v1/responses              /v1/chat/completions
```

## Features

- **Responses-only bridge** — built for Codex (`wire_api = "responses"`), not a Chat Completions passthrough
- **Protocol translation** — tool calls, reasoning content, namespace tools, model mapping
- **Streaming** — real-time Chat SSE → Responses SSE conversion
- **Session persistence** — SQLite-backed history for `previous_response_id` continuity
- **Optional cache & rate limiting** — moka response cache, global RPS limit
- **Codex config generator** — `print-codex-config` outputs a ready-to-paste `config.toml` snippet

## Requirements

- [Rust](https://rustup.rs/) 1.75+ (for building from source)
- A [DeepSeek API key](https://platform.deepseek.com/)

## Quick Start

```bash
# Clone and configure
git clone <repo-url> crab-bridge-rs && cd crab-bridge-rs
cp .env.example .env
# Edit .env and set DEEPSEEK_API_KEY

# Run
cargo run -- serve
```

In another terminal:

```bash
cargo run -- prompt "Hello"
cargo run -- print-codex-config --api-key $DEEPSEEK_API_KEY
```

## Installation

Install scripts build a release binary and set up a config directory.

| Platform | Command |
|----------|---------|
| macOS | `./scripts/install-macos.sh` |
| Linux | `./scripts/install-linux.sh` |
| Windows | `powershell -ExecutionPolicy Bypass -File scripts/install-windows.ps1` |

**Install locations**

| Platform | Binary | Config |
|----------|--------|--------|
| macOS / Linux | `~/.local/bin/crabridge` | `~/.config/crabbridge/.env` |
| Windows | `%LOCALAPPDATA%\crabbridge\bin\crabridge.exe` | `%APPDATA%\crabbridge\.env` |

**Examples**

```bash
# macOS / Linux
DEEPSEEK_API_KEY=sk-xxx ./scripts/install-macos.sh
PREFIX=/usr/local ./scripts/install-linux.sh

# Start after install
cd ~/.config/crabbridge && crabridge serve
```

```powershell
# Windows
$env:DEEPSEEK_API_KEY = "sk-xxx"
.\scripts\install-windows.ps1
```

## Codex Integration

1. Start CrabBridge:

   ```bash
   crabridge serve
   ```

2. Generate a Codex provider snippet (also writes `~/.codex/crabbridge-models.json`):

   ```bash
   crabridge print-codex-config --api-key $DEEPSEEK_API_KEY --model deepseek-v4-pro
   ```

3. Paste the printed TOML into `~/.codex/config.toml`. Minimal form:

   ```toml
   model_provider = "crabbridge"
   model = "deepseek-v4-pro"
   model_catalog_json = "/Users/YOU/.codex/crabbridge-models.json"

   [model_providers.crabbridge]
   name = "crabbridge"
   base_url = "http://127.0.0.1:11435/v1"
   wire_api = "responses"
   env_key = "DEEPSEEK_API_KEY"
   ```

   Codex **0.105+ ignores `[model_properties.*]`**. Metadata must come from
   `model_catalog_json` (a full model catalog JSON). Without it you will see:
   `Model metadata for ... not found`.

   Note: setting `model_catalog_json` replaces Codex's remote OpenAI model list
   for this config — only models in that file are available.

4. Restart Codex and select the `crabbridge` provider.

## Configuration

Copy `.env.example` to `.env` (or use the path created by the install script). All options can also be passed as CLI flags.

### Required

| Variable | Description |
|----------|-------------|
| `DEEPSEEK_API_KEY` | DeepSeek API key |

### Core

| Variable | Default | Description |
|----------|---------|-------------|
| `DEEPSEEK_BASE_URL` | `https://api.deepseek.com/v1` | Upstream API base URL |
| `DEEPSEEK_MODEL` | `deepseek-chat` | Default DeepSeek model |
| `BRIDGE_ADDR` | `127.0.0.1:11435` | Listen address |
| `LOG_LEVEL` | `info` | Log level |

### Session storage

| Variable | Default | Description |
|----------|---------|-------------|
| `SESSION_DB` | `data/crabbridge.db` | SQLite database path |
| `SESSION_MEMORY_ONLY` | `false` | Disable SQLite, use memory only |
| `MAX_SESSIONS` | `256` | Max concurrent sessions |
| `SESSION_TTL_HOURS` | `168` | Session TTL (7 days) |

### Optional

| Variable | Default | Description |
|----------|---------|-------------|
| `CRABRIDGE_MODEL_MAP` | — | Map Codex models to DeepSeek, e.g. `gpt-5.4:deepseek-chat` |
| `CRABRIDGE_TOOL_DENYLIST` | — | Comma-separated tools to block |
| `CACHE_ENABLED` | `false` | Enable response cache |
| `RATE_LIMIT_RPS` | `0` | Global rate limit (0 = off) |

See `.env.example` for the full list.

## CLI

```bash
crabridge serve                  # Start the bridge server
crabridge prompt "Hello"         # Send a test request
crabridge prompt "Hello" --stream
crabridge print-codex-config     # Print Codex config snippet
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/v1/models` | Proxy DeepSeek model list |
| `POST` | `/v1/responses` | Responses API (Codex entry point) |

`/v1/chat/completions` is **not** exposed on the bridge.

## Development

```bash
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
```

For architecture details and module design, see [AGENT_SPEC.md](AGENT_SPEC.md).

## Project Layout

```
src/
├── handlers.rs       # HTTP routes
├── translate.rs      # Responses ↔ Chat conversion
├── stream.rs         # Streaming SSE translation
├── session.rs        # In-memory session store
├── session_sqlite.rs # SQLite persistence
├── types.rs          # API type definitions
└── ...
scripts/
├── install-macos.sh
├── install-linux.sh
└── install-windows.ps1
```
