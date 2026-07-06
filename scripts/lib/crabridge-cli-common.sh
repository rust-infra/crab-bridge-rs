#!/usr/bin/env bash
# Shared helpers for crabridge-cli setup / docker-check wrapper scripts.

set -euo pipefail

crabridge_script_dir() {
    cd "$(dirname "${BASH_SOURCE[1]}")" && pwd
}

crabridge_repo_root() {
    cd "$(dirname "${BASH_SOURCE[1]}")/.." && pwd
}

crabridge_default_config_dir() {
    if [[ -n "${CONFIG_DIR:-}" ]]; then
        printf '%s\n' "${CONFIG_DIR}"
        return
    fi
    if [[ -n "${XDG_CONFIG_HOME:-}" ]]; then
        printf '%s/crabbridge\n' "${XDG_CONFIG_HOME}"
        return
    fi
    printf '%s/.config/crabbridge\n' "${HOME}"
}

crabridge_default_config_file() {
    printf '%s/config.toml\n' "$(crabridge_default_config_dir)"
}

crabridge_default_bind_addr() {
    printf '%s\n' "${BIND_ADDR:-127.0.0.1:11435}"
}

crabridge_resolve_cli() {
    if [[ -n "${CRABRIDGE_CLI:-}" && -x "${CRABRIDGE_CLI}" ]]; then
        printf '%s\n' "${CRABRIDGE_CLI}"
        return 0
    fi

    local candidate
    for candidate in \
        "$(command -v crabridge-cli 2>/dev/null || true)" \
        "${HOME}/.local/bin/crabridge-cli" \
        "${PREFIX:-${HOME}/.local}/bin/crabridge-cli" \
        "$(crabridge_repo_root)/target/release/crabridge-cli"; do
        if [[ -n "${candidate}" && -x "${candidate}" ]]; then
            printf '%s\n' "${candidate}"
            return 0
        fi
    done

    if [[ -f "$(crabridge_repo_root)/Cargo.toml" ]] && command -v cargo >/dev/null 2>&1; then
        printf 'cargo-run\n'
        return 0
    fi

    return 1
}

crabridge_ensure_config_dir() {
    local config_file="$1"
    mkdir -p "$(dirname "${config_file}")"
}

crabridge_run_cli() {
    local cli="$1"
    shift

    if [[ "${cli}" == "cargo-run" ]]; then
        (
            cd "$(crabridge_repo_root)"
            cargo run --quiet --bin crabridge-cli --no-default-features -- "$@"
        )
        return
    fi

    exec "${cli}" "$@"
}

crabridge_setup_usage() {
    local platform="$1"
    cat <<EOF
Usage: setup-${platform}.sh [OPTIONS] [-- EXTRA_ARGS...]

Write or refresh Codex + CrabBridge configuration via crabridge-cli setup.

Options:
  --check              Check configuration only (same as docker-check-${platform}.sh)
  --config FILE        Bridge config path (default: $(crabridge_default_config_file))
  --config-dir DIR     Config directory (default: $(crabridge_default_config_dir))
  --bind-addr ADDR     Bridge listen address (default: $(crabridge_default_bind_addr))
  --provider SLUG      Single provider preset (deepseek | kimi)
  --providers LIST     Comma-separated providers (e.g. kimi,deepseek)
  --all-providers      Configure deepseek + kimi (default when no provider flags)
  --codex-only         Skip writing bridge TOML
  --force-config       Overwrite existing bridge config
  -h, --help           Show this help

Environment:
  CRABRIDGE_CLI        Path to crabridge-cli binary
  CONFIG_DIR           Override config directory
  BIND_ADDR            Override bridge bind address
  DEEPSEEK_API_KEY, KIMI_API_KEY, UPSTREAM_API_KEY

Examples:
  ./scripts/setup-${platform}.sh
  ./scripts/setup-${platform}.sh --check
  ./scripts/setup-${platform}.sh --provider kimi --force-config
  ./scripts/setup-${platform}.sh -- --api-key "\$DEEPSEEK_API_KEY"
EOF
}

crabridge_docker_check_usage() {
    local platform="$1"
    cat <<EOF
Usage: docker-check-${platform}.sh [OPTIONS] [-- EXTRA_ARGS...]

Validate Codex + bridge configuration before or during Docker deployment.
Runs: crabridge-cli setup --docker

Options:
  --config FILE        Bridge config path (default: $(crabridge_default_config_file))
  --config-dir DIR     Config directory (default: $(crabridge_default_config_dir))
  --bind-addr ADDR     Expected bridge listen address (default: $(crabridge_default_bind_addr))
  --provider SLUG      Provider slug when config file has no [providers.*] sections
  --providers LIST     Comma-separated provider slugs to validate
  --all-providers      Validate deepseek + kimi
  -h, --help           Show this help

Environment:
  CRABRIDGE_CLI        Path to crabridge-cli binary
  CONFIG_DIR, BIND_ADDR

Examples:
  ./scripts/docker-check-${platform}.sh
  ./scripts/docker-check-${platform}.sh --all-providers
  ./scripts/docker-check-${platform}.sh --config ./crabbridge.docker.toml
EOF
}

