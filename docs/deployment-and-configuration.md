# 部署与配置参考

本文说明 AnyFlows 当前支持的 Linux 部署方式和关键配置边界。手工启动使用仓库根目录或 Release 归档内的 `anyflows.conf`（TOML），同时会自动读取当前工作目录的 `.env`（如果存在）；Docker/systemd 使用 `.env.example`（EnvironmentFile）。两类配置都只在部署机上填写密钥，不要把生产配置提交到 Git、写入镜像或粘贴到日志。

## 手工启动

Release 归档和仓库均提供完整的 `anyflows.conf`。设置 `AF_CONFIG_FILE`，再通过环境变量注入管理会话签名和凭据加密密钥：

```bash
export AF_CONFIG_FILE="$PWD/anyflows.conf"
export AF_AUTH__SESSION_SIGNING_KEY="$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')"
export AF_CREDENTIAL_ENCRYPTION__KEY_ID=primary
export AF_CREDENTIAL_ENCRYPTION__KEY="$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')"
./anyflows
```

`AF_CONFIG_FILE` 指向的文件必须是 TOML；`.env`/`.env.example` 是 `KEY=VALUE` 环境文件，不能直接改名后交给 `AF_CONFIG_FILE`。应用启动时会从当前工作目录读取 `.env`，文件不存在时忽略；同名的进程环境变量优先，适合用命令行、容器或 systemd 覆盖。`.env` 解析失败会阻止启动。数据库 URL、计费 WAL 目录和 `server.bind` 应按部署机修改。填写后的 `anyflows.conf` 和 `.env` 应限制为仅服务用户可读。

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

所有业务配置变量使用 `AF_` 前缀和双下划线层级命名。变量可以来自进程环境或当前工作目录的 `.env`；同名时进程环境优先。完整示例与默认值以 `.env.example` 为准；下面只列出部署时最容易出错的项目。

| 变量 | 作用 | 注意事项 |
| --- | --- | --- |
| `AF_SERVER__BIND` | 服务监听地址 | 生产保持 `127.0.0.1:8080`，不要改为公网地址 |
| `AF_DATABASE__URL` | 数据库连接 | Docker 使用 Compose 注入的 PostgreSQL；systemd 默认使用 `/var/lib/anyflows/anyflows.db` |
| `AF_REDIS__URL` | Redis 连接 | OAuth 刷新、请求限流和公开 SSO 启动防护依赖 Redis；公开 SSO 限流在 Redis 不可用时失败关闭 |
| `AF_REDIS__REQUEST_RATE_LIMIT_NAMESPACE` | 请求限流键空间 | 默认 `anyflows.gateway.rpm.v1`；多实例必须保持一致，变更会使旧固定窗口失效 |
| `AF_CLICKHOUSE_ANALYTICS__*` | 可选 ClickHouse 只读看板事实源 | 必须完整配置端点、单条参数化查询、只读账号、超时和响应上限；缺失时继续使用事务主库 |
| `AF_CLICKHOUSE_EXPORT__*` | 可选 ClickHouse 异步事实投递 | 与只读账号分离；查询只能是 `INSERT INTO`，主库 outbox 负责有界回填、重试和去重，失败不影响计费或请求终态 |
| `AF_AUTH__SESSION_SIGNING_KEY` | 管理会话签名 | 必须是 32 字节随机值的 Base64URL 无填充文本，泄露后需轮换并重新登录 |
| `AF_CREDENTIAL_ENCRYPTION__KEY_ID` / `AF_CREDENTIAL_ENCRYPTION__KEY` | 渠道凭据加密 | ID 与 32 字节 Base64URL 无填充密钥必须成对配置；轮换时保留旧密钥的受控解密能力 |
| `AF_BILLING__WAL_DIRECTORY` | 计费 WAL 目录 | 每个实例必须独占目录，并纳入备份和磁盘监控 |
| `AF_HTTP_CLIENT__PROXY_URL` | 上游代理 | 只填写受控 HTTP(S)、SOCKS5 或 SOCKS5H 代理；运行时不读取系统代理环境变量 |
| `AF_OAUTH__*` | 上游订阅账号 OAuth 客户端材料 | 仅从启动配置读取；未配置的 Provider 不注册，密钥不得写入数据库或前端 |
| `AF_PAYMENT__*` | Stripe 支付与回执 | 服务端密钥和 webhook 密钥只通过部署密钥管理注入；Epay 人工退款不使用自动退款密钥 |
| `AF_ACCOUNT_VERIFICATION__ALIPAY__*` | 支付宝实名认证启动回退配置 | 管理员首次在线保存前生效；之后以后台“企业认证 → 认证配置”为准。密钥不要写入仓库或日志 |
| `AF_OPENAI_UPSTREAM__*` | 旧版静态 OpenAI-compatible 配置 | 可选且三项必须同时存在，仅保留兼容解析；M1 生产 Chat 只使用数据库渠道，不把它作为回退目标 |

