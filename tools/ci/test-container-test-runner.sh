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
