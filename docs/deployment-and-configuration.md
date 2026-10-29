# 二进制安装

本文以 Debian / Ubuntu 和 systemd 为例，说明二进制安装、管理员初始化及 HTTPS 配置。发布包内含 Classic、Next 两套前端，运行时无需 Rust 或 Node.js。

## 1. 准备服务器

服务器需支持 SSH，并具有 root 或 sudo 权限。安装所需工具：

```bash
sudo apt update
sudo apt install -y ca-certificates curl openssl tar
uname -m
```

`x86_64` 对应 amd64，`aarch64` 对应 arm64。下载发布页实际提供且与系统架构匹配的归档。GNU/Linux 包不适用于 Alpine、Windows 或 macOS。

本文使用 SQLite，无需单独安装数据库。多节点部署使用共享 PostgreSQL 或 MySQL，配置见 [多节点部署](multi-node-deployment.md)。

## 2. 下载并校验

从 [Releases](https://github.com/ChuqiCloud/AnyFlows/releases) 下载二进制归档和同名 `.sha256` 文件，放在服务器的同一目录。Alpha、Beta 和 RC 为预发布版本。GitHub 自动生成的 Source code 压缩包不含可执行程序；源码部署见 [Docker Compose 部署](docker-compose.md)。

以下以 `v0.3.0-beta.5` 的 amd64 包为例，使用其他版本时修改 `PACKAGE`：

```bash
PACKAGE='anyflows-v0.3.0-beta.5-linux-amd64'
sha256sum --check "$PACKAGE.tar.gz.sha256"
tar -xzf "$PACKAGE.tar.gz"
cd "$PACKAGE"
```

校验结果应为 `OK`。校验失败时重新下载，确认通过后再安装。

## 3. 安装并启动

以下命令用于首次安装，在解压后的目录执行。已有部署按 [升级与备份](migrations-and-release.md) 操作。

创建服务账号和目录；已有 `anyflows` 账号时跳过账号创建命令：

```bash
sudo useradd --system --user-group --home-dir /var/lib/anyflows --no-create-home --shell /usr/sbin/nologin anyflows
sudo install -d -o root -g root -m 0755 /usr/local/libexec/anyflows
sudo install -d -o root -g anyflows -m 0750 /etc/anyflows
sudo install -d -o anyflows -g anyflows -m 0750 /var/lib/anyflows
sudo install -d -o anyflows -g anyflows -m 0700 /var/lib/anyflows/billing-wal
sudo install -d -o anyflows -g anyflows -m 0700 /var/lib/anyflows/images
sudo install -o root -g root -m 0755 ./anyflows /usr/local/libexec/anyflows/anyflows
sudo install -o root -g root -m 0755 ./anyflows-launcher /usr/local/libexec/anyflows/anyflows-launcher
```

分别为会话签名和凭据加密生成密钥。运行两次以下命令，每次生成一个独立的 32 字节随机值：

```bash
openssl rand -base64 32 | tr '+/' '-_' | tr -d '='
```

创建仅 root 和服务组可读的配置文件：

```bash
sudo touch /etc/anyflows/anyflows.env
sudo chown root:anyflows /etc/anyflows/anyflows.env
sudo chmod 0640 /etc/anyflows/anyflows.env
sudoedit /etc/anyflows/anyflows.env
```

写入以下配置，将两处占位值替换为刚生成的不同密钥。多节点或外部数据库部署应在启动前调整连接信息。

```dotenv
AF_SERVER__BIND=127.0.0.1:8080
AF_DATABASE__URL=sqlite:///var/lib/anyflows/anyflows.db?mode=rwc
AF_AUTH__SESSION_SIGNING_KEY=replace-with-session-signing-key
AF_CREDENTIAL_ENCRYPTION__KEY_ID=primary
AF_CREDENTIAL_ENCRYPTION__KEY=replace-with-credential-encryption-key
AF_BILLING__WAL_DIRECTORY=/var/lib/anyflows/billing-wal
AF_IMAGE_STORAGE__DIRECTORY=/var/lib/anyflows/images
AF_TELEMETRY__LEVEL=info
```

安装归档中的 systemd 服务单元并启动：

```bash
sudo install -o root -g root -m 0644 ./anyflows.service /etc/systemd/system/anyflows.service
sudo systemctl daemon-reload
sudo systemctl enable --now anyflows
sudo systemctl --no-pager status anyflows
curl --fail http://127.0.0.1:8080/readyz
```

数据库迁移在启动时执行。`/readyz` 返回成功后，服务可接受请求。安装位置如下：

| 内容 | 位置 |
| --- | --- |
| 程序 | `/usr/local/libexec/anyflows/anyflows` |
| 启动器 | `/usr/local/libexec/anyflows/anyflows-launcher` |
| 托管版本与更新状态 | `/var/lib/anyflows/launcher` |
| 配置 | `/etc/anyflows/anyflows.env` |
| SQLite 和运行数据 | `/var/lib/anyflows` |
| 计费恢复日志（WAL） | `/var/lib/anyflows/billing-wal` |
| 生图文件 | `/var/lib/anyflows/images` |

程序只接受回环监听。使用同机反向代理提供外部访问。

默认服务由启动器托管，并继承 `EnvironmentFile` 中的配置。在线安装后由启动器自动重启当前节点，更新仅写入数据目录中的托管版本区域；程序目录保持只读。直接运行主程序的裸启动方式仍可使用，安装后需通过原服务管理器重启。详见 [版本检测与在线升级](online-updates.md)。

## 4. 创建管理员

在本地电脑建立 SSH 隧道，将用户名和服务器地址替换为实际值，并保持终端运行：

```bash
ssh -N -L 18080:127.0.0.1:8080 user@server
```

在浏览器打开 `http://127.0.0.1:18080`，进入控制台并创建首个管理员。系统没有默认管理员密码。完成初始化后关闭隧道，配置公网入口。

## 5. 使用域名和 HTTPS

将域名解析到服务器，开放 80、443 端口，并在同机安装反向代理。以下使用 [Caddy](https://caddyserver.com/docs/install)；也可使用已有的 Nginx 等代理。

按 Caddy 官方步骤安装后，将以下内容写入 `/etc/caddy/Caddyfile`，替换域名：

```caddyfile
api.example.com {
    reverse_proxy 127.0.0.1:8080 {
        flush_interval -1
    }
}
```

检查配置并重新加载：

```bash
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

访问 `https://api.example.com`，在后台站点设置中填写该 HTTPS 地址，供登录、认证和支付等回调使用。

使用 Nginx 时，关闭上游响应缓冲，并按最长模型响应时间设置读取超时。真实客户端 IP 限流和 API Key 来源限制见 [代理与客户端地址](configuration.md#代理与客户端地址)。

## 6. 接入第一个模型

按 [渠道管理](channels.md) 添加上游并配置模型、路由及分组权限，再创建 [用户 API Key](authentication-and-access.md#创建和使用-api-key) 发起调用。

## 常用操作

```bash
sudo systemctl restart anyflows
sudo journalctl -u anyflows -n 100 --no-pager
curl --fail http://127.0.0.1:8080/readyz
```

修改配置使用 `sudoedit /etc/anyflows/anyflows.env`，保存后重启服务。`/healthz` 表示进程存活，`/readyz` 用于检查服务是否就绪。

通过版本接口核对部署版本和构建信息：

```bash
curl --fail http://127.0.0.1:8080/api/version
```

`/api/version`（别名 `/version`）无需登录，返回 `version`、`commit` 和 `build_time`，并禁用缓存。后两项分别为源码提交和 UTC 构建时间，缺少构建元数据时为 `null`。构建信息在编译时固定，运行时环境变量不会改变这些值。

启动失败时，用 `sudo journalctl -u anyflows -n 100 --no-pager` 检查端口、配置和依赖连接错误。保留数据库、配置及 WAL，按日志定位原因。

升级前请阅读 [升级与备份](migrations-and-release.md)。其他部署方式和参数见 [容器部署](docker-compose.md)、[配置参考](configuration.md)。
