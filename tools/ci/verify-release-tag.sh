#!/usr/bin/env bash
set -euo pipefail

tag="${1:-}"
commit="${2:-}"
workspace="${RELEASE_WORKSPACE:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"

fail() {
  printf '发布校验失败：%s\n' "$*" >&2
  exit 1
}

semver='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$'
[[ "$tag" == v* && "${tag#v}" =~ $semver ]] || fail "标签必须是以 v 开头的 SemVer 版本"
[[ "$commit" =~ ^[0-9a-fA-F]{40}$ ]] || fail "提交必须是完整的 40 位 SHA"

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
[[ "${tag#v}" == "$workspace_version" ]] || fail "标签版本 ${tag#v} 与 Cargo 版本 $workspace_version 不一致"

head_commit="$(git -C "$workspace" rev-parse HEAD)"
[[ "$head_commit" == "$commit" ]] || fail "标签提交与当前检出提交不一致"

if [[ "${RELEASE_REQUIRE_MASTER:-1}" == "1" ]]; then
  master_ref="${RELEASE_MASTER_REF:-refs/remotes/origin/master}"
  if ! git -C "$workspace" rev-parse --verify --quiet "${master_ref}^{commit}" >/dev/null; then
    git -C "$workspace" fetch --no-tags origin "+refs/heads/master:$master_ref" \
      || fail "无法补全发布分支引用 $master_ref"
  fi
  git -C "$workspace" merge-base --is-ancestor "$commit" "$master_ref" \
    || fail "版本标签必须指向 master 分支中的提交"
fi

printf '版本标签 %s 已通过发布校验。\n' "$tag"
