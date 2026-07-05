# 🦀 CrabBridge

A lightweight Rust proxy that lets **Codex CLI** talk to **DeepSeek** or **Kimi Code** (`kimi-for-coding`) via the OpenAI Responses API.

CrabBridge accepts Responses API requests from Codex, converts them to upstream Chat Completions, and translates responses (including streaming SSE) back to the Responses format. Multi-turn conversations are persisted in SQLite so sessions survive restarts.

```
Codex CLI  ──Responses API──▶  CrabBridge  ──Chat Completions──▶  DeepSeek / Kimi Code
         /{provider}/v1/responses              /v1/chat/completions
```

One `crabridge serve` process can host **multiple upstream providers** at once. Codex selects the upstream via `base_url` path:

- `http://127.0.0.1:11435/deepseek/v1`
- `http://127.0.0.1:11435/kimi/v1`

Legacy `/v1/*` routes still work and map to `default_provider`.

## Features

- **Responses-only bridge** — built for Codex (`wire_api = "responses"`), not a Chat Completions passthrough
- **Protocol translation** — tool calls, reasoning content, namespace tools, model mapping
- **Streaming** — real-time Chat SSE → Responses SSE conversion
- **Session persistence** — SQLite-backed history for `previous_response_id` continuity
- **Optional cache & rate limiting** — moka response cache, global RPS limit
- **Codex config generator** — `print-codex-config` outputs a ready-to-paste `config.toml` snippet

## Requirements

- [Rust](https://rustup.rs/) 1.75+ (for building from source)
- An upstream API key:
  - [DeepSeek](https://platform.deepseek.com/), or
  - [Kimi Code](https://www.kimi.com/code/docs/en/) (`KIMI_API_KEY`, model `kimi-for-coding`)

## Quick Start

### DeepSeek (default)

```bash
cp crabbridge.example.toml crabbridge.toml
# set DEEPSEEK_API_KEY in your shell, or use [upstream] api_key in TOML
cargo run -- serve
```

### Multi-provider (recommended)

```bash
cp crabbridge.example.toml crabbridge.toml
# fill [providers.deepseek] and [providers.kimi] api_key values
cargo run -- setup --all-providers   # writes Codex + crabbridge.toml
cargo run -- serve
```

### Single provider

```bash
cargo run -- setup --provider kimi --api-key sk-xxx
cargo run -- serve
```

In another terminal:

```bash
cargo run -- prompt "Hello"
cargo run -- setup --docker   # check configuration
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
| macOS / Linux | `~/.local/bin/crabridge` | `~/.config/crabbridge/config.toml` |
| Windows | `%LOCALAPPDATA%\crabbridge\bin\crabridge.exe` | `%APPDATA%\crabbridge\config.toml` |

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

2. Generate Codex provider snippets (writes `~/.codex/crabbridge-models-{provider}.json`):

   ```bash
   # both providers
   crabridge setup --all-providers

   # or one at a time
   crabridge print-codex-config --provider deepseek
   crabridge print-codex-config --provider kimi
   ```

3. Paste into `~/.codex/config.toml`. Multi-provider form:

   ```toml
   model_provider = "crabbridge-deepseek"
   model = "deepseek-v4-pro"
   model_catalog_json = "/Users/YOU/.codex/crabbridge-models-deepseek.json"

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
   model_catalog_json = "/Users/YOU/.codex/crabbridge-models-kimi.json"
   ```

   Switch providers in Codex by changing `model_provider` (and `model`).

## Configuration

CrabBridge reads a TOML config file. Search order:

1. `--config PATH` or `CRABRIDGE_CONFIG`
2. `./crabbridge.toml`
3. `~/.config/crabbridge/config.toml` (Windows: `%APPDATA%\crabbridge\config.toml`)

Priority: **CLI flags > environment variables > TOML file > defaults**.

Copy `crabbridge.example.toml` to `crabbridge.toml`, or run `crabridge setup`.

Priority: **CLI flags > environment variables > TOML file > defaults**.

### Global config path

| Source | Description |
|--------|-------------|
| `--config PATH` | Explicit TOML file path |
| `CRABRIDGE_CONFIG` | Environment variable pointing to a TOML file |
| `./crabbridge.toml` | Default filename in the current directory |
| `~/.config/crabbridge/config.toml` | System default (Windows: `%APPDATA%\crabbridge\config.toml`) |

Config values are loaded **before** CLI parsing, so `env = ...` defaults in CLI flags also honor the TOML file.

### Useful environment variables

| Variable | Description |
|----------|-------------|
| `DEEPSEEK_API_KEY` | DeepSeek upstream key (used by Codex `env_key`) |
| `KIMI_API_KEY` | Kimi Code upstream key (used by Codex `env_key`) |
| `CRABRIDGE_CONFIG` | Path to `crabbridge.toml` |
| `CRABRIDGE_{SLUG}_BASE_URL` | Override base URL for a provider, e.g. `CRABRIDGE_DEEPSEEK_BASE_URL` |
| `CRABRIDGE_{SLUG}_MODEL_MAP` | Per-provider model map |
| `UPSTREAM_BASE_URL` | Global fallback base URL (also set by legacy `[upstream] base_url`) |
| `BRIDGE_ADDR` | Server listen address |
| `SESSION_DB` | SQLite database path |
| `CRABRIDGE_MODEL_MAP` | Global model map |
| `CRABRIDGE_TOOL_DENYLIST` | Comma-separated tools to block |

```toml
default_provider = "deepseek"

[providers.deepseek]
base_url = "https://api.deepseek.com/v1"
model_map = "gpt-5.4:deepseek-v4-pro"

[providers.kimi]
base_url = "https://api.kimi.com/coding/v1"
model_map = "gpt-5.4:kimi-for-coding"

[server]
bind_addr = "127.0.0.1:11435"
```

Provider sections support `base_url` (override upstream endpoint) and `model_map` (Codex model name → upstream model name). The legacy `[upstream]` section (`base_url`, `api_key`, `model`) is still supported and is used as a global fallback.

API keys are resolved from the Codex request header and forwarded to the matching provider; set them as shell environment variables (e.g. `DEEPSEEK_API_KEY`, `KIMI_API_KEY`).

SQLite session/reasoning rows are scoped by `provider` so Kimi and DeepSeek histories do not mix.

## CLI

```bash
crabridge serve                                # Start the bridge server
crabridge serve --config crabbridge.toml       # Use an explicit config file
crabridge setup                                # Write Codex + crabbridge.toml
crabridge setup --docker                       # Check current configuration
crabridge prompt "Hello"                       # Send a test request
crabridge prompt "Hello" --stream
crabridge setup --all-providers                # Configure deepseek + kimi at once
crabridge prompt "Hello" --provider kimi
crabridge print-codex-config --all-providers
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/{provider}/v1/models` | Proxy upstream model list |
| `POST` | `/{provider}/v1/responses` | Responses API (Codex entry point) |
| `GET/POST` | `/v1/*` | Legacy routes → `default_provider` |

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
