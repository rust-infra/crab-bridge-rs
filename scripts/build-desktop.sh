#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="${ROOT}/crates/crabbridge-desktop"
DIST_DIR="${ROOT}/dist/desktop"
TARGET_DIR="${CARGO_TARGET_DIR:-"${ROOT}/target"}"
BUNDLE_DIR="${TARGET_DIR}/release/bundle"

log() { echo "[build-desktop] $*"; }

if ! command -v cargo-tauri >/dev/null 2>&1 && ! cargo tauri --version >/dev/null 2>&1; then
  log "installing tauri-cli (one-time)"
  cargo install tauri-cli --locked
fi

# Embed current git tag / describe into the binary and package metadata.
# shellcheck disable=SC1091
source "${ROOT}/scripts/sync-version-from-git.sh"
export CRABBRIDGE_VERSION

python3 "${ROOT}/scripts/generate-desktop-icons.py"

log "building CrabBridge desktop bundle (version=${CRABBRIDGE_VERSION})"
(
  cd "${DESKTOP_DIR}"
  cargo tauri build
)

mkdir -p "${DIST_DIR}"

copy_dmg() {
  local dest_name="$1"
  local found=""

  if [[ -d "${BUNDLE_DIR}/dmg" ]]; then
    found="$(find "${BUNDLE_DIR}/dmg" -maxdepth 1 -name "*.dmg" ! -name "rw.*" -print -quit 2>/dev/null || true)"
  fi
  if [[ -z "${found}" && -d "${BUNDLE_DIR}" ]]; then
    found="$(find "${BUNDLE_DIR}" -name "*.dmg" ! -name "rw.*" -print -quit 2>/dev/null || true)"
  fi
  if [[ -n "${found}" ]]; then
    cp "${found}" "${DIST_DIR}/${dest_name}"
    log "copied ${dest_name} from ${found}"
  fi
}

copy_bundle() {
  local pattern="$1"
  local dest_name="$2"
  local found
  if [[ ! -d "${BUNDLE_DIR}" ]]; then
    log "bundle directory not found: ${BUNDLE_DIR}"
    return 0
  fi
  found="$(find "${BUNDLE_DIR}" -name "${pattern}" -print -quit 2>/dev/null || true)"
  if [[ -n "${found}" ]]; then
    cp "${found}" "${DIST_DIR}/${dest_name}"
    log "copied ${dest_name} from ${found}"
  fi
}

copy_app_bundle() {
  local app_path=""
  if [[ -d "${BUNDLE_DIR}/macos" ]]; then
    app_path="$(find "${BUNDLE_DIR}/macos" -maxdepth 1 -name "*.app" -print -quit 2>/dev/null || true)"
  fi
  if [[ -n "${app_path}" ]]; then
    rm -rf "${DIST_DIR}/CrabBridge.app"
    cp -R "${app_path}" "${DIST_DIR}/CrabBridge.app"
    log "copied CrabBridge.app from ${app_path}"
  fi
}

if [[ "$(uname -s)" == "Darwin" ]]; then
  copy_dmg "crabbridge-desktop-macos.dmg"
  copy_app_bundle
  copy_bundle "*.app.tar.gz" "crabbridge-desktop-macos.app.tar.gz"
elif [[ "$(uname -s)" == "Linux" ]]; then
  copy_bundle "*.AppImage" "crabbridge-desktop-linux.AppImage"
  copy_bundle "*.deb" "crabbridge-desktop-linux.deb"
else
  copy_bundle "*.msi" "crabbridge-desktop-windows.msi"
  copy_bundle "*.exe" "crabbridge-desktop-windows-setup.exe"
fi

log "desktop bundles staged under ${DIST_DIR}"
ls -lh "${DIST_DIR}" || true
