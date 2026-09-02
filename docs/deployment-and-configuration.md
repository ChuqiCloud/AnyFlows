# 部署与配置参考

本文说明 AnyFlows 当前支持的 Linux 部署方式和关键配置边界。部署前请复制仓库根目录的 `.env.example`，只在部署机上填写密钥；不要把生产 `.env` 提交到 Git、写入镜像或粘贴到日志。

## 选择部署方式

| 方式 | 适用场景 | 数据位置 | 公网边界 |
| --- | --- | --- | --- |
| Docker Compose | 快速部署、容器化运维 | PostgreSQL、Redis 和应用 WAL 命名卷 | Compose 使用 host 网络，应用和依赖只监听宿主机回环 |
| systemd | Linux amd64 单机、直接使用发布归档 | `/var/lib/anyflows`，SQLite 与计费 WAL 位于其中 | 服务只监听 `127.0.0.1:8080` |

两种方式都需要由宿主机上的 Nginx、Caddy 或其他反向代理承担公网 TLS、访问控制和请求体策略。当前首切片不直接发布公网端口，也不支持 Docker Compose 的跨主机桥接网络。

## Docker Compose

前置条件：Linux、Docker Engine、Docker Compose 插件和 OpenSSL。

在仓库根目录执行：

```bash
./scripts/deploy/up.sh
curl --fail http://127.0.0.1:8080/healthz
docker compose --env-file .env ps
docker compose --env-file .env logs -f app
```

`up.sh` 在 `.env` 不存在时调用 `init-env.sh`，生成权限为 `0600` 的配置，包含随机 PostgreSQL 密码、管理会话签名密钥和凭据加密密钥。脚本拒绝覆盖已有 `.env`；修改配置前应先备份并确认文件权限。

停止应用但保留数据：

```bash
docker compose --env-file .env down
```

删除命名卷会删除数据库、Redis 或计费 WAL 数据。执行前必须完成数据库和 WAL 备份，并确认没有在途请求。

## systemd

前置条件：Linux amd64、root 或 sudo、`systemctl`、`openssl`、`curl`、`install`。从 Linux amd64 发布归档解压后，在归档目录执行：

```bash
sudo ./install.sh install
sudo ./install.sh status
sudo ./install.sh upgrade
sudo ./install.sh rollback
```

安装脚本会创建专用 `anyflows` 用户和组，并把配置写入 `/etc/anyflows/anyflows.env`，把 SQLite 数据和计费 WAL 写入 `/var/lib/anyflows`。配置文件权限为 `0640`，所有者为 `root:anyflows`；已有配置不会被覆盖。

`install` 和 `upgrade` 在替换二进制后重启服务，并在 `127.0.0.1:8080/readyz` 上等待最多 30 秒。新版本未就绪时，若存在上一版本，脚本会自动恢复上一份二进制；`rollback` 也会交换当前与上一版本并重新执行就绪检查。

查看服务日志：

```bash
sudo journalctl -u anyflows -f
curl --fail http://127.0.0.1:8080/readyz
```

## 环境变量

所有变量使用 `AF_` 前缀和双下划线层级命名。完整示例与默认值以 `.env.example` 为准；下面只列出部署时最容易出错的项目。

| 变量 | 作用 | 注意事项 |
| --- | --- | --- |
| `AF_SERVER__BIND` | 服务监听地址 | 生产保持 `127.0.0.1:8080`，不要改为公网地址 |
| `AF_DATABASE__URL` | 数据库连接 | Docker 使用 Compose 注入的 PostgreSQL；systemd 默认使用 `/var/lib/anyflows/anyflows.db` |
| `AF_REDIS__URL` | Redis 连接 | OAuth 刷新、请求限流和公开 SSO 启动防护依赖 Redis；公开 SSO 限流在 Redis 不可用时失败关闭 |
| `AF_AUTH__SESSION_SIGNING_KEY` | 管理会话签名 | 必须是 32 字节随机值的 Base64URL 无填充文本，泄露后需轮换并重新登录 |
| `AF_CREDENTIAL_ENCRYPTION__KEY_ID` / `AF_CREDENTIAL_ENCRYPTION__KEY` | 渠道凭据加密 | ID 与 32 字节 Base64URL 无填充密钥必须成对配置；轮换时保留旧密钥的受控解密能力 |
| `AF_BILLING__WAL_DIRECTORY` | 计费 WAL 目录 | 每个实例必须独占目录，并纳入备份和磁盘监控 |
| `AF_HTTP_CLIENT__PROXY_URL` | 上游代理 | 只填写受控 HTTP(S)、SOCKS5 或 SOCKS5H 代理；运行时不读取系统代理环境变量 |
| `AF_OAUTH__*` | 上游订阅账号 OAuth 客户端材料 | 仅从启动配置读取；未配置的 Provider 不注册，密钥不得写入数据库或前端 |
| `AF_PAYMENT__*` | Stripe 支付与回执 | 服务端密钥和 webhook 密钥只通过部署密钥管理注入；Epay 人工退款不使用自动退款密钥 |

配置变更通常需要重启应用。不要在运行中的容器或服务内直接编辑配置，也不要把 `AF_AUTH__SESSION_SIGNING_KEY`、`AF_CREDENTIAL_ENCRYPTION__KEY`、支付密钥或 OAuth client secret 放入命令历史。

## 健康检查与排障

- `/healthz` 只表示进程可以响应请求。
- `/readyz` 还会检查应用依赖是否在就绪截止时间内可用，升级和回滚以它为准。
- Docker 使用 `docker compose --env-file .env logs -f app` 查看应用日志；systemd 使用 `journalctl -u anyflows`。
- 看到 `503` 时先检查数据库、Redis、密钥配置和磁盘空间；不要通过关闭鉴权或把服务绑定到公网地址绕过故障。
- 公开企业 SSO 启动会对客户端 IP 与规范化标识执行 Redis 原子组合限流。系统不保存原始 IP、邮箱或企业公开标识；Redis 缺失或故障时该入口保持失败关闭。

## 备份与升级顺序

1. 备份 `.env` 或 `/etc/anyflows/anyflows.env`，并限制备份介质访问权限。
2. 备份 PostgreSQL/SQLite 数据、计费 WAL 和需要保留的 Redis 数据；确认备份可读取。
3. 先在隔离环境验证新发布归档的 `/healthz` 与 `/readyz`，再执行 Compose `up -d --build` 或 systemd `upgrade`。
4. 升级后检查服务日志、就绪探针、数据库迁移结果和一条受控管理请求。
5. 若 systemd 升级未通过就绪检查，脚本会自动回滚；Compose 应停止新容器并恢复上一镜像，同时保留数据卷，禁止直接删除卷排障。

数据库迁移属于应用启动流程的一部分。升级前仍应保留可恢复备份，并在三方言生产环境分别验证迁移和回滚方案；不要手工修改迁移历史表。
