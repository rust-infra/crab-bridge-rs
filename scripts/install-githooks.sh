#!/usr/bin/env bash
# Point this clone at the versioned hooks under .githooks/
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

git config core.hooksPath .githooks
chmod +x .githooks/pre-push

echo "Installed git hooks: core.hooksPath=$(git config --get core.hooksPath)"
echo "pre-push will run: cargo fmt --check --all && cargo clippy --workspace -- -D warnings"
echo "Bypass with: SKIP_PRE_PUSH=1 git push"
