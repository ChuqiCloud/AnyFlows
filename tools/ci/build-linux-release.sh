#!/usr/bin/env bash
set -euo pipefail

target="${1:-}"
case "$target" in
  x86_64-unknown-linux-gnu) expected_arch=x86_64 ;;
  aarch64-unknown-linux-gnu) expected_arch=aarch64 ;;
  *) printf 'Unsupported native Linux release target: %s\n' "$target" >&2; exit 2 ;;
esac
[[ "$(uname -m)" == "$expected_arch" ]] || {
  printf 'The release runner must match target %s.\n' "$target" >&2
  exit 1
}

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly image='rust:1.97.1-bookworm@sha256:77fac8b98f9f46062bb680b6d25d5bcaabfc400143952ebc572e924bcbedc3fa'
toolchain="$(sed -n 's/^channel = "\(.*\)"/\1/p' "$workspace/rust-toolchain.toml")"
[[ "$toolchain" == '1.97.1' ]] || {
  printf 'Update the pinned Bookworm image together with the Rust toolchain.\n' >&2
  exit 1
}
for frontend in web web-next; do
  [[ -s "$workspace/$frontend/dist/index.html" ]] || {
    printf 'Build %s before the native Linux release.\n' "$frontend" >&2
    exit 1
  }
done

builder=''
cleanup() {
  local status=$?
  trap - EXIT
  if [[ -n "$builder" ]]; then
    # Restore completed compilation work even when a later release check fails.
    for cache in bookworm bookworm-cargo; do
      mkdir -p "$workspace/target/$cache"
      if ! docker cp "$builder:/workspace/target/$cache/." "$workspace/target/$cache/"; then
        if [[ "$status" == 0 ]]; then status=1; fi
      fi
    done
    if ! docker rm --force "$builder" >/dev/null; then
      if [[ "$status" == 0 ]]; then status=1; fi
    fi
    # Only a completed invocation may publish a nonempty compilation cache.
    # Do not emit this output when cancellation interrupted the build or cleanup.
    if [[ -n "${GITHUB_OUTPUT:-}" && "$status" != 124 && "$status" != 130 && "$status" != 137 && "$status" != 143 ]] \
      && [[ -d "$workspace/target/bookworm-cargo/registry" ]] \
      && [[ -n "$(find "$workspace/target/bookworm" -type f -name '*.rlib' -size +0c -print -quit)" ]]; then
      printf 'cache-ready=true\n' >> "$GITHUB_OUTPUT"
    fi
  fi
  exit "$status"
}
trap cleanup EXIT

builder="$(docker create --workdir /workspace \
  --env CARGO_HOME=/workspace/target/bookworm-cargo \
  --env CARGO_TARGET_DIR=/workspace/target/bookworm \
  --env RUSTUP_TOOLCHAIN="$toolchain" \
  --env CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
  --env CARGO_TERM_COLOR=always \
  --env AF_BUILD_COMMIT="${AF_BUILD_COMMIT:-$(git -C "$workspace" rev-parse HEAD)}" \
  --env AF_BUILD_TIME="${AF_BUILD_TIME:-$(date -u +%Y-%m-%dT%H:%M:%SZ)}" \
  "$image" sleep infinity)"
docker start "$builder" >/dev/null
docker exec "$builder" mkdir -p /workspace/target/bookworm /workspace/target/bookworm-cargo

# Copy instead of bind-mounting: a job may itself run inside a Docker container.
git -C "$workspace" archive --format=tar HEAD | docker exec --interactive "$builder" tar -xf - -C /workspace
for frontend in web web-next; do
  docker cp "$workspace/$frontend/dist" "$builder:/workspace/$frontend/dist"
done
for cache in bookworm bookworm-cargo; do
  if [[ -d "$workspace/target/$cache" ]]; then
    docker cp "$workspace/target/$cache/." "$builder:/workspace/target/$cache/"
  fi
done

docker exec "$builder" bash -euo pipefail -c '
  [[ "$(getconf GNU_LIBC_VERSION)" == "glibc 2.36" ]]
  rustc --version --verbose
  cargo build --release --locked --package af-server --package af-launcher --target "$1"
  for executable in af-server anyflows-launcher; do
    binary="target/bookworm/$1/release/$executable"
    versions="$(readelf --version-info "$binary" | grep -oE "GLIBC_[0-9]+(\.[0-9]+)+" | sort -Vu)"
    maximum="$(printf "%s\n" "$versions" | sed "s/^GLIBC_//" | sort -V | tail -n 1)"
    [[ -n "$maximum" && "$(printf "%s\n" "$maximum" 2.36 | sort -V | tail -n 1)" == "2.36" ]] || {
      printf "%s exceeds the glibc 2.36 release baseline: %s\n" "$executable" "$maximum" >&2
      exit 1
    }
    printf "%s requires at most GLIBC_%s\n" "$executable" "$maximum"
    "$binary" --version
  done
' _ "$target"
