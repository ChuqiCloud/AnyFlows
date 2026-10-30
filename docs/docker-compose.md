# Docker Compose 部署

仓库提供从源码构建的 Compose 配置，包含 AnyFlows、PostgreSQL 和 Redis。直接运行服务可使用 [二进制包](deployment-and-configuration.md)。

## 安装

准备 Linux、Git、OpenSSL、Docker Engine 和 Docker Compose 插件。Compose 使用 Linux host 网络，应用、PostgreSQL 和 Redis 分别监听本机回环端口 8080、5432、6379；这些端口须未被其他服务占用。

检出已发布版本，以下以 `v0.3.0-beta.6` 为例：

```bash
git clone --branch v0.3.0-beta.6 --depth 1 https://github.com/ChuqiCloud/AnyFlows.git
cd AnyFlows
```

生成数据库密码：

```bash
openssl rand -hex 24
```

分别为会话签名和凭据加密运行一次以下命令，生成两个独立密钥：

```bash
openssl rand -base64 32 | tr '+/' '-_' | tr -d '='
```

创建 `.env`，写入下列配置，并将占位值替换为生成的密码和密钥：

```bash
umask 077
touch .env
chmod 600 .env
vi .env
```

```dotenv
POSTGRES_PASSWORD=replace-with-database-password
AF_AUTH__SESSION_SIGNING_KEY=replace-with-session-signing-key
AF_CREDENTIAL_ENCRYPTION__KEY_ID=primary
AF_CREDENTIAL_ENCRYPTION__KEY=replace-with-credential-encryption-key
AF_TELEMETRY__LEVEL=info
```

检查配置、构建镜像并启动：

```bash
docker compose --env-file .env config --quiet
docker compose --env-file .env up -d --build
docker compose --env-file .env ps
docker compose --env-file .env logs --tail 100 app
curl --fail http://127.0.0.1:8080/readyz
```

镜像名 `anyflows:local` 指本机构建的镜像，不是公共镜像地址。服务只监听 `127.0.0.1:8080`；按 [管理员初始化和域名配置](deployment-and-configuration.md#4-创建管理员) 创建管理员，并配置同机反向代理和 HTTPS。

## 配置与数据

备份 `.env`，保持权限为 `0600`，不要提交到 Git。修改环境变量后重新创建容器：

```bash
docker compose --env-file .env up -d
```

`restart` 不会更新已有容器的环境变量。Compose 的 `environment` 字段优先于 `env_file`；数据库地址等已在 Compose 中指定的字段需修改对应配置，参见 [配置参考](configuration.md)。

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

`down -v` 会删除命名卷。升级前备份配置、数据库及 WAL，检出目标发布标签后重新构建：

```bash
docker compose --env-file .env up -d --build
docker compose --env-file .env ps
curl --fail http://127.0.0.1:8080/readyz
```

数据库迁移和回滚限制见 [升级与备份](migrations-and-release.md)。本文件中的 Compose 用于单机；跨主机部署按 [多节点部署](multi-node-deployment.md) 规划共享依赖和网络。
