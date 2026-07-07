#!/usr/bin/env bash
# Shared install logic for macOS and Linux.
set -euo pipefail

OS_NAME="${1:?OS name required (macos|linux)}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BINARY_NAME="crabridge"
CLI_BINARY_NAME="crabridge-cli"

PREFIX="${PREFIX:-${HOME}/.local}"
BIN_DIR="${PREFIX}/bin"
CONFIG_DIR="${CONFIG_DIR:-${HOME}/.config/crabbridge}"
BUILD_DIR="${BUILD_DIR:-${REPO_ROOT}}"
SKIP_BUILD="${SKIP_BUILD:-0}"

usage() {
    cat <<EOF
Usage: install-${OS_NAME}.sh [OPTIONS]

Build and install CrabBridge.

Options:
  --prefix DIR       Install prefix (default: ~/.local)
  --config-dir DIR   Config directory (default: ~/.config/crabbridge)
  --build-dir DIR    Source directory with Cargo.toml (default: repo root)
  --skip-build       Skip cargo build (install existing release binary)
  -h, --help         Show this help

Environment:
  PREFIX, CONFIG_DIR, BUILD_DIR, SKIP_BUILD
  DEEPSEEK_API_KEY   If set, written into the generated config.toml

Examples:
  ./scripts/install-${OS_NAME}.sh
  PREFIX=/usr/local ./scripts/install-${OS_NAME}.sh
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix)
            PREFIX="$2"
            BIN_DIR="${PREFIX}/bin"
            shift 2
            ;;
        --config-dir)
            CONFIG_DIR="$2"
            shift 2
            ;;
        --build-dir)
            BUILD_DIR="$2"
            shift 2
            ;;
        --skip-build)
            SKIP_BUILD=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

log() {
    printf '==> %s\n' "$*"
}

warn() {
    printf 'warning: %s\n' "$*" >&2
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

ensure_cargo() {
    if command -v cargo >/dev/null 2>&1; then
        return
    fi

    if [[ "${OS_NAME}" == "macos" ]]; then
        die "Rust toolchain not found. Install with: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    fi

    die "Rust toolchain not found. Install with: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
}

build_binary() {
    [[ -f "${BUILD_DIR}/Cargo.toml" ]] || die "No Cargo.toml in BUILD_DIR=${BUILD_DIR}"

    log "Building release binaries in ${BUILD_DIR}"
    (
        cd "${BUILD_DIR}"
        cargo build --release --bin "${BINARY_NAME}"
        cargo build --release --bin "${CLI_BINARY_NAME}"
    )
}

install_binary() {
    local src="${BUILD_DIR}/target/release/${BINARY_NAME}"
    local cli_src="${BUILD_DIR}/target/release/${CLI_BINARY_NAME}"
    [[ -f "${src}" ]] || die "Binary not found at ${src}. Run build first."
    [[ -f "${cli_src}" ]] || die "Binary not found at ${cli_src}. Run build first."

    mkdir -p "${BIN_DIR}"
    install -m 755 "${src}" "${BIN_DIR}/${BINARY_NAME}"
    install -m 755 "${cli_src}" "${BIN_DIR}/${CLI_BINARY_NAME}"
    log "Installed ${BIN_DIR}/${BINARY_NAME}"
    log "Installed ${BIN_DIR}/${CLI_BINARY_NAME}"
}

install_config() {
    mkdir -p "${CONFIG_DIR}"
    mkdir -p "${CONFIG_DIR}/data"

    local config_file="${CONFIG_DIR}/config.toml"
    if [[ -f "${config_file}" ]]; then
        warn "Config already exists: ${config_file} (unchanged)"
        return
    fi

    if [[ -f "${REPO_ROOT}/crabbridge.example.toml" ]]; then
        cp "${REPO_ROOT}/crabbridge.example.toml" "${config_file}"
    else
        cat >"${config_file}" <<'EOF'
default_provider = "deepseek"

[providers.deepseek]
api_key = "sk-your-api-key-here"
base_url = "https://api.deepseek.com/v1"
model = "deepseek-v4-pro"

[providers.kimi]
api_key = "sk-your-kimi-code-key"
base_url = "https://api.kimi.com/coding/v1"
model = "kimi-for-coding"

[server]
bind_addr = "127.0.0.1:11435"
log_level = "info"

[session]
db = "data/crabbridge.db"
memory_only = false
EOF
    fi

    if [[ -n "${DEEPSEEK_API_KEY:-}" ]]; then
        if grep -q '^api_key = ' "${config_file}"; then
            sed -i.bak "s|^api_key = .*|api_key = \"${DEEPSEEK_API_KEY}\"|" "${config_file}"
            rm -f "${config_file}.bak"
        fi
    fi

    log "Created config: ${config_file}"
}

path_hint() {
    if [[ ":${PATH}:" == *":${BIN_DIR}:"* ]]; then
        return
    fi

    cat <<EOF

Add CrabBridge to your PATH (if not already):

  export PATH="${BIN_DIR}:\$PATH"

Persist in your shell profile (~/.bashrc, ~/.zshrc, etc.):

  echo 'export PATH="${BIN_DIR}:\$PATH"' >> ~/.bashrc
EOF
}

print_next_steps() {
    cat <<EOF

CrabBridge installed successfully.

  Binary:  ${BIN_DIR}/${BINARY_NAME}
           ${BIN_DIR}/${CLI_BINARY_NAME}
  Config:  ${CONFIG_DIR}/config.toml

Next steps:
  1. Edit ${CONFIG_DIR}/config.toml and set upstream.api_key
  2. Start the bridge:
       ${BIN_DIR}/${BINARY_NAME} serve
  3. Configure Codex:
       ${BIN_DIR}/${CLI_BINARY_NAME} setup
  4. Test:
       ${BIN_DIR}/${BINARY_NAME} prompt "Hello"
EOF
    path_hint
}

main() {
    log "Installing CrabBridge on ${OS_NAME}"
    ensure_cargo

    if [[ "${SKIP_BUILD}" != "1" ]]; then
        build_binary
    fi

    install_binary
    install_config
    print_next_steps
}

main "$@"
