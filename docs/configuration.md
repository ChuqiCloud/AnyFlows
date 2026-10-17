# 配置参考

单机二进制安装通常只需使用安装器生成的配置。域名、注册、厂商目录、实名认证等可在后台管理的设置，优先通过后台调整。

## 配置从哪里读取

| 启动方式 | 主要配置位置 |
| --- | --- |
| systemd 安装 | `/etc/anyflows/anyflows.env`，由服务单元加载 |
| Docker Compose | 仓库根目录 `.env`，以及 Compose 的 `environment` 字段 |
| 手工启动 | 当前目录的 `.env`、TOML 文件和进程环境变量 |

程序默认查找 `anyflows.conf`，再兼容 `anyflows.toml`；可用 `AF_CONFIG_FILE` 指定 TOML 路径。环境变量覆盖 TOML，同名变量以进程环境为准，当前工作目录的 `.env` 只补充未设置的变量。环境文件解析失败会阻止启动。

`.env` 是 `KEY=VALUE` 格式，不能当作 TOML 传给 `AF_CONFIG_FILE`。`anyflows.conf` 中的开发示例密钥必须替换后才能用于生产；systemd 安装器会自动生成随机密钥。

完整环境变量示例见 [.env.example](../.env.example)。修改启动配置后需重启服务，多节点应更新每个节点。

## 常用参数

| 变量 | 用途 |
| --- | --- |
| `AF_SERVER__BIND` | 监听地址，默认 `127.0.0.1:8080`，只接受回环地址 |
| `AF_DATABASE__URL` | SQLite、PostgreSQL 或 MySQL 连接地址 |
| `AF_REDIS__URL` | Redis 连接地址，用于多节点共享限流、调度状态及 OAuth 刷新租约等 |
| `AF_REDIS__REQUEST_RATE_LIMIT_NAMESPACE` | 请求限流键空间，同一集群保持一致 |
| `AF_AUTH__SESSION_SIGNING_KEY` | 后台登录会话签名密钥 |
| `AF_CREDENTIAL_ENCRYPTION__KEY_ID` | 渠道凭据加密密钥标识 |
| `AF_CREDENTIAL_ENCRYPTION__KEY` | 渠道凭据加密密钥 |
| `AF_BILLING__WAL_DIRECTORY` | 当前实例独占的计费恢复日志目录 |
| `AF_TELEMETRY__LEVEL` | 运行日志级别，通常使用 `info` |
| `AF_HTTP_CLIENT__PROXY_URL` | 访问上游时使用的 HTTP(S)、SOCKS5 或 SOCKS5H 代理 |
| `AF_SERVER__SHUTDOWN_TIMEOUT_SECS` | 停止时等待在途请求结束的时间，默认 30 秒，范围 1–300 秒 |

程序不从系统代理环境变量推断上游代理，请显式配置 `AF_HTTP_CLIENT__PROXY_URL`。

## 密钥与备份

会话签名密钥和凭据加密密钥分别需要 32 字节随机值，以 Base64URL 无填充格式保存。安装器已生成；手工部署可为每把密钥分别运行：

```bash
openssl rand -base64 32 | tr '+/' '-_' | tr -d '='
```

密钥只写入受限配置或密钥管理系统。systemd 配置默认为 `root:anyflows`、权限 `0640`。备份数据库时同时保留加密密钥和密钥标识；丢失它们会导致已保存的渠道凭据无法解密。多节点必须使用相同的签名密钥和加密密钥。

不要通过重新运行初始化脚本来替换已有密钥。轮换前先确认旧数据的解密和迁移方案。

## 代理与客户端地址

默认按 TCP 对端地址识别客户端。同机反向代理下，这通常是回环地址。需要按访客真实 IP 限流或执行 API Key 的 IP 白名单时，可配置：

```dotenv
AF_SERVER__CLIENT_IP_SOURCE=x-forwarded-for
AF_SERVER__TRUSTED_PROXY_CIDRS=["127.0.0.1/32"]
```

代理必须正确重建或清洗 `X-Forwarded-For`。仅信任实际使用的代理地址，不能填 `0.0.0.0/0`。上例信任同机回环进程；有多层代理时应结合 [多节点部署](multi-node-deployment.md) 配置完整可信链。

浏览器跨域默认关闭。确需前后端分域时，通过 `AF_SERVER__CORS_ALLOWED_ORIGINS` 设置完整 Origin 的 JSON 数组，例如 `["https://console.example.com"]`，不使用通配符或带路径的地址。同源部署无需设置。

## 后台设置与可选服务

- **实名认证**：在后台「实名认证配置」分别管理个人、企业主体的方式和说明开关。支付宝应用密钥在线加密保存；首次在线保存前可用 `AF_ACCOUNT_VERIFICATION__ALIPAY__*` 作为启动回退配置，在线保存后以后台设置为准。由环境变量转为在线配置时需重新输入密钥。
- **上游 OAuth**：Codex 使用内置客户端。其他需要配置的客户端材料见 `AF_OAUTH__*`；账号连接与用户登录 OAuth 是两项功能。
- **渠道探活**：`AF_CHANNEL_PROBE__*` 控制自动探活。探活可能产生真实上游调用和费用。
- **支付**：Stripe 密钥等启动材料使用 `AF_PAYMENT__*`，其他可在线管理的支付设置通过后台填写。
- **ClickHouse**：`AF_CLICKHOUSE_ANALYTICS__*` 用于可选分析读取，`AF_CLICKHOUSE_EXPORT__*` 用于异步事实投递。不配置时保留事务数据库路径；读写使用不同账号。

具体字段、范围及默认值以当前版本的 [.env.example](../.env.example) 为准。