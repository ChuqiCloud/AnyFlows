#!/usr/bin/env bash
set -Eeuo pipefail

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

test_build_container_network_mode() (
    docker() {
        [[ "${1:-}" == "inspect" && "${2:-}" == "build123" ]]
    }

    HOSTNAME="build123"
    source "${SCRIPT_DIR}/container-endpoint.sh"
    ci_prepare_service_endpoint 5432

    [[ "${CI_SERVICE_NETWORK_MODE}" == "build-container" ]]
    [[ "${CI_SERVICE_HOST}" == "127.0.0.1" ]]
    [[ "${CI_SERVICE_PORT}" == "5432" ]]
    [[ "${CI_SERVICE_DOCKER_ARGS[*]}" == "--network container:build123" ]]

    ci_tcp_reachable() {
        [[ "${1:-}" == "127.0.0.1" && "${2:-}" == "5432" ]]
    }
    ci_wait_for_service_endpoint "postgres-test" 5432 "PostgreSQL"
)

test_published_port_fallback() (
    docker() {
        return 1
    }

    HOSTNAME="missing"
    source "${SCRIPT_DIR}/container-endpoint.sh"
    ci_prepare_service_endpoint 6379

    [[ "${CI_SERVICE_NETWORK_MODE}" == "published" ]]
    [[ -z "${CI_SERVICE_HOST}" ]]
    [[ -z "${CI_SERVICE_PORT}" ]]
    [[ "${CI_SERVICE_DOCKER_ARGS[*]}" == "--publish 6379" ]]

    ci_wait_for_published_endpoint() {
        [[ "${1:-}" == "redis-test" ]]
        [[ "${2:-}" == "6379" ]]
        [[ "${3:-}" == "Redis" ]]
    }
    ci_wait_for_service_endpoint "redis-test" 6379 "Redis"
)

test_build_container_network_mode
test_published_port_fallback
