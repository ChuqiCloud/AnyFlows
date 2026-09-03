#!/usr/bin/env bash
set -Eeuo pipefail

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly RUNNER="${SCRIPT_DIR}/run-live-provider-test.sh"

export ANYFLOWS_LIVE_OPENAI_RESPONSES_CASE="live_native_openai_responses_low_cost"
export ANYFLOWS_LIVE_OPENAI_RESPONSES_BASE_URL="https://api.example.com/v1"
export ANYFLOWS_LIVE_OPENAI_RESPONSES_API_KEY="temporary-test-key"
export ANYFLOWS_LIVE_OPENAI_RESPONSES_MODEL="test-model"
export ANYFLOWS_LIVE_PROVIDER_PREFLIGHT_ONLY=1

bash "${RUNNER}" openai >/dev/null

export ANYFLOWS_LIVE_ANTHROPIC_CASE="live_native_anthropic_runtime"
export ANYFLOWS_LIVE_ANTHROPIC_BASE_URL="https://api.example.com/v1"
export ANYFLOWS_LIVE_ANTHROPIC_API_KEY="temporary-test-key"
export ANYFLOWS_LIVE_ANTHROPIC_MODEL="test-model"
bash "${RUNNER}" anthropic >/dev/null

export ANYFLOWS_LIVE_GEMINI_CASE="live_native_gemini_runtime"
export ANYFLOWS_LIVE_GEMINI_BASE_URL="https://generativelanguage.example.com/v1beta"
export ANYFLOWS_LIVE_GEMINI_API_KEY="temporary-test-key"
export ANYFLOWS_LIVE_GEMINI_MODEL="test-model"
bash "${RUNNER}" gemini >/dev/null

assert_rejected() {
    local expected="$1"
    shift
    local output
    if output="$(bash "${RUNNER}" openai 2>&1)"; then
        echo "预期真实供应商预检失败，但命令成功：${expected}" >&2
        exit 1
    fi
    if [[ "${output}" != *"${expected}"* ]]; then
        echo "真实供应商预检失败原因不匹配：${expected}" >&2
        exit 1
    fi
}

export ANYFLOWS_LIVE_OPENAI_RESPONSES_BASE_URL="https://user:secret@api.example.com/v1"
assert_rejected "HTTPS 地址"

export ANYFLOWS_LIVE_OPENAI_RESPONSES_BASE_URL="https://api.example.com/v1?x=1"
assert_rejected "HTTPS 地址"

export ANYFLOWS_LIVE_OPENAI_RESPONSES_BASE_URL="https://127.0.0.1:443/v1"
assert_rejected "loopback"

export ANYFLOWS_LIVE_OPENAI_RESPONSES_BASE_URL="https://api.example.com:65536/v1"
assert_rejected "端口无效"

export ANYFLOWS_LIVE_OPENAI_RESPONSES_BASE_URL="https://api.example.com/v1"
export ANYFLOWS_LIVE_OPENAI_RESPONSES_API_KEY="   "
assert_rejected "缺少环境变量"

echo "真实供应商 E2E 预检回归通过"
