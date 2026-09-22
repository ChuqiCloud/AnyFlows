#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/anyflows-release-test.XXXXXX")"
trap 'rm -rf "$temporary_directory"' EXIT

workspace_version="$(
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
)"
tag="v$workspace_version"
commit="$(git -C "$workspace" rev-parse HEAD)"

RELEASE_REQUIRE_MASTER=0 \
  bash "$workspace/tools/ci/verify-release-tag.sh" "$tag" "$commit" \
  >"$temporary_directory/verify.out"
grep -q "版本标签 $tag 已通过发布校验" "$temporary_directory/verify.out"

if RELEASE_REQUIRE_MASTER=0 \
  bash "$workspace/tools/ci/verify-release-tag.sh" "v999.0.0" "$commit" \
  >/dev/null 2>&1; then
  printf '版本不一致的标签不应通过校验。\n' >&2
  exit 1
fi

fake_binary="$temporary_directory/af-server"
printf '#!/usr/bin/env sh\nexit 0\n' >"$fake_binary"
chmod 0755 "$fake_binary"

RELEASE_SKIP_ARCH_CHECK=1 SOURCE_DATE_EPOCH=0 \
  bash "$workspace/tools/ci/package-release.sh" \
  "$tag" amd64 "$fake_binary" "$temporary_directory/release"

printf '测试变更摘要。\n' >"$temporary_directory/release/CHANGELOG.md"
bash "$workspace/tools/ci/finalize-release.sh" \
  "$tag" "$commit" "$temporary_directory/release"

test "$(wc -l <"$temporary_directory/release/SHA256SUMS")" -eq 1
test -x "$workspace/scripts/systemd/install.sh"
bash -n "$workspace/scripts/systemd/install.sh"
test -s "$workspace/CHANGELOG.md"
grep -q 'ProtectSystem=strict' "$workspace/scripts/systemd/anyflows.service"
grep -q "anyflows-${tag}-linux-amd64.tar.gz" \
  "$temporary_directory/release/RELEASE_NOTES.md"
! grep -q "anyflows-${tag}-linux-arm64.tar.gz" \
  "$temporary_directory/release/RELEASE_NOTES.md"
grep -q '测试变更摘要。' "$temporary_directory/release/RELEASE_NOTES.md"

# Read the full listing before matching to avoid SIGPIPE under pipefail.
archive_listing="$temporary_directory/archive.list"
tar -tzf "$temporary_directory/release/anyflows-${tag}-linux-amd64.tar.gz" \
  >"$archive_listing"
grep -q "/anyflows$" "$archive_listing"
grep -q "/install.sh$" "$archive_listing"
grep -q "/anyflows.service$" "$archive_listing"
grep -q "/CHANGELOG.md$" "$archive_listing"
grep -q "/anyflows.conf$" "$archive_listing"

printf '发布脚本回归通过。\n'
