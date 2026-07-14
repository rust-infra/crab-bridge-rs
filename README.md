# 🦀 CrabBridge

A lightweight Rust proxy that lets **Codex CLI** talk to **DeepSeek** or **Kimi Code** through the OpenAI Responses API.

CrabBridge accepts Responses API requests from Codex, converts them to upstream Chat Completions (including streaming SSE), and persists multi-turn sessions in SQLite. One process can host multiple providers, selected via the URL path:

```
Codex CLI ──▶ http://127.0.0.1:11435/deepseek/v1
              http://127.0.0.1:11435/kimi/v1
```

API keys are forwarded from each Codex request's `env_key` as a Bearer token — they are **never** stored in `crabbridge.toml`.

## Quick Start

```bash
export DEEPSEEK_API_KEY=sk-...
export KIMI_API_KEY=sk-...        # optional, for Kimi routes

cargo run --bin crabridge -- serve
```

`serve` listens on `127.0.0.1:11435` with built-in **deepseek** and **kimi** routes — no config file required. Press **Ctrl-C** to stop.

**Desktop app** (recommended for Codex): `cargo run --bin crabbridge-desktop`. The bridge auto-starts on launch; open **Setup Wizard → Set as Codex Provider** only to switch provider, override a base URL, or store keys in the keychain.

## Install

| Platform | Command |
|----------|---------|
| macOS | `./scripts/install-macos.sh` |
| Linux | `./scripts/install-linux.sh` |
| Windows | `powershell -ExecutionPolicy Bypass -File scripts/install-windows.ps1` |

Installs the `crabridge` binary and a starter config. Binary goes to `~/.local/bin` (macOS/Linux) or `%LOCALAPPDATA%\crabbridge\bin` (Windows); config to `~/.config/crabbridge/config.toml` (or `%APPDATA%\crabbridge\config.toml`).

## Codex Integration

Export your keys in the terminal where Codex runs, start CrabBridge, then let the desktop **Setup Wizard → Set as Codex Provider** write `~/.codex/config.toml` for you. To configure manually:

```toml
model_provider = "crabbridge-deepseek"
model = "deepseek-v4-pro"

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

Switch providers by changing `model_provider` and `model`.

## Configuration

`crabbridge.toml` is optional and configures **routes and server settings only** (never API keys). Copy `crabbridge.example.toml` to customize:

```toml
[providers.deepseek]
# base_url = "https://api.deepseek.com/v1"

[providers.kimi]

[server]
bind_addr = "127.0.0.1:11435"
```

- Config search order: `--config PATH` / `CRABRIDGE_CONFIG` → `./crabbridge.toml` → `~/.config/crabbridge/config.toml`.
- Priority: CLI flags > env vars > TOML > built-in defaults.
- Each `[providers.{slug}]` enables that route; with no provider sections both built-ins stay enabled.
- Model mapping: `[providers.*.model_map]` or `[advanced].model_map`; defaults are `deepseek-v4-pro` and `kimi-for-coding`.

**Common env vars:** `DEEPSEEK_API_KEY`, `KIMI_API_KEY`, `CRABRIDGE_CONFIG`, `CRABRIDGE_{SLUG}_BASE_URL`, `BRIDGE_ADDR`, `SESSION_DB`.

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/admin` · `/metrics` | Dashboard (HTML) · Prometheus metrics |
| `GET` | `/{provider}/v1/models` | Proxy upstream model list |
| `POST` | `/{provider}/v1/responses` | Responses API (Codex entry point) |

All upstream requests require `Authorization: Bearer <api_key>`. `/v1/chat/completions` is not exposed.

## Build Desktop App

**Local (current platform only):**

```bash
./scripts/build-desktop.sh
```

Generates icons, runs `cargo tauri build`, and stages installers under `dist/desktop/`:

| Platform | Artifacts |
|----------|-----------|
| macOS | `crabbridge-desktop-macos.dmg`, `CrabBridge.app` |
| Linux | `crabbridge-desktop-linux.AppImage`, `.deb` |
| Windows | `crabbridge-desktop-windows.msi`, setup `.exe` |

The script auto-installs [tauri-cli](https://v2.tauri.app/) on first run (`cargo install tauri-cli --locked`).

**CI (all platforms from one Mac):**

Push a version tag or run the workflow manually on GitHub Actions:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Workflow: [`.github/workflows/build-desktop.yml`](.github/workflows/build-desktop.yml)

- Builds on `macos-latest` (Apple Silicon + Intel), `ubuntu-22.04`, and `windows-latest` in parallel
- Creates a **draft** GitHub Release `v0.1.0` with installers attached
- Also uploads workflow artifacts if you want to download without publishing the release

Before the first run, enable **Settings → Actions → General → Workflow permissions → Read and write permissions**.

Release builds sync the version from the git tag via `scripts/sync-version-from-git.sh` (writes `Cargo.toml` + `tauri.conf.json` and sets `CRABBRIDGE_VERSION` for compile-time embedding). Local `crabridge --version`, `/admin`, and the desktop home screen show `crabbridge_core::VERSION` (tag / `git describe`).

## Development

```bash
# Enable versioned git hooks (fmt + clippy before every push)
./scripts/install-githooks.sh

cargo build --workspace --release
cargo run --bin crabridge -- serve
cargo run --bin crabbridge-desktop

cargo test --workspace
cargo clippy --workspace -- -D warnings
```

`pre-push` lives in [`.githooks/pre-push`](.githooks/pre-push). Bypass once with `SKIP_PRE_PUSH=1 git push`.

**Layout:** `crabbridge-core` (types, config, provider, runtime) · `crabbridge-server` (`crabridge` HTTP bridge) · `crabbridge-desktop` (Tauri 2 tray app).

For architecture details, see [AGENT_SPEC.md](AGENT_SPEC.md).
