#!/usr/bin/env bash
set -euo pipefail

readonly SERVICE_NAME="anyflows"
readonly SERVICE_UNIT="${SERVICE_NAME}.service"
readonly INSTALL_DIR="/usr/local/libexec/${SERVICE_NAME}"
readonly BINARY_PATH="${INSTALL_DIR}/${SERVICE_NAME}"
readonly PREVIOUS_BINARY_PATH="${INSTALL_DIR}/${SERVICE_NAME}.previous"
readonly CONFIG_DIR="/etc/${SERVICE_NAME}"
readonly CONFIG_PATH="${CONFIG_DIR}/${SERVICE_NAME}.env"
readonly EXAMPLE_CONFIG_PATH="${CONFIG_DIR}/${SERVICE_NAME}.env.example"
readonly DATA_DIR="/var/lib/${SERVICE_NAME}"
readonly WAL_DIR="${DATA_DIR}/billing-wal"

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
package_binary="${script_dir}/${SERVICE_NAME}"
package_version="${script_dir}/VERSION"
package_unit="${script_dir}/${SERVICE_UNIT}"
package_example="${script_dir}/.env.example"

fail() {
    printf 'AnyFlows systemd 部署失败：%s\n' "$*" >&2
    exit 1
}

require_root() {
    [[ "$(id -u)" -eq 0 ]] || fail "请使用 root 或 sudo 执行"
}

require_commands() {
    local command_name
    for command_name in systemctl openssl curl install; do
        command -v "$command_name" >/dev/null 2>&1 || fail "缺少命令 $command_name"
    done
}

validate_package() {
    [[ -x "$package_binary" ]] || fail "归档中缺少可执行文件 ${SERVICE_NAME}"
    [[ -f "$package_version" ]] || fail "归档中缺少 VERSION"
    [[ -f "$package_unit" ]] || fail "归档中缺少 ${SERVICE_UNIT}"
    [[ -f "$package_example" ]] || fail "归档中缺少 .env.example"
    grep -Eq '^tag=v[0-9]+\.[0-9]+\.[0-9]+' "$package_version" \
        || fail "VERSION 中的版本标签格式无效"
}

ensure_service_account() {
    getent group "$SERVICE_NAME" >/dev/null 2>&1 || groupadd --system "$SERVICE_NAME"
    id -u "$SERVICE_NAME" >/dev/null 2>&1 || useradd --system \
        --gid "$SERVICE_NAME" --home-dir "$DATA_DIR" --create-home \
        --shell /usr/sbin/nologin "$SERVICE_NAME"
}

generate_key() {
    openssl rand -base64 32 | tr '+/' '-_' | tr -d '='
}

install_initial_config() {
    install -d -o root -g "$SERVICE_NAME" -m 0750 "$CONFIG_DIR"
    if [[ ! -e "$EXAMPLE_CONFIG_PATH" ]]; then
        install -o root -g root -m 0644 "$package_example" "$EXAMPLE_CONFIG_PATH"
    fi
    if [[ -e "$CONFIG_PATH" ]]; then
        [[ -f "$CONFIG_PATH" ]] || fail "配置路径不是普通文件"
    else
        local temporary_config
        temporary_config=$(mktemp "${CONFIG_DIR}/.anyflows.env.XXXXXX")
        trap 'rm -f -- "$temporary_config"' RETURN
        umask 077
        {
            printf '%s\n' '# AnyFlows systemd 初始配置；请按部署环境修改数据库和上游设置。'
            printf 'AF_SERVER__BIND=127.0.0.1:8080\n'
            printf 'AF_DATABASE__URL=sqlite:///var/lib/anyflows/anyflows.db?mode=rwc\n'
            printf 'AF_AUTH__SESSION_SIGNING_KEY=%s\n' "$(generate_key)"
            printf 'AF_CREDENTIAL_ENCRYPTION__KEY_ID=primary\n'
            printf 'AF_CREDENTIAL_ENCRYPTION__KEY=%s\n' "$(generate_key)"
            printf 'AF_BILLING__WAL_DIRECTORY=%s\n' "$WAL_DIR"
            printf 'AF_TELEMETRY__LEVEL=info\n'
        } > "$temporary_config"
        install -o root -g "$SERVICE_NAME" -m 0640 "$temporary_config" "$CONFIG_PATH"
        rm -f -- "$temporary_config"
        trap - RETURN
        printf '已生成初始配置 %s\n' "$CONFIG_PATH"
    fi
    chown root:"$SERVICE_NAME" "$CONFIG_PATH"
    chmod 0640 "$CONFIG_PATH"
}

