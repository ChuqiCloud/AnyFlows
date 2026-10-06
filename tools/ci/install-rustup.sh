#!/usr/bin/env bash
set -euo pipefail

# Runner 镜像可能没有预装 rustup，先补齐官方工具链管理器。
if ! command -v rustup >/dev/null 2>&1; then
  install_url="${RUSTUP_INIT_URL:-https://sh.rustup.rs}"
  if command -v curl >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 --retry 3 --fail --silent --show-error "$install_url" \
      | sh -s -- -y --profile minimal --default-toolchain none
  elif command -v wget >/dev/null 2>&1; then
    wget --https-only --tries=3 -qO- "$install_url" \
      | sh -s -- -y --profile minimal --default-toolchain none
  else
    echo "需要 curl 或 wget 才能安装 rustup" >&2
    exit 1
  fi
fi

cargo_bin="$HOME/.cargo/bin"
export PATH="$cargo_bin:$PATH"

# Gitea Actions 兼容 GitHub Actions 的环境文件约定；本地直接执行时忽略它们。
if [[ -n "${GITHUB_PATH:-}" ]]; then
  printf '%s\n' "$cargo_bin" >> "$GITHUB_PATH"
fi
if [[ -n "${GITHUB_ENV:-}" ]]; then
  printf 'PATH=%s\n' "$cargo_bin:$PATH" >> "$GITHUB_ENV"
fi

rustup --version
