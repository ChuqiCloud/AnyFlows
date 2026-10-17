# Docker Compose 部署

此方式会在服务器上从源码构建镜像，适合已有 Docker 运维环境的用户。首次构建包含 Rust 和两套前端，需要下载依赖并占用较多时间、内存和磁盘。只想运行服务时，优先使用 [二进制包](deployment-and-configuration.md)。

## 安装

准备 Linux、Git、OpenSSL、Docker Engine 和 Docker Compose 插件。仓库 Compose 使用 Linux host 网络，应用、PostgreSQL 和 Redis 分别占用本机回环端口 8080、5432、6379，先确认没有冲突。

```bash
git clone https://github.com/ChuqiCloud/AnyFlows.git
cd AnyFlows
./scripts/deploy/up.sh
```

首次运行会生成 `.env`，内含随机数据库密码和应用密钥，然后构建并启动服务。已有 `.env` 不会被覆盖。此处的 `anyflows:local` 是本机构建的镜像名称，不是可直接下载的公共镜像。

```bash
docker compose --env-file .env ps
docker compose --env-file .env logs -f app
curl --fail http://127.0.0.1:8080/readyz
```

服务只监听 `127.0.0.1:8080`。按 [二进制教程中的初始化和域名步骤](deployment-and-configuration.md#4-创建管理员)，通过 SSH 隧道创建管理员，再配置同机反向代理和 HTTPS。

## 配置与数据

`.env` 需要妥善备份，不要提交到 Git。修改环境变量后执行：

```bash
docker compose --env-file .env up -d
```

单独执行 `restart` 不会将新环境变量注入已有容器。Compose 的 `environment` 字段优先于 `env_file`；数据库地址等字段已在 Compose 中指定，需要结合 [配置参考](configuration.md) 一起修改。

PostgreSQL、Redis 和计费 WAL 分别保存在命名卷中。查看实际卷名：

```bash
docker compose --env-file .env config --volumes
```

修改已有数据库的密码时，还需更新数据库内的账号密码；只修改 `.env` 不会重置已有数据库。

## 停止和升级

停止服务并保留数据：

```bash
docker compose --env-file .env down
```

请勿添加 `-v`；该选项会删除命名卷。升级前备份配置、数据库及 WAL，再获取目标版本源码并重新构建：

```bash
docker compose --env-file .env up -d --build
docker compose --env-file .env ps
curl --fail http://127.0.0.1:8080/readyz
```

数据库迁移和回滚限制见 [升级与备份](migrations-and-release.md)。本文件中的 Compose 用于单机；跨主机部署按 [多节点部署](multi-node-deployment.md) 规划共享依赖和网络。