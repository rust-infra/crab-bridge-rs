#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="${ROOT}/crates/crabbridge-desktop"
DIST_DIR="${ROOT}/dist/desktop"

log() { echo "[build-desktop] $*"; }

if ! command -v cargo-tauri >/dev/null 2>&1 && ! cargo tauri --version >/dev/null 2>&1; then
  log "installing tauri-cli (one-time)"
  cargo install tauri-cli --locked
fi

python3 "${ROOT}/scripts/generate-desktop-icons.py"

log "building CrabBridge desktop bundle"
(
  cd "${DESKTOP_DIR}"
  cargo tauri build --release
)

mkdir -p "${DIST_DIR}"

copy_bundle() {
  local pattern="$1"
  local dest_name="$2"
  local found
  found="$(find "${DESKTOP_DIR}/target/release/bundle" -name "${pattern}" -print -quit || true)"
  if [[ -n "${found}" ]]; then
    cp "${found}" "${DIST_DIR}/${dest_name}"
    log "copied ${dest_name}"
  fi
}

if [[ "$(uname -s)" == "Darwin" ]]; then
  copy_bundle "*.dmg" "crabbridge-desktop-macos.dmg"
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