prepare_directories() {
    install -d -o "$SERVICE_NAME" -g "$SERVICE_NAME" -m 0750 "$DATA_DIR"
    install -d -o "$SERVICE_NAME" -g "$SERVICE_NAME" -m 0700 "$WAL_DIR"
    install -d -o root -g root -m 0755 "$INSTALL_DIR"
}

install_binary() {
    if [[ -x "$BINARY_PATH" ]]; then
        install -o root -g "$SERVICE_NAME" -m 0755 "$BINARY_PATH" "$PREVIOUS_BINARY_PATH"
    fi
    local temporary_binary
    temporary_binary=$(mktemp "${INSTALL_DIR}/.anyflows.binary.XXXXXX")
    install -o "$SERVICE_NAME" -g "$SERVICE_NAME" -m 0755 "$package_binary" "$temporary_binary"
    mv -f -- "$temporary_binary" "$BINARY_PATH"
}

wait_until_ready() {
    local attempt
    for attempt in $(seq 1 30); do
        if curl --fail --silent --show-error --max-time 2 \
            http://127.0.0.1:8080/readyz >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    return 1
}

install_unit() {
    install -o root -g root -m 0644 "$package_unit" "/etc/systemd/system/$SERVICE_UNIT"
    systemctl daemon-reload
    systemctl enable "$SERVICE_UNIT" >/dev/null
}

restore_after_failed_upgrade() {
    [[ -f "$PREVIOUS_BINARY_PATH" ]] || return 1
    install -o "$SERVICE_NAME" -g "$SERVICE_NAME" -m 0755 \
        "$PREVIOUS_BINARY_PATH" "$BINARY_PATH"
    systemctl restart "$SERVICE_UNIT" || return 1
    wait_until_ready
}

deploy() {
    local had_previous_binary=0
    [[ -x "$BINARY_PATH" ]] && had_previous_binary=1
    install_binary
    install_unit
    if systemctl restart "$SERVICE_UNIT" && wait_until_ready; then
        printf 'AnyFlows %s 已安装并通过 /readyz。\n' "$(sed -n 's/^tag=//p' "$package_version" | head -n 1)"
        return 0
    fi
    if [[ "$had_previous_binary" -eq 1 ]] && restore_after_failed_upgrade; then
        fail "新版本未通过健康检查，已自动回滚上一版本"
    fi
    systemctl stop "$SERVICE_UNIT" >/dev/null 2>&1 || true
    fail "服务未通过健康检查，且没有可用的上一版本"
}

rollback() {
    [[ -f "$PREVIOUS_BINARY_PATH" ]] || fail "没有可回滚的上一版本"
    local temporary_binary
    temporary_binary=$(mktemp "${INSTALL_DIR}/.anyflows.rollback.XXXXXX")
    install -o root -g "$SERVICE_NAME" -m 0755 "$BINARY_PATH" "$temporary_binary"
    install -o "$SERVICE_NAME" -g "$SERVICE_NAME" -m 0755 "$PREVIOUS_BINARY_PATH" "$BINARY_PATH"
    install -o root -g "$SERVICE_NAME" -m 0755 "$temporary_binary" "$PREVIOUS_BINARY_PATH"
    rm -f -- "$temporary_binary"
    systemctl restart "$SERVICE_UNIT"
    wait_until_ready || fail "回滚版本未通过健康检查"
    printf 'AnyFlows 已回滚并通过 /readyz。\n'
}

usage() {
    printf '%s\n' \
        "用法：sudo ./install.sh [install|upgrade|rollback|start|stop|restart|status]" \
        "install/upgrade 安装当前归档并在健康检查失败时自动回滚。"
}

main() {
    local action="${1:-install}"
    require_root
    require_commands
    if [[ "$action" == "status" ]]; then
        systemctl --no-pager status "$SERVICE_UNIT"
        return
    fi
    if [[ "$action" == "start" || "$action" == "stop" || "$action" == "restart" ]]; then
        systemctl "$action" "$SERVICE_UNIT"
        [[ "$action" != "restart" ]] || wait_until_ready || fail "重启后未通过健康检查"
        return
    fi
    validate_package
    ensure_service_account
    prepare_directories
    install_initial_config
    case "$action" in
        install|upgrade) deploy ;;
        rollback) rollback ;;
        *) usage; exit 2 ;;
    esac
}

main "$@"
