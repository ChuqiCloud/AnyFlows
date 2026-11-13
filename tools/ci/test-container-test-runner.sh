#!/usr/bin/env bash
set -Eeuo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/container-test-runner.sh"

temp_dir="$(mktemp --directory)"
trap 'rm -rf "${temp_dir}"' EXIT
test_binary="${temp_dir}/af_db-test"
touch "${test_binary}"
chmod 0755 "${test_binary}"

cargo() {
    printf '  Executable unittests src/lib.rs (%s)\n' "${test_binary}"
}

resolved="$(ci_build_test_binary af-db lib)"
[[ "${resolved}" == "${test_binary}" ]]

if ci_build_test_binary af-cache unknown >/dev/null 2>&1; then
    echo "未知测试目标类型必须失败" >&2
    exit 1
fi

if ci_run_test_binary network runner "${test_binary}" "KEY=value" >/dev/null 2>&1; then
    echo "缺少参数分隔符必须失败" >&2
    exit 1
fi

# Exercise the Redis entry point without Docker or Cargo: its isolated instance must
# never allocate a bridge subnet, and every runner must join that instance only.
export CI_STUB_TEST_BINARY="${test_binary}"
export CI_STUB_LOG="${temp_dir}/redis-runner.log"
export CI_STUB_FAIL_START=0
cargo() {
    printf 'cargo %s\n' "$*" >>"${CI_STUB_LOG}"
    if [[ " $* " == *" --list "* ]]; then
        printf '%s: test\n' redis_hybrid_cache_smoke \
            redis_runtime_registry_shares_nodes_and_bounds_retention \
            redis_request_rate_limit_multi_instance_capacity \
            redis_rpm_admission_is_shared_across_http_instances
    elif [[ " $* " == *" --no-run "* ]]; then
        printf '  Executable tests (%s)\n' "${CI_STUB_TEST_BINARY}"
    fi
}
docker() {
    printf 'docker %s\n' "$*" >>"${CI_STUB_LOG}"
    case "$1" in
        exec) printf 'PONG\n' ;;
        start) [[ "${CI_STUB_FAIL_START}" == 0 ]] ;;
    esac
}
export -f cargo docker

: >"${CI_STUB_LOG}"
env -u AF_TEST_REDIS_URL bash "${SCRIPT_DIR}/run-cache-redis-test.sh"
if grep --quiet '^docker network ' "${CI_STUB_LOG}"; then
    echo "隔离 Redis 测试不能创建或删除桥接网络" >&2
    exit 1
fi
grep --quiet '^docker run .* --network none ' "${CI_STUB_LOG}"
[[ "$(grep --count '^docker create .* --network container:anyflows-redis-' "${CI_STUB_LOG}")" == 4 ]]
[[ "$(grep --count '^docker create .*AF_TEST_REDIS_URL=redis://127.0.0.1:6379/' "${CI_STUB_LOG}")" == 4 ]]
[[ "$(grep --count '^docker rm --force ' "${CI_STUB_LOG}")" == 5 ]]

# A failed runner must still remove both the runner and its dedicated Redis.
: >"${CI_STUB_LOG}"
if env -u AF_TEST_REDIS_URL CI_STUB_FAIL_START=1 \
    bash "${SCRIPT_DIR}/run-cache-redis-test.sh" --cache-only; then
    echo "测试容器失败必须传递失败状态" >&2
    exit 1
fi
[[ "$(grep --count '^docker rm --force ' "${CI_STUB_LOG}")" == 2 ]]

# A caller-provided Redis remains a direct test connection with no Docker usage.
: >"${CI_STUB_LOG}"
AF_TEST_REDIS_URL=redis://external.example:6380/ \
    bash "${SCRIPT_DIR}/run-cache-redis-test.sh" --cache-only
if grep --quiet '^docker ' "${CI_STUB_LOG}"; then
    echo "外部 Redis 分支不能管理容器" >&2
    exit 1
fi
[[ "$(grep --count '^cargo .* --exact --include-ignored$' "${CI_STUB_LOG}")" == 3 ]]
