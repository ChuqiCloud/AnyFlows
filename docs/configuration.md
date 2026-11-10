# 配置参考

本文件说明启动配置、密钥及代理设置。域名、注册、厂商目录、实名认证等在线设置通过后台管理。

## 配置来源

| 启动方式 | 主要配置位置 |
| --- | --- |
| 本文档的 systemd 部署 | `/etc/anyflows/anyflows.env`，由服务单元加载 |
| Docker Compose | 仓库根目录 `.env`，以及 Compose 的 `environment` 字段 |
| 手工启动 | 当前目录的 `.env`、TOML 文件和进程环境变量 |

程序按以下顺序读取配置：

1. 加载当前工作目录的 `.env`，仅补充尚未设置的进程环境变量。
2. 读取 `AF_CONFIG_FILE` 指定的 TOML；未指定时查找 `anyflows.conf`，不存在时兼容读取 `anyflows.toml`。
3. 用 `AF_` 环境变量覆盖 TOML 配置。

`.env` 使用 `KEY=VALUE` 格式，不能作为 TOML 传给 `AF_CONFIG_FILE`。环境文件解析失败或 `AF_` 配置含未知字段时，程序拒绝启动。生产部署须替换配置中的示例密钥和占位值。

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
| `AF_IMAGE_STORAGE__DIRECTORY` | 生图文件存储目录，默认 `data/images` |
| `AF_TELEMETRY__LEVEL` | 运行日志级别，通常使用 `info` |
| `AF_HTTP_CLIENT__PROXY_URL` | 访问上游时使用的 HTTP(S)、SOCKS5 或 SOCKS5H 代理 |
| `AF_SERVER__SHUTDOWN_TIMEOUT_SECS` | 停止时等待在途请求结束的时间，默认 30 秒，范围 1–300 秒 |

程序不从系统代理环境变量推断上游代理，请显式配置 `AF_HTTP_CLIENT__PROXY_URL`。

生图目录可使用相对或绝对路径；相对路径以进程工作目录为基准。部署时建议显式配置绝对路径，并确保服务账号有读写权限。多节点部署必须让所有节点使用指向同一共享可写目录的配置，详见[多节点部署](multi-node-deployment.md)。

## 密钥与备份

会话签名密钥和凭据加密密钥分别使用独立的 32 字节随机值，以 Base64URL 无填充格式保存。生成方式与文件权限见 [二进制安装](deployment-and-configuration.md#3-安装并启动)。多节点使用相同的签名密钥和加密密钥。

已有部署保留原密钥，数据库备份同时保留加密密钥和标识；丢失后无法解密渠道凭据。轮换前确认解密和迁移方案，备份范围见 [升级与备份](migrations-and-release.md)。

## 代理与客户端地址

默认按 TCP 对端地址识别客户端，同机反向代理下通常为回环地址。按真实客户端 IP 限流或执行 API Key 的 IP 白名单时，配置：

```dotenv
AF_SERVER__CLIENT_IP_SOURCE=x-forwarded-for
AF_SERVER__TRUSTED_PROXY_CIDRS=["127.0.0.1/32"]
```

代理须重建或清洗 `X-Forwarded-For`，可信范围仅包含实际代理地址，不接受 `0.0.0.0/0`。上例信任同机回环进程；多层代理配置见 [多节点部署](multi-node-deployment.md)。

浏览器跨域默认关闭。前后端分域时，使用 `AF_SERVER__CORS_ALLOWED_ORIGINS` 指定完整 Origin 的 JSON 数组，如 `["https://console.example.com"]`，不含通配符或路径。同源部署无需设置。

## 后台设置与可选服务

- **实名认证**：在后台「实名认证配置」分别管理个人、企业主体的方式和说明开关。支付宝应用密钥在线加密保存；首次在线保存前可用 `AF_ACCOUNT_VERIFICATION__ALIPAY__*` 作为启动回退配置，在线保存后以后台设置为准。由环境变量转为在线配置时需重新输入密钥。
- **上游 OAuth**：Codex 使用内置客户端。其他需要配置的客户端材料见 `AF_OAUTH__*`；账号连接与用户登录 OAuth 是两项功能。
- **渠道探活**：`AF_CHANNEL_PROBE__*` 控制自动探活。探活可能产生真实上游调用和费用。
- **支付**：Stripe 密钥等启动材料使用 `AF_PAYMENT__*`，其他可在线管理的支付设置通过后台填写。
- **ClickHouse**：`AF_CLICKHOUSE_ANALYTICS__*` 用于分析读取，`AF_CLICKHOUSE_EXPORT__*` 用于异步导出用量和请求记录。两项均可选；读写使用不同账号。

具体字段、范围及默认值以当前版本的 [.env.example](../.env.example) 为准。
