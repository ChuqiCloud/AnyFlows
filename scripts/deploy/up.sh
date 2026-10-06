#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root_dir=$(CDPATH= cd -- "$script_dir/../.." && pwd)
env_file="$root_dir/.env"

case "$(uname -s)" in
    Linux) ;;
    *)
        printf '%s\n' "当前 Compose 首切片使用 Linux host 网络，仅支持 Linux 主机。" >&2
        exit 1
        ;;
esac
if ! command -v docker >/dev/null 2>&1; then
    printf '%s\n' "缺少 docker，请先安装 Docker Engine。" >&2
    exit 1
fi
if ! docker compose version >/dev/null 2>&1; then
    printf '%s\n' "缺少 Docker Compose 插件，请先安装 docker compose。" >&2
    exit 1
fi

if [ ! -f "$env_file" ]; then
    "$script_dir/init-env.sh"
fi

cd "$root_dir"
docker compose --env-file "$env_file" config --quiet
docker compose --env-file "$env_file" up -d --build
docker compose --env-file "$env_file" ps
