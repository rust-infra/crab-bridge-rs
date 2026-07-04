#!/usr/bin/env bash
# Install CrabBridge on Linux.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "${SCRIPT_DIR}/install-unix.sh" linux "$@"
