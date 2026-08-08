#!/usr/bin/env bash
set -Eeuo pipefail

readonly PROVIDER="${1:-}"

case "${PROVIDER}" in
    openai)
        readonly CASE_VARIABLE="ANYFLOWS_LIVE_OPENAI_RESPONSES_CASE"
        readonly EXPECTED_CASE="live_native_openai_responses_low_cost"
        readonly BASE_URL_VARIABLE="ANYFLOWS_LIVE_OPENAI_RESPONSES_BASE_URL"
        readonly API_KEY_VARIABLE="ANYFLOWS_LIVE_OPENAI_RESPONSES_API_KEY"
        readonly MODEL_VARIABLE="ANYFLOWS_LIVE_OPENAI_RESPONSES_MODEL"
        readonly TEST_TARGET="openai_responses_runtime_chain"
        readonly TEST_NAME="live_native_openai_responses_low_cost"
        ;;
    anthropic)
        readonly CASE_VARIABLE="ANYFLOWS_LIVE_ANTHROPIC_CASE"
        readonly EXPECTED_CASE="live_native_anthropic_runtime"
        readonly BASE_URL_VARIABLE="ANYFLOWS_LIVE_ANTHROPIC_BASE_URL"
        readonly API_KEY_VARIABLE="ANYFLOWS_LIVE_ANTHROPIC_API_KEY"
        readonly MODEL_VARIABLE="ANYFLOWS_LIVE_ANTHROPIC_MODEL"
        readonly TEST_TARGET="anthropic_runtime_chain"
        readonly TEST_NAME="live_native_anthropic_runtime"
        ;;
    gemini)
        readonly CASE_VARIABLE="ANYFLOWS_LIVE_GEMINI_CASE"
        readonly EXPECTED_CASE="live_native_gemini_runtime"
        readonly BASE_URL_VARIABLE="ANYFLOWS_LIVE_GEMINI_BASE_URL"
        readonly API_KEY_VARIABLE="ANYFLOWS_LIVE_GEMINI_API_KEY"
        readonly MODEL_VARIABLE="ANYFLOWS_LIVE_GEMINI_MODEL"
        readonly TEST_TARGET="gemini_runtime_chain"
        readonly TEST_NAME="live_native_gemini_runtime"
        ;;
    *)
        echo "用法：$0 <openai|anthropic|gemini>" >&2
        exit 2
        ;;
esac

require_environment_variable() {
    local variable_name="$1"
    if [[ -z "${!variable_name:-}" ]]; then
        echo "真实 ${PROVIDER} E2E 缺少环境变量：${variable_name}" >&2
        return 1
    fi
}

for variable_name in \
    "${CASE_VARIABLE}" \
    "${BASE_URL_VARIABLE}" \
    "${API_KEY_VARIABLE}" \
    "${MODEL_VARIABLE}"; do
    require_environment_variable "${variable_name}"
done

if [[ "${!CASE_VARIABLE}" != "${EXPECTED_CASE}" ]]; then
    echo "真实 ${PROVIDER} E2E 门禁值不匹配：${CASE_VARIABLE}" >&2
    exit 2
fi

readonly BASE_URL="${!BASE_URL_VARIABLE}"
if [[ ! "${BASE_URL}" =~ ^https:// ]]; then
    echo "真实 ${PROVIDER} E2E 只允许 HTTPS 上游地址" >&2
    exit 2
fi
if [[ "${BASE_URL}" =~ ^https://(localhost|127\.0\.0\.1|\[::1\])([/:]|$) ]]; then
    echo "真实 ${PROVIDER} E2E 不接受 loopback 上游" >&2
    exit 2
fi

# 先确认精确测试仍然存在，避免过滤器漂移后以零测试成功。
test_list="$(
    cargo test --locked --package af-server --test "${TEST_TARGET}" -- --list
)"
if ! grep --fixed-strings --line-regexp --quiet "${TEST_NAME}: test" <<<"${test_list}"; then
    echo "未发现必需的真实供应商 E2E：${TEST_NAME}" >&2
    exit 1
fi

# 每次只运行一个精确用例，禁止通过宽泛过滤器批量消耗真实额度。
cargo test --locked --package af-server --test "${TEST_TARGET}" \
    "${TEST_NAME}" -- --exact
