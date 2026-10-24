# AnyFlows

[English](README.en.md) · [使用文档](docs/README.md) · [版本下载](https://github.com/ChuqiCloud/AnyFlows/releases)

AnyFlows 是一个可自行部署的 AI API 网关。将不同厂商的模型接入同一服务，为应用统一提供 API Key、模型路由、额度管理和调用记录。

## 主要功能

- 支持 OpenAI Chat、OpenAI Responses、Anthropic Messages 和 Gemini GenerateContent，以及四类协议之间的请求与响应转换。
- 支持流式响应、工具调用和连续对话，保留可转换的历史内容与用量信息。
- 管理渠道、密钥池和账号池，按模型能力与渠道状态分配请求。
- 管理用户、分组、API Key、模型权限、价格和额度。
- 查看调用日志、用量、费用及模型和渠道的运行情况。
- 内置 Classic 和 Next 两套管理前端，支持切换和外部静态模板。
- 支持 SQLite、PostgreSQL、MySQL，可接入 Redis 共享限流等状态。

本仓库提供社区版核心，可独立部署。企业空间、企业 SSO 等私有功能由独立扩展提供。

## 二进制部署

Linux 服务器可使用发布包部署。包内包含二进制程序和两套前端，运行时不需要 Rust 或 Node.js。

从 [Releases](https://github.com/ChuqiCloud/AnyFlows/releases) 下载归档及对应的 `.sha256` 文件。目前社区发布包提供 Linux amd64 版本；Beta 为测试版。

在下载目录校验并解压。将 `版本号` 替换为实际版本，保留预发布后缀：

```bash
PACKAGE='anyflows-v版本号-linux-amd64'
sha256sum --check "$PACKAGE.tar.gz.sha256"
tar -xzf "$PACKAGE.tar.gz"
cd "$PACKAGE"
```

按 [二进制部署](docs/deployment-and-configuration.md) 创建服务用户、配置数据库和密钥，再通过 systemd 启动。单机可使用 SQLite，默认监听 `127.0.0.1:8080`。首次访问通过 SSH 隧道创建管理员，然后配置域名和 HTTPS。

其他部署方式见 [Docker Compose](docs/docker-compose.md) 和 [多节点部署](docs/multi-node-deployment.md)。

## 开始使用

1. 在控制台完成初始化并创建管理员。
2. 添加渠道和凭据，配置可用模型、路由与价格。
3. 创建用户 API Key，在客户端填写服务地址、密钥和模型名。
4. 发起请求，在调用日志中查看结果和用量。站点 `/api` 页面提供接口目录和在线调试。

OpenAI 兼容客户端通常使用 `https://api.example.com/v1` 作为 Base URL；会自动追加 `/v1` 的客户端填写站点根地址。协议与参数说明见 [对话协议兼容](docs/conversation-compatibility.md)。

## 文档

配置和升级见 [配置参考](docs/configuration.md)、[升级与备份](docs/migrations-and-release.md)。账号、渠道及开发参考见 [文档目录](docs/README.md)。

## 参与开发

后端使用 Rust、Axum 和 SeaORM；前端使用 React 和 TypeScript。从 `main` 创建分支并提交 Pull Request。开发环境和检查命令见 [贡献指南](CONTRIBUTING.md)。

## 许可证

本项目采用 [GNU AGPL v3](LICENSE)。
