# 🦀 CrabBridge

A lightweight Rust proxy that lets **Codex CLI** talk to **DeepSeek** or **Kimi Code** via the OpenAI Responses API.

CrabBridge accepts Responses API requests from Codex, converts them to upstream Chat Completions, and translates responses (including streaming SSE) back to the Responses format. Multi-turn conversations are persisted in SQLite so sessions survive restarts.

```
Codex CLI  ──Responses API──▶  CrabBridge  ──Chat Completions──▶  DeepSeek / Kimi Code
         /{provider}/v1/responses              /v1/chat/completions
              Authorization: Bearer <key from Codex env_key>
```

One `crabridge serve` process can host **multiple upstream providers** at once. Codex selects the upstream via `base_url` path:

- `http://127.0.0.1:11435/deepseek/v1`
- `http://127.0.0.1:11435/kimi/v1`

## Features

- **Responses-only bridge** — built for Codex (`wire_api = "responses"`), not a Chat Completions passthrough
- **Built-in providers** — DeepSeek and Kimi work out of the box; no config file required to start `serve`
- **Protocol translation** — tool calls, reasoning content, namespace tools, provider-aware model mapping
- **Streaming** — real-time Chat SSE → Responses SSE conversion
- **Session persistence** — SQLite-backed history keyed by `response_id` for `previous_response_id` continuity
- **Optional cache & rate limiting** — moka response cache, global RPS limit
- **Desktop tray app** — Tauri 2 GUI with optional Setup Wizard, Settings, and embedded bridge lifecycle

## Requirements

