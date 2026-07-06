#!/usr/bin/env bash
# Validate CrabBridge + Codex configuration on Linux (crabridge-cli setup --docker).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/crabridge-cli-common.sh
source "${SCRIPT_DIR}/lib/crabridge-cli-common.sh"

crabridge_run_docker_check_flow linux "$@"
