# 多节点部署

多个应用节点通过负载均衡分担请求，共享数据库和 Redis。各节点使用相同版本及密钥，分别保留本地计费恢复日志。

## 部署结构

```mermaid
flowchart LR
    Client[客户端] --> LB[HTTPS 负载均衡]
    LB --> ProxyA[节点 A 内网反向代理]
    LB --> ProxyB[节点 B 内网反向代理]
    ProxyA --> AppA[节点 A 127.0.0.1:8080]
    ProxyB --> AppB[节点 B 127.0.0.1:8080]
    AppA --> DB[(共享 PostgreSQL / MySQL)]
    AppB --> DB
    AppA --> Redis[(共享 Redis)]
    AppB --> Redis
    AppA --> WalA[节点 A 独立 WAL]
    AppB --> WalB[节点 B 独立 WAL]
```

AnyFlows 只接受回环监听。每个应用节点配置同机代理，负载均衡连接节点代理的内网端口。

## 1. 准备共享依赖

- 使用同一套 PostgreSQL 或 MySQL 数据库。SQLite 不支持此部署结构，包括通过 NFS 共享数据库文件的方式。
- 使用同一个 Redis 实例和逻辑数据库，共享限流、并发控制、账号调度状态和 OAuth 刷新租约。不同环境使用独立 Redis 实例或逻辑库；请求限流 namespace 仅隔离限流键。
- 每个节点使用同一版本程序、相同的会话签名密钥、凭据加密密钥与标识、请求限流 namespace，以及一致的对外站点地址。
- 每个节点使用独立持久数据目录和 WAL 目录。故障节点的磁盘及 WAL 需保留，用于恢复尚未完成的计费记录。
- 使用外部前端模板时，将相同版本的模板文件分发到所有节点，并分别重新扫描。

数据库和 Redis 仅允许应用节点访问，账号按需授权；两者的备份与高可用单独配置。

管理员可在控制台「运行状态」查看各节点的进程负载、依赖状态和最近 10 分钟趋势。节点信息通过共享 Redis 汇总，详见 [运行状态](runtime-status.md)。

## 2. 配置第一个节点

按 [二进制安装](deployment-and-configuration.md#3-安装并启动) 创建账号、目录并安装程序及服务单元，在启动服务前写入共享依赖和密钥配置。下面列出各节点的主要配置，地址、密码和密钥均需替换：

```dotenv
AF_SERVER__BIND=127.0.0.1:8080
AF_DATABASE__URL=postgres://anyflows:数据库密码@10.0.0.10:5432/anyflows
AF_REDIS__URL=redis://:Redis密码@10.0.0.11:6379/0
AF_AUTH__SESSION_SIGNING_KEY=所有节点相同的32字节Base64URL密钥
AF_CREDENTIAL_ENCRYPTION__KEY_ID=primary
AF_CREDENTIAL_ENCRYPTION__KEY=所有节点相同的32字节Base64URL密钥
AF_REDIS__REQUEST_RATE_LIMIT_NAMESPACE=anyflows.gateway.rpm.v1
AF_BILLING__WAL_DIRECTORY=/var/lib/anyflows/billing-wal
```

MySQL 连接格式为 `mysql://用户:密码@主机:3306/数据库`。连接 URL 中密码的特殊字符需进行 URL 编码。

先仅启动第一个节点，等待数据库迁移和就绪检查完成，再通过 SSH 隧道创建管理员。数据库迁移须由单个节点执行：

```bash
sudo systemctl start anyflows
curl --fail http://127.0.0.1:8080/readyz
```

空数据库只需初始化一次。更改连接 URL 不会迁移已有 SQLite 数据；数据库类型切换需另行迁移数据，并验证完整性。

## 3. 加入其他节点

在其他服务器安装相同版本，启动前通过受限配置文件或密钥管理系统写入相同的数据库、Redis 连接信息和密钥。

各节点可使用相同的 WAL 路径名称，如 `/var/lib/anyflows/billing-wal`，但目录必须由各实例独占。逐个启动并检查 `/readyz`，确认无迁移、解密或 Redis 错误后加入负载均衡。

## 4. 配置代理和负载均衡

以下示例中，负载均衡地址为 `10.0.1.10`，节点 A 为 `10.0.1.21`，节点 B 为 `10.0.1.22`。各节点用 Nginx 监听内网 8081，转发到本机应用。负载均衡保留原始 `Host`，设置 `X-Forwarded-Proto: https`，并覆盖外部传入的 `X-Forwarded-For`，写入可信客户端地址。节点代理只接受负载均衡来源，保留该地址链。

节点 A 的 Nginx 示例（节点 B 替换监听地址）：

```nginx
server {
    listen 10.0.1.21:8081;
    allow 10.0.1.10;
    deny all;
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $http_host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $http_x_forwarded_proto;
        proxy_set_header Connection "";
        proxy_buffering off;
        proxy_read_timeout 900s;
    }
}
```

替换示例内网地址，并在安全组中限制访问。云负载均衡使用独立健康检查来源时，将其加入允许列表。

负载均衡的上游为 `10.0.1.21:8081` 和 `10.0.1.22:8081`，使用 `/readyz` 做就绪探测；模型流式请求需禁用响应缓冲，并配置足够的连接时限。管理入口和上游 OAuth 账号连接使用会话保持，或将管理域名及回调固定到一个节点。

按真实客户端 IP 限流或执行 IP 白名单时，在各节点启用 `x-forwarded-for`，并信任本机代理和实际负载均衡地址：

```dotenv
AF_SERVER__CLIENT_IP_SOURCE=x-forwarded-for
AF_SERVER__TRUSTED_PROXY_CIDRS=["127.0.0.1/32","10.0.1.10/32"]
```

可信范围仅包含实际代理地址。接入 CDN 时，由边缘代理校验 CDN 来源并重建客户端地址链。

## 5. 管理与升级

上游 OAuth 账号连接的临时授权会话保存在发起节点内存中，不通过 Redis 共享。开始连接和提交回调等请求须到达同一节点；授权过程中重启节点需重新授权。

后台设置按各自刷新机制生效，节点间可能有短暂延迟。启动配置和 Passkey 等初始化配置变更需按要求重启各节点。

升级按 [升级与备份](migrations-and-release.md) 执行，数据库迁移由单个节点完成。滚动升级须确认新旧版本兼容，否则使用维护窗口。
