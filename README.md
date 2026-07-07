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

Legacy `/v1/*` routes still work and map to `default_provider`.

## Features

- **Responses-only bridge** — built for Codex (`wire_api = "responses"`), not a Chat Completions passthrough
- **Protocol translation** — tool calls, reasoning content, namespace tools, provider-aware model mapping
- **Streaming** — real-time Chat SSE → Responses SSE conversion
- **Session persistence** — SQLite-backed history keyed by `response_id` for `previous_response_id` continuity
- **Optional cache & rate limiting** — moka response cache, global RPS limit
- **Codex config generator** — `crabridge-cli setup` / `print-codex-config` output ready-to-paste Codex snippets

## Requirements

- [Rust](https://rustup.rs/) 1.75+ (for building from source)
- API keys in your **shell** for Codex (`DEEPSEEK_API_KEY`, `KIMI_API_KEY`) — CrabBridge forwards the Bearer token from each Codex request to the matching upstream; keys are **not** stored in `crabbridge.toml`

## Quick Start

### DeepSeek (default)

```bash
export DEEPSEEK_API_KEY=sk-...
cp crabbridge.example.toml crabbridge.toml
cargo run --bin crabridge -- serve
```

### Multi-provider (recommended)

```bash
export DEEPSEEK_API_KEY=sk-...
export KIMI_API_KEY=sk-...
cargo run --bin crabridge-cli -- setup --all-providers   # writes Codex config + crabbridge.toml with both routes
cargo run --bin crabridge -- serve
```

`setup --all-providers` writes a TOML with both `[providers.deepseek]` and `[providers.kimi]` sections. If you hand-edit `crabbridge.toml`, include a section for **each** provider you want enabled — a file with only `[providers.deepseek]` serves DeepSeek alone.

With **no** config file at all, `serve` defaults to both built-in providers (`deepseek` + `kimi`).

### Single provider

```bash
export KIMI_API_KEY=sk-...
cargo run --bin crabridge-cli -- setup --provider kimi
cargo run --bin crabridge -- serve
```

In another terminal:

```bash
cargo run --bin crabridge -- prompt "Hello"
cargo run --bin crabridge -- prompt "Hello" --provider kimi
cargo run --bin crabridge-cli -- setup --docker   # check configuration
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
| macOS / Linux | `~/.local/bin/crabridge`, `~/.local/bin/crabridge-cli` | `~/.config/crabbridge/config.toml` |
| Windows | `%LOCALAPPDATA%\crabbridge\bin\crabridge.exe`, `crabridge-cli.exe` | `%APPDATA%\crabbridge\config.toml` |

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

1. Set upstream keys in your shell (Codex reads these via `env_key`):

   ```bash
   export DEEPSEEK_API_KEY=sk-...
   export KIMI_API_KEY=sk-...
   ```

2. Start CrabBridge:

   ```bash
   crabbridge serve
   ```

3. Generate Codex provider snippets (writes `~/.codex/crabbridge-models-{provider}.json`):

   ```bash
   crabridge-cli setup --all-providers
   # or one at a time
   crabridge-cli print-codex-config --provider deepseek
   crabridge-cli print-codex-config --provider kimi
   ```

4. Paste into `~/.codex/config.toml`. Multi-provider form:

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

```bash
crabridge serve --config ~/.config/crabbridge/config.toml
crabridge-cli -c crabbridge.toml setup --all-providers
```

The config file is loaded **before** the CLI subcommand runs (`explicit_config_before_cli`). Both `crabridge` and `crabridge-cli` share the same config resolution and global `-c` / `--config` flag.

**Priority:** CLI flags > environment variables > TOML file > built-in defaults.

### What goes in `crabbridge.toml`

Bridge TOML configures **routes and server settings**, not upstream API keys:

| Section | Purpose |
|---------|---------|
| `default_provider` | Legacy `/v1/*` fallback route |
| `[providers.{slug}]` | Enable a provider route; optional `base_url`, `model_map` |
| `[server]` | `bind_addr`, `log_level`, … |
| `[session]` | SQLite path, TTL, memory-only mode |
| `[cache]` / `[rate_limit]` / `[advanced]` | Optional features |
| `[admin]` | `enabled = true` — local dashboard at `/admin` and Prometheus at `/metrics` |

```toml
default_provider = "deepseek"

[providers.deepseek]
# base_url = "https://api.deepseek.com/v1"
# model_map = "gpt-5.4:deepseek-v4-pro"

[providers.kimi]
# base_url = "https://api.kimi.com/coding/v1"
# model_map = "gpt-5.4:kimi-for-coding"

[server]
bind_addr = "127.0.0.1:11435"
```

Open `http://127.0.0.1:11435/admin` for the local dashboard while `crabridge serve` is running.

Upstream URLs default from the provider slug (`deepseek` → DeepSeek API, `kimi` → Kimi Code API). Override with `base_url` or `CRABRIDGE_{SLUG}_BASE_URL`.

**Model mapping:** Codex model names are mapped per route via `[providers.*.model_map]` or global `[advanced].model_map`. Unmapped names fall back to the provider preset default (`deepseek-v4-pro`, `kimi-for-coding`). Upstream model IDs pass through only when they match the active provider (e.g. `deepseek-v4-pro` on `/kimi/v1` becomes `kimi-for-coding`).

**Sessions:** History is keyed by `response_id` only. The `provider` column in SQLite is metadata (updated on write); Codex can switch `model_provider` mid-session and still resume via `previous_response_id`.

### Useful environment variables

| Variable | Description |
|----------|-------------|
| `DEEPSEEK_API_KEY` | DeepSeek key — set in shell for Codex `env_key`; forwarded as Bearer token |
| `KIMI_API_KEY` | Kimi Code key — same pattern |
| `CRABRIDGE_CONFIG` | Path to `crabbridge.toml` |
| `CRABRIDGE_{SLUG}_BASE_URL` | Override upstream base URL for a route |
| `CRABRIDGE_DEFAULT_PROVIDER` | Legacy `/v1/*` default slug |
| `BRIDGE_ADDR` | Server listen address |
| `SESSION_DB` | SQLite database path |
| `CRABRIDGE_MODEL_MAP` | Global model map |
| `CRABRIDGE_TOOL_DENYLIST` | Comma-separated tools to block |

## CLI

Two binaries are built from this repo:

| Binary | Purpose |
|--------|---------|
| `crabridge` | Run the HTTP bridge server and send test prompts |
| `crabridge-cli` | Generate Codex config snippets and write setup files |

```bash
# Server (crabridge)
crabridge serve                                # Start the bridge server
crabridge serve --config crabbridge.toml       # Explicit config path
crabridge prompt "Hello"                       # Send a test request (uses env key)
crabridge prompt "Hello" --provider kimi

# Setup & Codex snippets (crabridge-cli)
crabridge-cli setup                            # Write Codex + crabbridge.toml
crabridge-cli setup --all-providers            # Configure deepseek + kimi at once
crabridge-cli setup --providers=kimi,deepseek  # Pick providers explicitly
crabridge-cli setup --docker                   # Check current configuration
crabridge-cli print-codex-config --all-providers
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
| `GET/POST` | `/v1/*` | Legacy routes → `default_provider` |

All upstream-bound requests require `Authorization: Bearer <api_key>`. `/v1/chat/completions` is **not** exposed on the bridge.

## Desktop MVP (macOS tray app)

The desktop app wraps the HTTP bridge in a menu-bar tray icon. On first launch it opens a **Quick Setup** wizard; after that it auto-starts the bridge when you log in (optional).

### Run from source

```bash
cargo run --bin crabbridge-desktop
```

### First-run flow

1. **Provider Configuration** — select DeepSeek or Kimi, set Base URL, verify API key (masked from shell env when available), save.
2. **Run Setup** — writes `~/.codex/config.toml`, model catalogs, and `~/.config/crabbridge/config.toml`.
3. **Start Bridge & Finish** — starts the embedded bridge and marks onboarding complete.

Open Codex in your **usual terminal** (where API keys are already exported). The bridge listens at `http://127.0.0.1:11435` by default.

### Tray menu

| Item | Action |
|------|--------|
| Start / Stop Bridge | Control the embedded HTTP server |
| Open Admin Dashboard | Opens `http://127.0.0.1:11435/admin` |
| Quick Setup… | Re-open the onboarding wizard |
| Run Codex Setup | Re-run CLI setup (config refresh) |
| Check Configuration | Validates Codex + bridge config |
| Settings… | API keys, autostart, logs, bridge controls |

### Release bundle

```bash
./scripts/build-desktop.sh   # .dmg (macOS), .AppImage (Linux), .msi (Windows)
```

## Development

```bash
cargo build --workspace --release          # both binaries
cargo build --release --bin crabridge      # HTTP bridge only
cargo build --release --bin crabridge-cli    # slim CLI (no axum/sqlite/moka)
cargo run --bin crabbridge-desktop         # desktop tray app (Tauri)

# Desktop release bundle (.dmg / .AppImage / .msi — requires tauri-cli)
./scripts/build-desktop.sh

cargo test --workspace
cargo clippy --workspace -- -D warnings
```

For architecture details and module design, see [AGENT_SPEC.md](AGENT_SPEC.md).

## Project Layout

```
crab-bridge-rs/
├── Cargo.toml                    # workspace root
├── crates/
│   ├── crabbridge-core/          # shared types, config, provider, runtime
│   │   └── src/
│   │       ├── types.rs          # Responses + Chat Completions types
│   │       ├── provider.rs       # DeepSeek / Kimi presets
│   │       ├── config.rs         # TOML load + provider resolution
│   │       └── runtime.rs        # shared init + Tokio block_on
│   ├── crabbridge-cli/           # Codex setup binary (crabridge-cli)
│   │   └── src/
│   │       ├── main.rs           # thin entry
│   │       ├── cli.rs            # setup / print-codex-config handlers
│   │       ├── cli_opts.rs       # Clap for crabridge-cli
│   │       ├── codex_config.rs   # Codex config snippet generator
│   │       └── setup.rs          # setup + setup --docker
│   └── crabbridge-server/        # HTTP bridge binary (crabridge)
│       ├── static/admin.html     # embedded admin dashboard
│       ├── tests/integration.rs  # mockito integration tests
│       └── src/
│           ├── main.rs           # thin entry
│           ├── serve.rs          # library API: start_serve / ServeHandle
│           ├── server.rs         # serve / prompt handlers
│           ├── opts.rs           # Clap for crabridge
│           ├── app.rs            # Router construction
│           ├── admin.rs          # /admin dashboard + /metrics
│           ├── metrics.rs        # Runtime counters + Prometheus export
│           ├── handlers.rs       # HTTP routes
│           ├── translate.rs      # Responses ↔ Chat conversion
│           ├── stream.rs         # Streaming SSE translation
│           ├── session.rs        # Session store
│           ├── session_sqlite.rs # SQLite persistence
│           └── ...
│   └── crabbridge-desktop/       # desktop tray app (crabbridge-desktop)
│       ├── tauri.conf.json
│       ├── icons/
│       ├── static/settings.html  # settings window (API keys, autostart, checks)
│       ├── static/welcome.html   # first-run onboarding wizard
│       └── src/
│           ├── main.rs           # thin entry
│           ├── lib.rs            # tray + Tauri commands
│           ├── bridge.rs         # embedded server lifecycle
│           ├── onboarding.rs     # first-run wizard orchestration
│           ├── prefs.rs          # desktop-prefs.json
│           ├── env_export.rs     # env.sh + zsh hook
│           ├── secrets.rs        # keychain API key storage
│           ├── autostart.rs      # launch at login
│           ├── health.rs         # configuration checks
│           ├── settings.rs       # settings / welcome windows
│           ├── tray.rs           # system tray menu
│           └── setup_wizard.rs   # Codex setup from desktop
scripts/
├── install-macos.sh
├── install-linux.sh
└── install-windows.ps1
```
