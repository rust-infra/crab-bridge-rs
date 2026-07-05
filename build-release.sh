#!/usr/bin/env bash
set -euo pipefail

# 独立跨平台打包脚本：支持 macOS / Linux / Windows
# 依赖：Rust、rustup、cargo-zigbuild（脚本会自动提示安装）

BIN_NAME="crabridge"
DIST_DIR="dist"

# 目标平台列表（target, 压缩包名后缀）
TARGETS=(
  "x86_64-unknown-linux-musl:linux-x64"
  "aarch64-unknown-linux-musl:linux-arm64"
  "x86_64-apple-darwin:macos-x64"
  "aarch64-apple-darwin:macos-arm64"
  "x86_64-pc-windows-gnu:windows-x64"
)

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# 检查命令是否存在
command_exists() {
  command -v "$1" >/dev/null 2>&1
}

# 确保 cargo-zigbuild 已安装
ensure_cargo_zigbuild() {
  if ! command_exists cargo-zigbuild; then
    log_warn "未找到 cargo-zigbuild，准备安装..."
    if ! command_exists zig; then
      log_error "请先安装 Zig：https://ziglang.org/download/"
      log_error "macOS 可用: brew install zig"
      log_error "Linux 可用: 下载解压后放到 PATH"
      exit 1
    fi
    cargo install cargo-zigbuild
  fi
}

# 确保 target 已安装
ensure_target() {
  local target="$1"
  if ! rustup target list --installed | grep -q "^${target}\$"; then
    log_info "安装 Rust target: ${target}"
    rustup target add "${target}"
  fi
}

# 构建单个目标
build_target() {
  local target="$1"
  local suffix="$2"

  log_info "开始构建: ${target}"

  cargo zigbuild --release --target "${target}"

  local src_dir="target/${target}/release"
  local exe_name="${BIN_NAME}"
  local out_name="${BIN_NAME}-${suffix}"

  if [[ "${target}" == *"windows"* ]]; then
    exe_name="${BIN_NAME}.exe"
    out_name="${BIN_NAME}-${suffix}.exe"
  fi

  local src_path="${src_dir}/${exe_name}"
  if [[ ! -f "${src_path}" ]]; then
    log_error "构建产物不存在: ${src_path}"
    exit 1
  fi

  # 创建输出目录
  mkdir -p "${DIST_DIR}"

  # 复制产物到 dist
  cp "${src_path}" "${DIST_DIR}/${out_name}"

  # 非 Windows 二进制 strip 减小体积
  if [[ "${target}" != *"windows"* ]]; then
    strip "${DIST_DIR}/${out_name}" || log_warn "strip 失败，跳过"
  fi

  # 打包
  local pkg_name="${BIN_NAME}-${suffix}"
  if [[ "${target}" == *"windows"* ]]; then
    (cd "${DIST_DIR}" && zip "${pkg_name}.zip" "${out_name}" >/dev/null)
    log_info "已生成: ${DIST_DIR}/${pkg_name}.zip"
  else
    (cd "${DIST_DIR}" && tar -czf "${pkg_name}.tar.gz" "${out_name}")
    log_info "已生成: ${DIST_DIR}/${pkg_name}.tar.gz"
  fi
}

main() {
  log_info "跨平台独立二进制打包脚本"
  log_info "目标平台: Linux(x64/arm64), macOS(x64/arm64), Windows(x64)"

  ensure_cargo_zigbuild

  mkdir -p "${DIST_DIR}"

  for item in "${TARGETS[@]}"; do
    target="${item%%:*}"
    suffix="${item##*:}"
    ensure_target "${target}"
    build_target "${target}" "${suffix}"
  done

  log_info "全部构建完成，产物目录: ${DIST_DIR}/"
  ls -lh "${DIST_DIR}"
}

main "$@"
