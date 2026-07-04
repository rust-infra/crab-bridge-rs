# 🦀 CrabBridge

A lightweight Rust proxy that lets **Codex CLI** talk to **DeepSeek** or **Kimi Code** (`kimi-for-coding`) via the OpenAI Responses API.

CrabBridge accepts Responses API requests from Codex, converts them to upstream Chat Completions, and translates responses (including streaming SSE) back to the Responses format. Multi-turn conversations are persisted in SQLite so sessions survive restarts.

```
Codex CLI  ──Responses API──▶  CrabBridge  ──Chat Completions──▶  DeepSeek / Kimi Code
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
- An upstream API key:
  - [DeepSeek](https://platform.deepseek.com/), or
  - [Kimi Code](https://www.kimi.com/code/docs/en/) (`KIMI_API_KEY`, model `kimi-for-coding`)

## Quick Start

### DeepSeek (default)

```bash
cp crabbridge.example.toml crabbridge.toml
# set upstream.api_key = "sk-..."
cargo run -- serve
```

### Kimi Code (`kimi-for-coding`)

Uses the Kimi Code OpenAI-compatible endpoint (membership / coding agents):

```bash
# in crabbridge.toml:
# provider = "kimi"
# [upstream]
# api_key = "sk-xxx"
# base_url = "https://api.kimi.com/coding/v1"
# model = "kimi-for-coding"
cargo run -- serve
```

Or one-shot:

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

2. Generate a Codex provider snippet (also writes `~/.codex/crabbridge-models.json`):

   ```bash
   # DeepSeek
   crabridge print-codex-config --model deepseek-v4-pro

   # Kimi Code
   CRABRIDGE_PROVIDER=kimi crabridge print-codex-config
   ```

3. Paste the printed TOML into `~/.codex/config.toml`. Minimal form:

   ```toml
   model_provider = "crabbridge"
   model = "kimi-for-coding"   # or deepseek-v4-pro
   model_catalog_json = "/Users/YOU/.codex/crabbridge-models.json"

   [model_providers.crabbridge]
   name = "crabbridge"
   base_url = "http://127.0.0.1:11435/v1"
   wire_api = "responses"
   env_key = "KIMI_API_KEY"   # or DEEPSEEK_API_KEY
   ```

   Codex **0.105+ ignores `[model_properties.*]`**. Metadata must come from
   `model_catalog_json` (a full model catalog JSON). Without it you will see:
   `Model metadata for ... not found`.

   Note: setting `model_catalog_json` replaces Codex's remote OpenAI model list
   for this config — only models in that file are available.

4. Restart Codex and select the `crabbridge` provider.

## Configuration

CrabBridge reads a TOML config file. Search order:

1. `--config PATH` or `CRABRIDGE_CONFIG`
2. `./crabbridge.toml`
3. `~/.config/crabbridge/config.toml` (Windows: `%APPDATA%\crabbridge\config.toml`)

Priority: **CLI flags > environment variables > TOML file > defaults**.

Copy `crabbridge.example.toml` to `crabbridge.toml`, or run `crabridge setup`.

```toml
provider = "deepseek"   # or "kimi"

[upstream]
api_key = "sk-your-api-key-here"
# base_url = "https://api.deepseek.com/v1"
# model = "deepseek-v4-pro"

[server]
bind_addr = "127.0.0.1:11435"
log_level = "info"

[session]
db = "data/crabbridge.db"
memory_only = false

[cache]
enabled = false

[rate_limit]
rps = 0

# [advanced]
# model_map = "gpt-5.4:deepseek-v4-pro"
# tool_denylist = "spawn_agent,wait_agent"
```

**Kimi Code defaults** (`provider = "kimi"`):

| Setting | Value |
|---------|-------|
| Base URL | `https://api.kimi.com/coding/v1` |
| Model | `kimi-for-coding` |
| Codex `env_key` | `KIMI_API_KEY` |

Codex still needs `DEEPSEEK_API_KEY` / `KIMI_API_KEY` in the **shell** environment (`env_key` in `~/.codex/config.toml`). That is separate from the bridge TOML.

Environment variables (`UPSTREAM_API_KEY`, `BRIDGE_ADDR`, …) remain supported as overrides.

## CLI

```bash
crabridge serve                  # Start the bridge server
crabridge setup                  # Write Codex + crabbridge.toml
crabridge setup --docker         # Check current configuration
crabridge prompt "Hello"         # Send a test request
crabridge prompt "Hello" --stream
crabridge print-codex-config     # Print Codex config snippet only
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
