#!/usr/bin/env bash
set -euo pipefail

tag="${1:-}"
commit="${2:-}"
output_directory="${3:-release}"
workspace="${RELEASE_WORKSPACE:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"

fail() {
  printf '发布汇总失败：%s\n' "$*" >&2
  exit 1
}

[[ "$tag" == v* ]] || fail "缺少版本标签"
[[ "$commit" =~ ^[0-9a-fA-F]{40}$ ]] || fail "提交必须是完整的 40 位 SHA"

if [[ "$output_directory" != /* ]]; then
  output_directory="$workspace/$output_directory"
fi
mkdir -p "$output_directory"

archive_name="anyflows-${tag}-linux-amd64.tar.gz"
for file_name in "$archive_name" "$archive_name.sha256"; do
  if [[ -f "$workspace/$file_name" && ! -f "$output_directory/$file_name" ]]; then
    mv "$workspace/$file_name" "$output_directory/$file_name"
  fi
  [[ -f "$output_directory/$file_name" ]] || fail "缺少 $file_name"
done

(
  cd "$output_directory"
  sha256sum --check --strict "$archive_name.sha256"
)

(
  cd "$output_directory"
  cat "$archive_name.sha256" >SHA256SUMS
  sha256sum --check --strict SHA256SUMS
)

release_notes="$output_directory/RELEASE_NOTES.md"
cat >"$release_notes" <<EOF
# AnyFlows $tag

本版本由 CNB 在版本标签 \`$tag\` 对应提交 \`$commit\` 上构建并校验。

## 发行包

- \`anyflows-${tag}-linux-amd64.tar.gz\`：Linux x86_64

归档包含内嵌完整前端资源的 \`anyflows\` 单二进制、\`README.md\`、\`.env.example\`、\`anyflows.conf\`、\`CHANGELOG.md\`、systemd 安装脚本/服务单元和 \`VERSION\`。

## SHA-256

\`\`\`text
EOF
cat "$output_directory/SHA256SUMS" >>"$release_notes"
cat >>"$release_notes" <<'EOF'
```
EOF

if [[ -s "$output_directory/CHANGELOG.md" ]]; then
  printf '\n## 版本变更\n\n' >>"$release_notes"
  cat "$output_directory/CHANGELOG.md" >>"$release_notes"
else
  printf '\n## 版本变更\n\n- 首个公开版本，或当前构建未找到可比较的上一版本标签。\n' \
    >>"$release_notes"
fi

printf 'amd64 归档与摘要已通过校验。\n'