crabridge_parse_common_args() {
    CONFIG_FILE=""
    PROVIDER=""
    PROVIDERS=""
    ALL_PROVIDERS=0
    CODEX_ONLY=0
    FORCE_CONFIG=0
    CHECK_ONLY=0
    EXTRA_ARGS=()

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --check)
                CHECK_ONLY=1
                shift
                ;;
            --config)
                CONFIG_FILE="$2"
                shift 2
                ;;
            --config-dir)
                CONFIG_DIR="$2"
                shift 2
                ;;
            --bind-addr)
                BIND_ADDR="$2"
                shift 2
                ;;
            --provider)
                PROVIDER="$2"
                shift 2
                ;;
            --providers)
                PROVIDERS="$2"
                shift 2
                ;;
            --all-providers)
                ALL_PROVIDERS=1
                shift
                ;;
            --codex-only)
                CODEX_ONLY=1
                shift
                ;;
            --force-config)
                FORCE_CONFIG=1
                shift
                ;;
            -h | --help)
                return 2
                ;;
            --)
                shift
                EXTRA_ARGS+=("$@")
                break
                ;;
            *)
                EXTRA_ARGS+=("$1")
                shift
                ;;
        esac
    done

    if [[ -z "${CONFIG_FILE}" ]]; then
        CONFIG_FILE="$(crabridge_default_config_file)"
    fi
}

crabridge_build_cli_args() {
    CLI_ARGS=()
    CLI_ARGS+=(-c "${CONFIG_FILE}")
    CLI_ARGS+=(setup)
    CLI_ARGS+=(--bind-addr "$(crabridge_default_bind_addr)")

    if [[ "${CHECK_ONLY}" -eq 1 ]]; then
        CLI_ARGS+=(--docker)
    fi
    if [[ "${ALL_PROVIDERS}" -eq 1 ]]; then
        CLI_ARGS+=(--all-providers)
    elif [[ -n "${PROVIDERS}" ]]; then
        CLI_ARGS+=(--providers "${PROVIDERS}")
    elif [[ -n "${PROVIDER}" ]]; then
        CLI_ARGS+=(--provider "${PROVIDER}")
    elif [[ "${CHECK_ONLY}" -eq 0 ]]; then
        CLI_ARGS+=(--all-providers)
    fi
    if [[ "${CODEX_ONLY}" -eq 1 ]]; then
        CLI_ARGS+=(--codex-only)
    fi
    if [[ "${FORCE_CONFIG}" -eq 1 ]]; then
        CLI_ARGS+=(--force-config)
    fi
    if [[ "${#EXTRA_ARGS[@]}" -gt 0 ]]; then
        CLI_ARGS+=("${EXTRA_ARGS[@]}")
    fi
}

crabridge_run_setup_flow() {
    local platform="$1"
    shift

    crabridge_parse_common_args "$@" || {
        crabridge_setup_usage "${platform}"
        exit 0
    }

    local cli
    cli="$(crabridge_resolve_cli)" || {
        echo "error: crabridge-cli not found. Install it or set CRABRIDGE_CLI." >&2
        exit 1
    }

    crabridge_ensure_config_dir "${CONFIG_FILE}"
    crabridge_build_cli_args

    if [[ "${CHECK_ONLY}" -eq 1 ]]; then
        echo "==> Checking CrabBridge configuration (${platform})"
    else
        echo "==> Applying CrabBridge setup (${platform})"
    fi
    echo "    config: $(realpath -m "${CONFIG_FILE}" 2>/dev/null || printf '%s' "${CONFIG_FILE}")"
    echo "    bind:   $(crabridge_default_bind_addr)"

    crabridge_run_cli "${cli}" "${CLI_ARGS[@]}"
}

crabridge_run_docker_check_flow() {
    local platform="$1"
    shift

    CHECK_ONLY=1
    crabridge_parse_common_args "$@" || {
        crabridge_docker_check_usage "${platform}"
        exit 0
    }
    CHECK_ONLY=1

    local cli
    cli="$(crabridge_resolve_cli)" || {
        echo "error: crabridge-cli not found. Install it or set CRABRIDGE_CLI." >&2
        exit 1
    }

    crabridge_build_cli_args

    echo "==> Docker configuration check (${platform})"
    echo "    config: $(realpath -m "${CONFIG_FILE}" 2>/dev/null || printf '%s' "${CONFIG_FILE}")"
    echo "    bind:   $(crabridge_default_bind_addr)"

    crabridge_run_cli "${cli}" "${CLI_ARGS[@]}"
}
