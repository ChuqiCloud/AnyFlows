#!/usr/bin/env bash
set -Eeuo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/container-test-runner.sh"

readonly REDIS_IMAGE="docker.io/library/redis:8.2.7-bookworm@sha256:d30960f73a599496d8b2802c97758bab6b1cd421fd06337f837779c47a57e1f3"
readonly CACHE_SMOKE_TEST_NAMES=(
    "redis_hybrid_cache_smoke"
    "redis_request_rate_limit_multi_instance_capacity"
)
readonly HTTP_SMOKE_TEST_TARGET="token_auth_chain"
readonly HTTP_SMOKE_TEST_NAME="redis_rpm_admission_is_shared_across_http_instances"

container_name=""
network_name=""
declare -a runner_names=()

cleanup() {
    local status=$?
    trap - EXIT
    local runner_name=""
    for runner_name in "${runner_names[@]}"; do
        docker rm --force "${runner_name}" >/dev/null 2>&1 || true
    done
    if [[ -n "${container_name}" ]]; then
        if ((status != 0)); then
            docker logs "${container_name}" >&2 || true
        fi
        docker rm --force "${container_name}" >/dev/null 2>&1 || true
    fi
    if [[ -n "${network_name}" ]]; then
        docker network rm "${network_name}" >/dev/null 2>&1 || true
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

run_cache_smoke_tests() {
    local test_list=""
    local test_binary=""
    local runner_name=""
    local test_name=""

    test_list="$(cargo test --locked --package af-cache --test redis_integration -- --list)"
    for test_name in "${CACHE_SMOKE_TEST_NAMES[@]}"; do
        if ! grep --fixed-strings --line-regexp --quiet "${test_name}: test" <<<"${test_list}"; then
            echo "未发现必需的 Redis 缓存冒烟测试：${test_name}" >&2
            return 1
        fi
    done

    if [[ -n "${network_name}" ]]; then
        test_binary="$(ci_build_test_binary af-cache test redis_integration)"
        for test_name in "${CACHE_SMOKE_TEST_NAMES[@]}"; do
            runner_name="anyflows-redis-cache-test-${BASHPID}-${test_name}"
            runner_names+=("${runner_name}")
            ci_run_test_binary "${network_name}" "${runner_name}" "${test_binary}" \
                "AF_TEST_REDIS_URL=${redis_url}" \
                "AF_REQUIRE_LIVE_REDIS=1" \
                -- "${test_name}" --exact --include-ignored
        done
        return
    fi

    for test_name in "${CACHE_SMOKE_TEST_NAMES[@]}"; do
        AF_TEST_REDIS_URL="${redis_url}" \
            AF_REQUIRE_LIVE_REDIS=1 \
            cargo test --locked --package af-cache --test redis_integration \
                "${test_name}" -- --exact --include-ignored
    done
}

run_http_smoke_test() {
    local test_list=""
    local test_binary=""
    local runner_name=""

    test_list="$(cargo test --locked --package af-server --test "${HTTP_SMOKE_TEST_TARGET}" -- --list)"
    if ! grep --fixed-strings --line-regexp --quiet "${HTTP_SMOKE_TEST_NAME}: test" <<<"${test_list}"; then
        echo "未发现必需的 Redis HTTP 限流冒烟测试：${HTTP_SMOKE_TEST_NAME}" >&2
        return 1
    fi

    if [[ -n "${network_name}" ]]; then
        test_binary="$(ci_build_test_binary af-server test "${HTTP_SMOKE_TEST_TARGET}")"
        runner_name="anyflows-redis-http-test-${BASHPID}"
        runner_names+=("${runner_name}")
        ci_run_test_binary "${network_name}" "${runner_name}" "${test_binary}" \
            "AF_TEST_REDIS_URL=${redis_url}" \
            "AF_REQUIRE_LIVE_REDIS=1" \
            -- "${HTTP_SMOKE_TEST_NAME}" --exact --include-ignored
        return
    fi

    AF_TEST_REDIS_URL="${redis_url}" \
        AF_REQUIRE_LIVE_REDIS=1 \
        cargo test --locked --package af-server --test "${HTTP_SMOKE_TEST_TARGET}" \
            "${HTTP_SMOKE_TEST_NAME}" -- --exact --include-ignored
}

redis_url="${AF_TEST_REDIS_URL:-}"
if [[ -z "${redis_url}" ]]; then
    network_name="anyflows-redis-network-${BASHPID}"
    container_name="anyflows-redis-${BASHPID}"
    docker network create "${network_name}" >/dev/null
    docker run --detach --name "${container_name}" --network "${network_name}" \
        --network-alias redis \
        "${REDIS_IMAGE}" redis-server --save '' --appendonly no >/dev/null
    wait_for_redis
    redis_url="redis://redis:6379/"
fi

case "${redis_url}" in
    redis://*) ;;
    *)
        echo "AF_TEST_REDIS_URL 必须使用 redis:// 地址" >&2
        exit 2
        ;;
esac

run_cache_smoke_tests
run_http_smoke_test
