#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root_dir=$(CDPATH= cd -- "$script_dir/../.." && pwd)
env_file="$root_dir/.env"

if [ -e "$env_file" ]; then
    printf '%s\n' "配置文件 .env 已存在，为避免覆盖密钥而停止。"
    exit 1
fi
if ! command -v openssl >/dev/null 2>&1; then
    printf '%s\n' "缺少 openssl，无法安全生成部署密钥。" >&2
    exit 1
fi

generate_key() {
    openssl rand -base64 32 | tr '+/' '-_' | tr -d '='
}

database_password=$(openssl rand -hex 24)
session_signing_key=$(generate_key)
credential_encryption_key=$(generate_key)

{
    printf '%s\n' '# AnyFlows Docker 部署密钥；权限应保持为 0600。'
    printf 'POSTGRES_PASSWORD=%s\n' "$database_password"
    printf 'AF_AUTH__SESSION_SIGNING_KEY=%s\n' "$session_signing_key"
    printf 'AF_CREDENTIAL_ENCRYPTION__KEY_ID=primary\n'
    printf 'AF_CREDENTIAL_ENCRYPTION__KEY=%s\n' "$credential_encryption_key"
    printf 'AF_TELEMETRY__LEVEL=info\n'
} > "$env_file"
chmod 600 "$env_file"
printf '已生成 %s\n' "$env_file"
