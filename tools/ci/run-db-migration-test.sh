#!/usr/bin/env bash
set -Eeuo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/container-test-runner.sh"

readonly DIALECT="${1:-}"
readonly POSTGRES_IMAGE="docker.io/library/postgres:17.10-bookworm@sha256:67870dc097790edf2bd6726658db995dcc830f799d41bb2b78ef07c9a2d5f010"
readonly MYSQL_IMAGE="docker.io/library/mysql:8.4.10@sha256:6ec1dc148edc412cce3ae27dd5b78dcd4e026976648424b3d31d9a8c6204db4e"
readonly TEST_USER="anyflows"
readonly TEST_PASSWORD="anyflows_ci"
readonly TEST_DATABASE="anyflows_migration_test_ci"
readonly SMOKE_TEST_NAME="migration::tests::migration_runner_smoke"

container_name=""
network_name=""
runner_name=""

cleanup() {
    local status=$?
    trap - EXIT
    if [[ -n "${runner_name}" ]]; then
        docker rm --force "${runner_name}" >/dev/null 2>&1 || true
    fi
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

run_smoke_test() {
    local test_list=""
    local test_binary=""

    test_list="$(cargo test --locked --package af-db --lib -- --list)"
    if ! grep --fixed-strings --line-regexp --quiet "${SMOKE_TEST_NAME}: test" <<<"${test_list}"; then
        echo "未发现必需的数据库迁移冒烟测试：${SMOKE_TEST_NAME}" >&2
        return 1
    fi

    if [[ -n "${network_name}" ]]; then
        test_binary="$(ci_build_test_binary af-db lib)"
        runner_name="anyflows-db-test-${BASHPID}"
        ci_run_test_binary "${network_name}" "${runner_name}" "${test_binary}" \
            "AF_TEST_DATABASE_URL=${database_url}" \
            "AF_REQUIRE_LIVE_DATABASE=1" \
            "AF_ALLOW_DESTRUCTIVE_MIGRATION_TEST=1" \
            -- "${SMOKE_TEST_NAME}" --exact --include-ignored
        return
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
            network_name="anyflows-postgres-network-${BASHPID}"
            container_name="anyflows-postgres-${BASHPID}"
            docker network create "${network_name}" >/dev/null
            docker run --detach --name "${container_name}" --network "${network_name}" \
                --network-alias postgres \
                --env POSTGRES_USER="${TEST_USER}" \
                --env POSTGRES_PASSWORD="${TEST_PASSWORD}" \
                --env POSTGRES_DB="${TEST_DATABASE}" \
                "${POSTGRES_IMAGE}" >/dev/null
            wait_for_postgres
            database_url="postgres://${TEST_USER}:${TEST_PASSWORD}@postgres:5432/${TEST_DATABASE}"
            ;;
        mysql)
            network_name="anyflows-mysql-network-${BASHPID}"
            container_name="anyflows-mysql-${BASHPID}"
            docker network create "${network_name}" >/dev/null
            docker run --detach --name "${container_name}" --network "${network_name}" \
                --network-alias mysql \
                --env MYSQL_USER="${TEST_USER}" \
                --env MYSQL_PASSWORD="${TEST_PASSWORD}" \
                --env MYSQL_ROOT_PASSWORD="root_ci" \
                --env MYSQL_DATABASE="${TEST_DATABASE}" \
                "${MYSQL_IMAGE}" --log-bin-trust-function-creators=1 >/dev/null
            wait_for_mysql
            database_url="mysql://${TEST_USER}:${TEST_PASSWORD}@mysql:3306/${TEST_DATABASE}?timezone=%2B08:00"
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