管理员可在“企业认证 → 认证配置”在线启用支付宝实名认证并设置应用 ID、应用私钥、支付宝公钥、网关、业务码及请求超时。人工审核不受该开关影响。后台密钥加密保存，读取接口只返回是否已配置；更新时留空表示保留后台原有密钥。首次从环境变量切换到在线配置时，需重新输入两把密钥，环境变量中的密钥不会复制到数据库。服务实例共享数据库配置；凭据加密密钥仍由部署环境提供，各实例须保持一致。

### ClickHouse 可选配置

ClickHouse 配置默认关闭，只有对应配置段完整存在时才会启用。分析读取和异步投递必须使用不同的账号与权限：前者只授予查询权限，后者只授予目标事实表的写入权限。端点必须是无凭据、无查询参数和无片段的 HTTP(S) 地址，密钥不得写入仓库、日志或镜像层。

`AF_CLICKHOUSE_ANALYTICS__QUERY` 是单条参数化查询，必须包含 `{period_start:Int64}` 与 `{period_end:Int64}`，返回一行与管理看板快照结构一致的 `JSONEachRow`。查询不得包含分号；超时默认 5 秒、上限 300 秒，响应体默认 1 MiB、上限 4 MiB。配置段缺失时，管理看板仍从事务主库读取，ClickHouse 不会覆盖当前渠道状态。

`AF_CLICKHOUSE_EXPORT__USAGE_INSERT_QUERY` 与 `AF_CLICKHOUSE_EXPORT__OUTCOME_INSERT_QUERY` 必须是单条 `INSERT INTO` 查询，运行时追加 `FORMAT JSONEachRow`、异步写入参数和去重 token。批量大小默认 64（上限 256），扫描周期默认 5 秒（上限 3600），请求超时默认 5 秒（上限 300），请求体默认 4 MiB（上限 16 MiB），历史回填默认 128 条（上限 256）。投递失败只会延迟 outbox 事实并触发受控重试，不回滚主库事实、计费结算或请求终态。

ClickHouse 查询超时、连接失败、响应超限或快照校验失败时，管理看板会返回统一的分析存储不可用状态，不展示部分或伪造数据；当前渠道状态仍从事务主库读取。上述故障不会阻断同步请求、计费、请求终态、权限判断或主库写入。恢复时先检查 ClickHouse `/ping`、只读账号和查询响应，再从管理员看板查看异步导出积压并执行有界重放；不要删除主库 outbox 事实或手工改写发布状态。

### 静态上游兼容配置

`AF_OPENAI_UPSTREAM__BASE_URL`、`AF_OPENAI_UPSTREAM__MODEL` 和 `AF_OPENAI_UPSTREAM__API_KEY` 仅用于旧版嵌入式配置的兼容解析，三项必须同时提供。M1 生产路径不会读取这些字段进行选路，也不会在数据库渠道不可用时回退到静态目标；新部署应优先配置数据库渠道与加密凭据。

配置变更通常需要重启应用；修改 `.env` 后也必须重启，运行中的进程不会动态重读。Docker 的 `docker compose --env-file .env` 会先由 Compose 读取变量并注入容器，systemd 则通过 `EnvironmentFile` 注入；手工启动时由 AnyFlows 自己读取 `.env`。不要在运行中的容器或服务内直接编辑配置，也不要把 `AF_AUTH__SESSION_SIGNING_KEY`、`AF_CREDENTIAL_ENCRYPTION__KEY`、支付密钥或 OAuth client secret 放入命令历史。

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
