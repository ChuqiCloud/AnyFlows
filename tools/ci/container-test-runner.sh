#!/usr/bin/env bash

readonly CI_TEST_RUNNER_IMAGE="docker.io/library/rust:1.97.1-bookworm@sha256:77fac8b98f9f46062bb680b6d25d5bcaabfc400143952ebc572e924bcbedc3fa"

ci_extract_test_binary() {
    sed -nE 's/^[[:space:]]*Executable .* \(([^()]*)\)$/\1/p' | tail -n 1
}

ci_build_test_binary() {
    local package="$1"
    local target_kind="$2"
    local target_name="${3:-}"
    local output=""
    local executable=""
    local -a target_args=()

    case "${target_kind}" in
        lib)
            target_args=(--lib)
            ;;
        test)
            if [[ -z "${target_name}" ]]; then
                echo "集成测试目标名不能为空" >&2
                return 2
            fi
            target_args=(--test "${target_name}")
            ;;
        *)
            echo "未知测试目标类型：${target_kind}" >&2
            return 2
            ;;
    esac

    if ! output="$(CARGO_TERM_COLOR=never cargo test --locked --package "${package}" \
        "${target_args[@]}" --no-run 2>&1)"; then
        printf '%s\n' "${output}" >&2
        return 1
    fi
    printf '%s\n' "${output}" >&2
    executable="$(ci_extract_test_binary <<<"${output}")"
    if [[ -z "${executable}" || ! -f "${executable}" ]]; then
        echo "无法定位已编译的 Rust 测试二进制" >&2
        return 1
    fi
    printf '%s\n' "${executable}"
}

ci_run_test_binary() {
    local network_name="$1"
    local runner_name="$2"
    local executable="$3"
    shift 3
    local separator_found=false
    local -a environment_args=()
    local -a test_args=()

    while (($# > 0)); do
        if [[ "$1" == "--" ]]; then
            separator_found=true
            shift
            break
        fi
        environment_args+=(--env "$1")
        shift
    done
    if [[ "${separator_found}" != true ]]; then
        echo "测试运行参数缺少 -- 分隔符" >&2
        return 2
    fi
    test_args=("$@")

    docker create --name "${runner_name}" --network "${network_name}" \
        "${environment_args[@]}" \
        "${CI_TEST_RUNNER_IMAGE}" \
        bash -c 'chmod 0755 /tmp/anyflows-test && exec /tmp/anyflows-test "$@"' \
        _ "${test_args[@]}" >/dev/null
    docker cp "${executable}" "${runner_name}:/tmp/anyflows-test"
    docker start --attach "${runner_name}"
}
