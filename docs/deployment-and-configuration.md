# 二进制安装

本文以使用 systemd 的 Debian / Ubuntu 服务器为例，完成安装、创建管理员和域名访问。发布包已内嵌 Classic、Next 两套前端，不需要在服务器安装 Rust 或 Node.js。

## 1. 准备服务器

需要一台可通过 SSH 登录的 Linux 服务器，以及 root 或 sudo 权限。安装常用工具：

```bash
sudo apt update
sudo apt install -y ca-certificates curl openssl tar
uname -m
```

`x86_64` 对应 amd64；`aarch64` 对应 arm64。只选择发布页实际提供且与系统匹配的包。Linux 包不适用于 Windows、macOS，普通 GNU/Linux 包也不能直接当作 Alpine 包使用。

默认使用 SQLite，单机起步不需要另外安装数据库。计划多节点部署时，请先看 [多节点部署](multi-node-deployment.md)。

## 2. 下载并校验

打开 [Releases](https://github.com/ChuqiCloud/AnyFlows/releases)，下载归档和同名的 `.sha256` 文件，放在服务器的同一目录中。公共二进制包正在准备发布；如果页面尚无归档，请等待发布，或使用 [Docker Compose 源码部署](docker-compose.md)。不要将 GitHub 自动生成的 Source code 压缩包当作二进制包。

以下以 amd64 为例，将 `PACKAGE` 中的「版本号」替换为下载版本，包括可能的 `-beta.N` 等后缀：

```bash
PACKAGE='anyflows-v版本号-linux-amd64'
sha256sum --check "$PACKAGE.tar.gz.sha256"
tar -xzf "$PACKAGE.tar.gz"
cd "$PACKAGE"
```

校验结果应为 `OK`。校验失败时重新下载，不要继续安装。

## 3. 安装并启动

在解压后的目录执行：

```bash
sudo ./install.sh install
sudo ./install.sh status
curl --fail http://127.0.0.1:8080/readyz
```

安装器创建 `anyflows` 服务用户、随机生成密钥、启动服务并设置开机启动。已有配置会保留。

| 内容 | 位置 |
| --- | --- |
| 程序 | `/usr/local/libexec/anyflows/anyflows` |
| 配置 | `/etc/anyflows/anyflows.env` |
| SQLite 和运行数据 | `/var/lib/anyflows` |
| 计费恢复日志（WAL） | `/var/lib/anyflows/billing-wal` |

服务只监听 `127.0.0.1:8080`，不能直接通过服务器公网 IP 加端口访问。请保持此设置，通过反向代理提供外部访问。

## 4. 创建管理员

先通过 SSH 隧道完成初始化。在自己的电脑上运行下面的命令，将用户名和服务器地址替换为实际值，保持终端窗口打开：

```bash
ssh -N -L 18080:127.0.0.1:8080 user@server
```

用电脑浏览器打开 `http://127.0.0.1:18080`，点击登录或进入控制台，按提示创建首个管理员。系统没有默认管理员密码。完成后可以关闭隧道，再配置公网入口。

## 5. 使用域名和 HTTPS

将域名解析到服务器，开放 80、443 端口，在同一台服务器上安装反向代理。下面使用 [Caddy](https://caddyserver.com/docs/install)，已有 Nginx 等代理时可继续使用。

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

浏览器访问 `https://api.example.com`。在后台站点设置中填写同一个 HTTPS 公开地址，供登录、认证和支付等回调使用。

使用 Nginx 时，请关闭上游响应缓冲，并根据最长模型响应时间设置读取超时，避免流式回答卡住或被提前断开。需要按真实客户端 IP 限流或限制 API Key 来源时，继续配置 [代理与客户端地址](configuration.md#代理与客户端地址)。

## 6. 接入第一个模型

1. 添加渠道，选择与上游一致的渠道类型，填写该类型要求的信息。
2. 在渠道中添加 API Key 或账号凭据，获取并确认凭据可用的模型。
3. 配置对外提供的模型、路由和价格，确认用户分组有权调用。
4. 创建用户 API Key，在站点 `/api` 页或客户端调用一次，再查看调用日志。

登录后台的会话与调用模型的 API Key 是两种凭据。客户端调用模型应使用用户 API Key。

## 常用操作

```bash
sudo systemctl restart anyflows
sudo systemctl stop anyflows
sudo systemctl start anyflows
sudo journalctl -u anyflows -f
curl --fail http://127.0.0.1:8080/healthz
curl --fail http://127.0.0.1:8080/readyz
```

修改配置使用 `sudoedit /etc/anyflows/anyflows.env`，保存后重启服务。`/healthz` 表示进程存活，`/readyz` 用于检查服务是否就绪。

安装失败时先看 `sudo journalctl -u anyflows -n 100 --no-pager`。端口占用、配置错误和数据库连接失败应按日志处理；不要删除数据库、配置或 WAL 重试安装。

升级前请阅读 [升级与备份](migrations-and-release.md)。其他部署方式和参数见 [容器部署](docker-compose.md)、[配置参考](configuration.md)。
