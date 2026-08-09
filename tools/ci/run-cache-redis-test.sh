#!/usr/bin/env bash
set -Eeuo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/container-endpoint.sh"

readonly REDIS_IMAGE="docker.io/library/redis:8.2.7-bookworm@sha256:d30960f73a599496d8b2802c97758bab6b1cd421fd06337f837779c47a57e1f3"
readonly SMOKE_TEST_NAME="redis_hybrid_cache_smoke"
readonly HTTP_SMOKE_TEST_TARGET="token_auth_chain"
readonly HTTP_SMOKE_TEST_NAME="redis_rpm_admission_is_shared_across_http_instances"

container_name=""

cleanup() {
    local status=$?
    trap - EXIT
    if [[ -n "${container_name}" ]]; then
        if ((status != 0)); then
            docker logs "${container_name}" >&2 || true
        fi
        docker rm --force "${container_name}" >/dev/null 2>&1 || true
    fi
    exit "${status}"
}
trap cleanup EXIT

wait_for_redis() {
    # 镜像首次拉起耗时不稳定，有限轮询可避免流水线永久等待。
    for _ in $(seq 1 60); do
        if docker exec "${container_name}" redis-cli ping 2>/dev/null | grep --quiet '^PONG$'; then
            return 0
        fi
        sleep 1
    done
    echo "Redis 测试实例未在 60 秒内就绪" >&2
    return 1
}

run_smoke_test() {
    local test_list=""

    test_list="$(cargo test --locked --package af-cache --test redis_integration -- --list)"
    if ! grep --fixed-strings --line-regexp --quiet "${SMOKE_TEST_NAME}: test" <<<"${test_list}"; then
        echo "未发现必需的 Redis 缓存冒烟测试：${SMOKE_TEST_NAME}" >&2
        return 1
    fi

    AF_TEST_REDIS_URL="${redis_url}" \
    AF_REQUIRE_LIVE_REDIS=1 \
    cargo test --locked --package af-cache --test redis_integration \
        "${SMOKE_TEST_NAME}" -- --exact --include-ignored
}

run_http_smoke_test() {
    local test_list=""

    test_list="$(cargo test --locked --package af-server --test "${HTTP_SMOKE_TEST_TARGET}" -- --list)"
    if ! grep --fixed-strings --line-regexp --quiet "${HTTP_SMOKE_TEST_NAME}: test" <<<"${test_list}"; then
        echo "未发现必需的 Redis HTTP 限流冒烟测试：${HTTP_SMOKE_TEST_NAME}" >&2
        return 1
    fi

    AF_TEST_REDIS_URL="${redis_url}" \
    AF_REQUIRE_LIVE_REDIS=1 \
    cargo test --locked --package af-server --test "${HTTP_SMOKE_TEST_TARGET}" \
        "${HTTP_SMOKE_TEST_NAME}" -- --exact --include-ignored
}

redis_url="${AF_TEST_REDIS_URL:-}"
if [[ -z "${redis_url}" ]]; then
    container_name="anyflows-redis-${BASHPID}"
    docker run --detach --name "${container_name}" --publish 6379 \
        "${REDIS_IMAGE}" redis-server --save '' --appendonly no >/dev/null
    wait_for_redis
    ci_wait_for_published_endpoint "${container_name}" 6379 "Redis"
    redis_url="redis://$(ci_url_host "${CI_SERVICE_HOST}"):${CI_SERVICE_PORT}/"
fi

case "${redis_url}" in
    redis://*) ;;
    *)
        echo "AF_TEST_REDIS_URL 必须使用 redis:// 地址" >&2
        exit 2
        ;;
esac

run_smoke_test
run_http_smoke_test
