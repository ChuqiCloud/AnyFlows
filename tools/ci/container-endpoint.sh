#!/usr/bin/env bash

CI_SERVICE_HOST=""
CI_SERVICE_PORT=""

ci_docker_daemon_hosts() {
    local endpoint="${DOCKER_HOST:-}"
    local current_gateway=""
    local bridge_gateway=""

    if [[ "${endpoint}" =~ ^tcp://(\[[^]]+\]|[^:/]+)(:[0-9]+)?/?$ ]]; then
        printf '%s\n' "${BASH_REMATCH[1]}"
    fi

    # CNB 构建容器可能不在默认 bridge 网络，优先读取当前容器实际使用的网关。
    if [[ -n "${HOSTNAME:-}" ]]; then
        current_gateway="$(docker inspect --format '{{range .NetworkSettings.Networks}}{{println .Gateway}}{{end}}' \
            "${HOSTNAME}" 2>/dev/null | sed -n '/./{p;q;}' || true)"
        if [[ -n "${current_gateway}" ]]; then
            printf '%s\n' "${current_gateway}"
        fi
    fi

    bridge_gateway="$(docker network inspect bridge \
        --format '{{(index .IPAM.Config 0).Gateway}}' 2>/dev/null || true)"
    if [[ -n "${bridge_gateway}" ]]; then
        printf '%s\n' "${bridge_gateway}"
    fi
    printf '%s\n' '127.0.0.1'
}

ci_url_host() {
    local host="$1"
    if [[ "${host}" == *:* && "${host}" != \[*\] ]]; then
        printf '[%s]\n' "${host}"
    else
        printf '%s\n' "${host}"
    fi
}

ci_tcp_reachable() {
    local host="${1#[}"
    host="${host%]}"
    timeout 1 bash -c 'exec 3<>"/dev/tcp/${1}/${2}"' _ "${host}" "${2}" \
        >/dev/null 2>&1
}

ci_published_port() {
    local container_name="$1"
    local container_port="$2"
    local mapping=""

    mapping="$(docker port "${container_name}" "${container_port}/tcp" | head -n 1 || true)"
    if [[ -z "${mapping}" ]]; then
        echo "无法读取容器 ${container_port}/tcp 的随机映射端口" >&2
        return 1
    fi
    printf '%s\n' "${mapping##*:}"
}

ci_wait_for_published_endpoint() {
    local container_name="$1"
    local container_port="$2"
    local label="$3"
    local host=""
    local -a candidates=()

    CI_SERVICE_HOST=""
    CI_SERVICE_PORT="$(ci_published_port "${container_name}" "${container_port}")"
    mapfile -t candidates < <(ci_docker_daemon_hosts | awk 'NF && !seen[$0]++')

    # 服务容器内部就绪并不代表发布端口对构建容器可达，需从客户端网络侧复核。
    for _ in $(seq 1 30); do
        for host in "${candidates[@]}"; do
            if ci_tcp_reachable "${host}" "${CI_SERVICE_PORT}"; then
                CI_SERVICE_HOST="${host}"
                return 0
            fi
        done
        sleep 1
    done
    echo "${label} 发布端口未在 30 秒内对当前构建容器可达" >&2
    return 1
}
