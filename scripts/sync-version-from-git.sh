#!/usr/bin/env bash
# Resolve release version for embedding / packaging.
#
# Usage:
#   ./scripts/sync-version-from-git.sh           # from env / tag / git describe
#   ./scripts/sync-version-from-git.sh 1.0.3     # explicit (optional leading v)
#   source ./scripts/sync-version-from-git.sh    # export CRABBRIDGE_VERSION
#
# Always exports CRABBRIDGE_VERSION for cargo build.rs.
# Cargo.toml + tauri.conf.json are only rewritten when the version is semver
# (so `git describe` like `1.0.3-3-gabc` is embedded but not written to package metadata).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"
cd "${ROOT}"

normalize() {
  local raw="${1:-}"
  raw="${raw#v}"
  echo "${raw}"
}

is_semver() {
  # Cargo-compatible: MAJOR.MINOR.PATCH with optional pre-release / build metadata.
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]
}

if [[ "${1:-}" != "" ]]; then
  VERSION="$(normalize "$1")"
elif [[ "${CRABBRIDGE_VERSION:-}" != "" ]]; then
  VERSION="$(normalize "${CRABBRIDGE_VERSION}")"
elif [[ "${GITHUB_REF_TYPE:-}" == "tag" && "${GITHUB_REF_NAME:-}" != "" ]]; then
  VERSION="$(normalize "${GITHUB_REF_NAME}")"
else
  VERSION="$(normalize "$(git describe --tags --always --dirty 2>/dev/null || true)")"
fi

if [[ -z "${VERSION}" ]]; then
  echo "sync-version-from-git: could not resolve a version" >&2
  return 1 2>/dev/null || exit 1
fi

export CRABBRIDGE_VERSION="${VERSION}"
export ROOT

if is_semver "${VERSION}"; then
  python3 - <<'PY'
import json
import os
import pathlib
import re

root = pathlib.Path(os.environ["ROOT"])
version = os.environ["CRABBRIDGE_VERSION"]

cargo = root / "Cargo.toml"
text = cargo.read_text(encoding="utf-8")
updated, n = re.subn(
    r'(?m)^(version\s*=\s*")[^"]*(")',
    rf"\g<1>{version}\g<2>",
    text,
    count=1,
)
if n != 1:
    raise SystemExit("failed to update workspace version in Cargo.toml")
cargo.write_text(updated, encoding="utf-8")

tauri_path = root / "crates" / "crabbridge-desktop" / "tauri.conf.json"
cfg = json.loads(tauri_path.read_text(encoding="utf-8"))
cfg["version"] = version
tauri_path.write_text(json.dumps(cfg, indent=2) + "\n", encoding="utf-8")

print(f"synced package metadata -> {version}")
PY
else
  echo "sync-version-from-git: embedding ${VERSION} (skipped Cargo/tauri rewrite; not plain-ish semver)"
fi

echo "CRABBRIDGE_VERSION=${CRABBRIDGE_VERSION}"
