#!/usr/bin/env bash
set -euo pipefail

source_commit="$(git rev-parse HEAD)"
destination="$(mktemp -d)"
trap 'rm -rf "$destination"' EXIT

git -C "$destination" init --quiet
git -C "$destination" config user.name ci
git -C "$destination" config user.email ci@example.invalid
git -C "$destination" commit --quiet --allow-empty -m 'empty export destination'

python3 tools/repository/export-public-core.py \
  --source . \
  --destination "$destination" \
  --commit "$source_commit" \
  --allow-replace

test -f "$destination/Cargo.toml"
test -f "$destination/docs/public-core-source.json"
test "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sourceCommit"])' "$destination/docs/public-core-source.json")" = "$source_commit"
python3 tools/ci/verify-public-core-boundary.py --root "$destination" --mode public
