#!/usr/bin/env bash
set -Eeuo pipefail

readonly DIALECT="${1:-}"
readonly POSTGRES_IMAGE="docker.io/library/postgres:17.10-bookworm@sha256:67870dc097790edf2bd6726658db995dcc830f799d41bb2b78ef07c9a2d5f010"
readonly MYSQL_IMAGE="docker.io/library/mysql:8.4.10@sha256:6ec1dc148edc412cce3ae27dd5b78dcd4e026976648424b3d31d9a8c6204db4e"
readonly TEST_USER="anyflows"
readonly TEST_PASSWORD="anyflows_ci"
readonly TEST_DATABASE="anyflows_migration_test_ci"
readonly SMOKE_TEST_NAME="migration::tests::migration_runner_smoke"

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

wait_for_postgres() {
    # 首次拉起镜像时初始化耗时不稳定，使用有限轮询避免流水线永久等待。
    for _ in $(seq 1 90); do
        if docker exec "${container_name}" pg_isready \
            --username="${TEST_USER}" --dbname="${TEST_DATABASE}" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    echo "PostgreSQL 测试实例未在 90 秒内就绪" >&2
    return 1
}

wait_for_mysql() {
    for _ in $(seq 1 120); do
        if docker exec --env MYSQL_PWD="${TEST_PASSWORD}" "${container_name}" \
            mysqladmin ping --host=127.0.0.1 --user="${TEST_USER}" --silent \
            >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    echo "MySQL 测试实例未在 120 秒内就绪" >&2
    return 1
}

docker_daemon_host() {
    local endpoint="${DOCKER_HOST:-}"
    local gateway=""

    if [[ "${endpoint}" =~ ^tcp://(\[[^]]+\]|[^:/]+)(:[0-9]+)?/?$ ]]; then
        printf '%s\n' "${BASH_REMATCH[1]}"
        return 0
    fi

    # Unix socket 场景优先使用 Docker bridge 网关，兼容宿主机与容器内客户端。
    gateway="$(docker network inspect bridge --format '{{(index .IPAM.Config 0).Gateway}}' 2>/dev/null || true)"
    printf '%s\n' "${gateway:-127.0.0.1}"
}

url_host() {
    local host="$1"
    if [[ "${host}" == *:* && "${host}" != \[*\] ]]; then
        printf '[%s]\n' "${host}"
    else
        printf '%s\n' "${host}"
    fi
}

published_port() {
    local container_port="$1"
    local mapping=""

    mapping="$(docker port "${container_name}" "${container_port}/tcp" | head -n 1)"
    if [[ -z "${mapping}" ]]; then
        echo "无法读取容器 ${container_port}/tcp 的随机映射端口" >&2
        return 1
    fi
    printf '%s\n' "${mapping##*:}"
}

run_smoke_test() {
    local test_list=""

    test_list="$(cargo test --locked --package af-db --lib -- --list)"
    if ! grep --fixed-strings --line-regexp --quiet "${SMOKE_TEST_NAME}: test" <<<"${test_list}"; then
        echo "未发现必需的数据库迁移冒烟测试：${SMOKE_TEST_NAME}" >&2
        return 1
    fi

    AF_TEST_DATABASE_URL="${database_url}" \
    AF_REQUIRE_LIVE_DATABASE=1 \
    AF_ALLOW_DESTRUCTIVE_MIGRATION_TEST=1 \
    cargo test --locked --package af-db --lib "${SMOKE_TEST_NAME}" -- --exact --include-ignored
}

database_url="${AF_TEST_DATABASE_URL:-}"

if [[ -z "${database_url}" ]]; then
    case "${DIALECT}" in
        sqlite)
            database_url="sqlite::memory:"
            ;;
        postgres)
            container_name="anyflows-postgres-${BASHPID}"
            docker run --detach --name "${container_name}" --publish 5432 \
                --env POSTGRES_USER="${TEST_USER}" \
                --env POSTGRES_PASSWORD="${TEST_PASSWORD}" \
                --env POSTGRES_DB="${TEST_DATABASE}" \
                "${POSTGRES_IMAGE}" >/dev/null
            wait_for_postgres
            database_url="postgres://${TEST_USER}:${TEST_PASSWORD}@$(url_host "$(docker_daemon_host)"):$(published_port 5432)/${TEST_DATABASE}"
            ;;
        mysql)
            container_name="anyflows-mysql-${BASHPID}"
            docker run --detach --name "${container_name}" --publish 3306 \
                --env MYSQL_USER="${TEST_USER}" \
                --env MYSQL_PASSWORD="${TEST_PASSWORD}" \
                --env MYSQL_ROOT_PASSWORD="root_ci" \
                --env MYSQL_DATABASE="${TEST_DATABASE}" \
                "${MYSQL_IMAGE}" >/dev/null
            wait_for_mysql
            database_url="mysql://${TEST_USER}:${TEST_PASSWORD}@$(url_host "$(docker_daemon_host)"):$(published_port 3306)/${TEST_DATABASE}?timezone=%2B08:00"
            ;;
        *)
            echo "用法：$0 <sqlite|postgres|mysql>" >&2
            exit 2
            ;;
    esac
fi

case "${DIALECT}:${database_url}" in
    sqlite:sqlite:* | postgres:postgres://* | postgres:postgresql://* | mysql:mysql://*) ;;
    *)
        echo "数据库方言与 AF_TEST_DATABASE_URL 不匹配：${DIALECT}" >&2
        exit 2
        ;;
esac

run_smoke_test
