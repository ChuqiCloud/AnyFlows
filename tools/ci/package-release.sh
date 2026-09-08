#!/usr/bin/env bash
set -euo pipefail

tag="${1:-auto}"
release_arch="${2:-}"
binary_path="${3:-target/release/af-server}"
output_directory="${4:-release}"
workspace="${RELEASE_WORKSPACE:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"

fail() {
  printf '发布归档失败：%s\n' "$*" >&2
  exit 1
}

workspace_version="$({
  awk '
    /^\[workspace\.package\]$/ { in_workspace_package = 1; next }
    in_workspace_package && /^\[/ { exit }
    in_workspace_package && /^version[[:space:]]*=/ {
      line = $0
      sub(/^[^"]*"/, "", line)
      sub(/".*$/, "", line)
      print line
      exit
    }
  ' "$workspace/Cargo.toml"
} || true)"
[[ -n "$workspace_version" ]] || fail "无法读取 workspace 版本"

if [[ "$tag" == "auto" ]]; then
  tag="v$workspace_version"
fi
[[ "$tag" == "v$workspace_version" ]] || fail "归档标签 $tag 与 Cargo 版本 $workspace_version 不一致"

case "$release_arch" in
  amd64)
    expected_host_arch="x86_64"
    ;;
  arm64)
    expected_host_arch="aarch64"
    ;;
  *)
    fail "仅支持 amd64 或 arm64"
    ;;
esac

if [[ "${RELEASE_SKIP_ARCH_CHECK:-0}" != "1" ]]; then
  [[ "$(uname -m)" == "$expected_host_arch" ]] \
    || fail "当前构建节点架构与目标 $release_arch 不一致"
fi

if [[ "$binary_path" != /* ]]; then
  binary_path="$workspace/$binary_path"
fi
if [[ "$output_directory" != /* ]]; then
  output_directory="$workspace/$output_directory"
fi
[[ -x "$binary_path" ]] || fail "缺少可执行 release 二进制 $binary_path"
[[ -x "$workspace/scripts/systemd/install.sh" ]] || fail "缺少 systemd 安装脚本"
[[ -f "$workspace/scripts/systemd/anyflows.service" ]] || fail "缺少 systemd 服务单元"
[[ -f "$workspace/anyflows.conf" ]] || fail "缺少 anyflows.conf"

archive_name="anyflows-${tag}-linux-${release_arch}.tar.gz"
package_name="anyflows-${tag}-linux-${release_arch}"
temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/anyflows-release.XXXXXX")"
trap 'rm -rf "$temporary_directory"' EXIT

mkdir -p "$temporary_directory/$package_name" "$output_directory"
install -m 0755 "$binary_path" "$temporary_directory/$package_name/anyflows"
install -m 0644 "$workspace/README.md" "$temporary_directory/$package_name/README.md"
install -m 0644 "$workspace/.env.example" "$temporary_directory/$package_name/.env.example"
install -m 0644 "$workspace/anyflows.conf" "$temporary_directory/$package_name/anyflows.conf"
install -m 0644 "$workspace/CHANGELOG.md" "$temporary_directory/$package_name/CHANGELOG.md"
install -m 0755 "$workspace/scripts/systemd/install.sh" "$temporary_directory/$package_name/install.sh"
install -m 0644 "$workspace/scripts/systemd/anyflows.service" "$temporary_directory/$package_name/anyflows.service"
printf '%s\ncommit=%s\n' "$tag" "$(git -C "$workspace" rev-parse HEAD)" \
  >"$temporary_directory/$package_name/VERSION"

# 固定归档元数据，保证同一提交重复构建时不会因时间戳产生无意义漂移。
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git -C "$workspace" show -s --format=%ct HEAD)}"
tar \
  --sort=name \
  --mtime="@$source_date_epoch" \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  --create \
  --gzip \
  --file="$output_directory/$archive_name" \
  --directory="$temporary_directory" \
  "$package_name"

(
  cd "$output_directory"
  sha256sum "$archive_name" >"$archive_name.sha256"
  sha256sum --check --strict "$archive_name.sha256"
)

printf '已生成 %s\n' "$output_directory/$archive_name"