- [Rust](https://rustup.rs/) 1.75+ (for building from source)
- API keys in your **shell** for Codex (`DEEPSEEK_API_KEY`, `KIMI_API_KEY`) — CrabBridge forwards the Bearer token from each Codex request to the matching upstream; keys are **not** stored in `crabbridge.toml`

## Quick Start

### Zero-config server

```bash
export DEEPSEEK_API_KEY=sk-...
export KIMI_API_KEY=sk-...    # optional, for Kimi routes

cargo run --bin crabridge -- serve
```

`serve` listens on `127.0.0.1:11435` with built-in **deepseek** and **kimi** routes. Press **Ctrl-C** to stop.

### Desktop app (recommended for Codex)

```bash
cargo run --bin crabbridge-desktop
```

On first launch the embedded bridge starts automatically. Open **Setup Wizard** only if you need to switch Codex provider, override a base URL, or store API keys in the keychain — then click **Set as Codex Provider**.

```bash
./scripts/build-desktop.sh   # release installers → dist/desktop/
```

### Optional `crabbridge.toml`

Copy `crabbridge.example.toml` when you want to customize bind address, model maps, cache, or per-provider base URLs:

```bash
cp crabbridge.example.toml ~/.config/crabbridge/config.toml
crabridge serve --config ~/.config/crabbridge/config.toml
```

A config with only `[providers.deepseek]` enables DeepSeek alone; empty `[providers.*]` sections still enable both built-in routes.

### Test from another terminal

```bash
cargo run --bin crabridge -- prompt "Hello"
cargo run --bin crabridge -- prompt "Hello" --provider kimi
```

Use **CrabBridge desktop → Check Configuration** (tray menu) to validate Codex + bridge setup.

## Installation

Install scripts build a release `crabridge` binary and create a starter config directory.

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
./scripts/install-macos.sh
PREFIX=/usr/local ./scripts/install-linux.sh

export DEEPSEEK_API_KEY=sk-...
crabridge serve
```

```powershell
# Windows
$env:DEEPSEEK_API_KEY = "sk-..."
.\scripts\install-windows.ps1

# Desktop release build (from repo root)
cd crates\crabbridge-desktop; cargo tauri build
```

Install scripts copy `crabbridge.example.toml` when present. API keys belong in your shell environment, not in the generated TOML.

## Codex Integration

1. Export keys in the terminal where you run Codex:

   ```bash
   export DEEPSEEK_API_KEY=sk-...
   export KIMI_API_KEY=sk-...
   ```

2. Start CrabBridge (`crabridge serve` or the desktop app).

3. Point Codex at the bridge — desktop **Setup Wizard → Set as Codex Provider** writes `~/.codex/config.toml` and model catalogs automatically. Or paste manually:

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

   Switch providers in Codex by changing `model_provider` and `model`.

## Configuration

### Config file search order

1. `--config PATH` / `-c PATH` or `CRABRIDGE_CONFIG`
2. `./crabbridge.toml`
3. `~/.config/crabbridge/config.toml` (Windows: `%APPDATA%\crabbridge\config.toml`)

**Priority:** CLI flags > environment variables > TOML file > built-in defaults.

### What goes in `crabbridge.toml`

Bridge TOML configures **routes and server settings**, not upstream API keys:

| Section | Purpose |
|---------|---------|
| `[providers.{slug}]` | Enable a provider route; optional `base_url`, `model_map` |
| `[server]` | `bind_addr`, `log_level`, … |
| `[session]` | SQLite path, TTL, memory-only mode |
| `[cache]` / `[rate_limit]` / `[advanced]` | Optional features |
| `[admin]` | `enabled = true` — local dashboard at `/admin` and Prometheus at `/metrics` |

```toml
[providers.deepseek]
# base_url = "https://api.deepseek.com/v1"

[providers.kimi]
# base_url = "https://api.kimi.com/coding/v1"

[server]
bind_addr = "127.0.0.1:11435"
```

Open `http://127.0.0.1:11435/admin` while `crabridge serve` is running.

Upstream URLs default from the provider slug. Override with `base_url` or `CRABRIDGE_{SLUG}_BASE_URL`.

**Model mapping:** per-route `[providers.*.model_map]` or global `[advanced].model_map`. Unmapped names fall back to provider defaults (`deepseek-v4-pro`, `kimi-for-coding`).

**Sessions:** History is keyed by `response_id`. The `provider` column in SQLite is metadata; Codex can switch `model_provider` mid-session and still resume via `previous_response_id`.

### Useful environment variables

| Variable | Description |
|----------|-------------|
| `DEEPSEEK_API_KEY` | DeepSeek key — set in shell for Codex `env_key`; forwarded as Bearer token |
| `KIMI_API_KEY` | Kimi Code key — same pattern |
| `CRABRIDGE_CONFIG` | Path to `crabbridge.toml` |
| `CRABRIDGE_{SLUG}_BASE_URL` | Override upstream base URL for a route |
| `CRABRIDGE_DEFAULT_PROVIDER` | Preferred provider slug in config metadata |
| `BRIDGE_ADDR` | Server listen address |
| `SESSION_DB` | SQLite database path |
| `CRABRIDGE_MODEL_MAP` | Global model map |
| `CRABRIDGE_TOOL_DENYLIST` | Comma-separated tools to block |

## CLI

| Binary | Purpose |
|--------|---------|
| `crabridge` | HTTP bridge server (`serve`, `prompt`) |
| `crabbridge-desktop` | Tray app — Setup Wizard, Settings, embedded bridge |

Codex setup lives in the **desktop app** (`setup.rs`, `codex_config.rs`); there is no separate setup CLI crate.

```bash
# Server
crabridge serve
crabridge serve --config crabbridge.toml
crabridge prompt "Hello"
crabridge prompt "Hello" --provider kimi

# Desktop
cargo run --bin crabbridge-desktop
./scripts/build-desktop.sh
```

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/admin` | Local admin dashboard (HTML) |
| `GET` | `/admin/api/overview` | Dashboard JSON snapshot |
| `GET` | `/metrics` | Prometheus metrics |
| `GET` | `/{provider}/v1/models` | Proxy upstream model list |
| `POST` | `/{provider}/v1/responses` | Responses API (Codex entry point) |

All upstream-bound requests require `Authorization: Bearer <api_key>`. `/v1/chat/completions` is **not** exposed on the bridge.

## Desktop app (Tauri 2 tray)

Cross-platform **menu-bar / system-tray** app that embeds the HTTP bridge.

| Window | Purpose |
|--------|---------|
| **Welcome** | Home (bridge controls, current provider) + optional Setup Wizard |
| **Settings** | Appearance, autostart, logs |

On first launch the bridge auto-starts with built-in DeepSeek and Kimi — no config file required. Open Codex in your usual terminal (where API keys are exported). Default listen address: `http://127.0.0.1:11435`.

### Tray menu

| Item | Action |
|------|--------|
| Start / Stop Bridge | Control the embedded HTTP server |
| Open Admin Dashboard | `http://127.0.0.1:11435/admin` |
| Welcome… | Home / Setup Wizard |
| Run Codex Setup | Re-run Codex + bridge config writer |
| Check Configuration | Validate Codex + bridge config |
| Settings… | Appearance, autostart, logs |

### Release bundle

```bash
./scripts/build-desktop.sh
```

Runs `cargo tauri build` and copies installers to `dist/desktop/`:

| Platform | Artifacts |
|----------|-----------|
| macOS | `crabbridge-desktop-macos.dmg`, `CrabBridge.app` |
| Linux | `crabbridge-desktop-linux.AppImage`, `.deb` |
| Windows | `crabbridge-desktop-windows.msi`, setup `.exe` |

Requires [tauri-cli](https://v2.tauri.app/) (`cargo install tauri-cli --locked`).

## Development

```bash
cargo build --workspace --release
cargo run --bin crabridge -- serve
cargo run --bin crabbridge-desktop

cargo test --workspace
cargo clippy --workspace -- -D warnings
```

Cross-compile server binaries: `./build-release.sh` → `dist/`.

For architecture details, see [AGENT_SPEC.md](AGENT_SPEC.md).

## Project Layout

```
crab-bridge-rs/
├── Cargo.toml
├── crabbridge.example.toml
├── build-release.sh              # cross-compile crabridge → dist/
├── crates/
│   ├── crabbridge-core/          # types, config, provider, runtime
│   ├── crabbridge-server/        # crabridge binary (HTTP bridge)
│   └── crabbridge-desktop/       # crabbridge-desktop (Tauri tray app)
└── scripts/
    ├── build-desktop.sh
    ├── install-macos.sh / install-linux.sh / install-unix.sh
    └── install-windows.ps1
```
