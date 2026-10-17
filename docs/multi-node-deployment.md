# 多节点部署

多节点用于分担请求流量和应用节点故障时继续提供服务。建议先完成单机安装，确认一次模型调用、日志和计费正常，再扩展节点。

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

AnyFlows 只接受回环监听。每个应用节点需要本机代理，负载均衡连接节点代理的内网端口；不要尝试把应用改为 `0.0.0.0` 或让负载均衡直接连接远程应用的 8080 端口。

## 1. 准备共享依赖

- 使用同一套 PostgreSQL 或 MySQL 数据库。SQLite 文件不适合多节点，不要通过 NFS 共享 SQLite。
- 使用同一个 Redis 实例和逻辑数据库，共享限流、并发控制、账号调度状态和 OAuth 刷新租约。不同环境使用不同 Redis 逻辑库或实例，不能只改请求限流的 namespace 就认为全部状态已隔离。
- 每个节点使用同一版本程序、相同的会话签名密钥、凭据加密密钥与标识、请求限流 namespace，以及一致的对外站点地址。
- 每个节点保留自己的持久数据盘和 WAL 目录。多个实例不能同时使用同一个 WAL 目录；故障节点的磁盘及 WAL 需要保留，以便恢复尚未完成的计费记录。
- 使用外部前端模板时，将相同版本的模板文件分发到所有节点，并分别重新扫描。

数据库和 Redis 只开放给应用节点，账号按实际需要授权。共享数据库与 Redis 自身的备份和高可用也需要单独规划。

## 2. 配置第一个节点

在服务器上按 [二进制安装](deployment-and-configuration.md) 安装。安装器会立即启动单机 SQLite 服务，先停止它，再编辑配置：

```bash
sudo systemctl stop anyflows
sudoedit /etc/anyflows/anyflows.env
```

下面是需要统一的主要字段。示例中的地址、密码和密钥必须替换，不能直接启动：

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

MySQL 使用当前版本支持的 `mysql://用户:密码@主机:3306/数据库` 连接格式。连接 URL 中密码的特殊字符需要 URL 编码。

启动第一个节点，等待迁移和就绪检查完成，再通过 SSH 隧道创建管理员：

```bash
sudo systemctl start anyflows
curl --fail http://127.0.0.1:8080/readyz
```

从空库开始只需初始化一次。已有 SQLite 数据不能靠修改 URL 自动转为 PostgreSQL 或 MySQL；迁移现有数据需先准备并验证数据迁移方案。

## 3. 加入其他节点

在其他服务器安装相同版本，停止服务，再通过受限配置或密钥管理系统写入共享连接信息与相同密钥。不要保留安装器为各节点单独生成的不同密钥。

每台服务器上的 `/var/lib/anyflows/billing-wal` 可以使用相同路径名称，但必须位于各自独立磁盘目录。逐个启动并检查 `/readyz`，确认日志没有迁移、解密或 Redis 错误后再加入负载均衡。

## 4. 配置代理和负载均衡

例如负载均衡内网地址为 `10.0.1.10`，节点 A 为 `10.0.1.21`，节点 B 为 `10.0.1.22`。各节点用 Nginx 监听内网 8081，转发到本机应用。负载均衡层保留原始 `Host`，将 `X-Forwarded-Proto` 设为 `https`，覆盖外部传入的 `X-Forwarded-For` 并写入可信客户端地址；节点代理只允许负载均衡来源，保留这一受控地址链。

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

将示例地址替换为实际内网地址，云负载均衡的健康检查若使用其他来源地址，也需加入允许列表，并在安全组中限制访问。

负载均衡的上游为 `10.0.1.21:8081` 和 `10.0.1.22:8081`，使用 `/readyz` 做就绪探测；模型流式请求需禁用响应缓冲，并配置足够的连接时限。管理入口和上游 OAuth 账号连接使用会话保持，或将管理域名及回调固定到一个节点。

若要按客户端真实 IP 限流或执行 IP 白名单，在各节点启用 `x-forwarded-for`，并信任本机代理地址及实际负载均衡地址。例如负载均衡为 `10.0.1.10`：

```dotenv
AF_SERVER__CLIENT_IP_SOURCE=x-forwarded-for
AF_SERVER__TRUSTED_PROXY_CIDRS=["127.0.0.1/32","10.0.1.10/32"]
```

不要信任整个公网或无关内网地址。若前面还有 CDN，应先在边缘代理校验可信 CDN 来源并重建客户端地址链，再让节点信任该链。

## 5. 管理与升级

上游 OAuth 账号连接过程中保存的临时授权会话位于发起连接的进程内存中，Redis 刷新租约不负责共享这部分状态。开始连接、提交回调等请求必须到同一个节点；过程中重启节点，需要重新授权。

后台设置按各自刷新机制生效，不保证所有设置在所有节点瞬时更新。涉及启动配置、Passkey 等初始化能力时，应按配置要求重启各节点。

升级时先备份数据库、配置和每个节点的 WAL。将待升级节点从负载均衡摘除，等待长请求结束，再停止服务。先让一个新版本节点完成数据库迁移并检查就绪，再启动其他节点。

只有确认目标版本与旧版本的数据库和运行时兼容后，才安排滚动升级；否则使用维护窗口。程序回退不会撤销数据库迁移，不能把多节点部署当作自动无损恢复方案。详细步骤见 [升级与备份](migrations-and-release.md)。
