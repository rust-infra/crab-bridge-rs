#!/usr/bin/env bash
# Configure Codex + CrabBridge on macOS via crabridge-cli setup.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/crabridge-cli-common.sh
source "${SCRIPT_DIR}/lib/crabridge-cli-common.sh"

crabridge_run_setup_flow macos "$@"
