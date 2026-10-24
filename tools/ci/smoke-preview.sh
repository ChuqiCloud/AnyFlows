#!/usr/bin/env bash
set -euo pipefail

archive="$(realpath "${1:?Usage: smoke-preview.sh archive.tar.gz}")"
temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/anyflows-preview.XXXXXX")"
server_pid=''
cleanup() {
  status=$?
  trap - EXIT
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  if ((status != 0)); then
    tail -n 80 "$temporary_directory/server.log" >&2 || true
  fi
  rm -rf -- "$temporary_directory"
  exit "$status"
}
trap cleanup EXIT

tar -xzf "$archive" -C "$temporary_directory"
package="$(find "$temporary_directory" -mindepth 1 -maxdepth 1 -type d -print -quit)"
test -x "$package/anyflows"
test -x "$package/anyflows-launcher"
test -x "$package/start.sh"
grep -Fx "commit=$(git rev-parse HEAD)" "$package/VERSION"
cd "$package"
export AF_CONFIG_FILE="$package/anyflows.conf"
export AF_SERVER__BIND='127.0.0.1:18080'
export AF_AUTH__SESSION_SIGNING_KEY="$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=\n')"
export AF_CREDENTIAL_ENCRYPTION__KEY="$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=\n')"
./start.sh > "$temporary_directory/server.log" 2>&1 &
server_pid=$!
ready=0
for _ in $(seq 1 90); do
  kill -0 "$server_pid" 2>/dev/null || break
  if curl --silent --fail --max-time 2 http://127.0.0.1:18080/readyz > /dev/null; then
    ready=1
    break
  fi
  sleep 1
done
test "$ready" = 1
curl --silent --show-error --fail --max-time 5 http://127.0.0.1:18080/healthz > /dev/null
curl --silent --show-error --fail --max-time 5 http://127.0.0.1:18080/ > "$temporary_directory/index.html"
grep -qi '<html' "$temporary_directory/index.html"
printf 'Preview archive passed startup, migrations, readiness and embedded frontend checks.\n'
