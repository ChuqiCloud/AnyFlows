# AnyFlows

[English](README.en.md) · [使用文档](https://github.com/ChuqiCloud/AnyFlows/blob/main/docs/README.md) · [版本下载](https://github.com/ChuqiCloud/AnyFlows/releases)

AnyFlows 是一个可自行部署的 AI API 网关。将不同厂商的模型接入同一服务，为应用统一提供 API Key、模型路由、额度管理和调用记录。

## 主要功能

- 兼容 OpenAI Chat、Responses 等接口，支持流式响应和模型列表。
- 管理渠道、密钥池和账号池，按模型能力与渠道状态分配请求。
- 管理用户、分组、API Key、模型权限、价格和额度。
- 查看调用日志、用量、费用及模型和渠道的运行情况。
- 内置 Classic 和 Next 两套管理前端，支持切换和外部静态模板。
- 支持 SQLite、PostgreSQL、MySQL，可接入 Redis 共享限流等状态。

本仓库提供社区版核心，可独立部署。企业空间、企业 SSO 等私有功能由独立扩展提供。

## 二进制部署

推荐在使用 systemd 的 Linux 服务器上安装发布包。包内已包含两套前端，无需安装 Rust、Node.js，也无需在服务器编译。

在 [Releases](https://github.com/ChuqiCloud/AnyFlows/releases) 下载适合服务器架构的归档及对应的 `.sha256` 文件。Beta 版本用于测试和部署验收；没有可用二进制时，也可以使用 [Docker Compose 源码部署](docs/docker-compose.md)。

以 Linux amd64 包为例，在下载目录执行。将 `PACKAGE` 中的「版本号」替换为下载版本，包含可能的预发布后缀：

```bash
PACKAGE='anyflows-v版本号-linux-amd64'
sha256sum --check "$PACKAGE.tar.gz.sha256"
tar -xzf "$PACKAGE.tar.gz"
cd "$PACKAGE"
sudo ./install.sh install
sudo ./install.sh status
```

安装器自动生成密钥、创建服务用户并设置开机启动，默认使用 SQLite。服务监听 `127.0.0.1:8080`。首次安装请通过 SSH 隧道访问并创建管理员，再配置域名和 HTTPS。

完整步骤见 [二进制安装教程](docs/deployment-and-configuration.md)。多台服务器部署见 [多节点部署](docs/multi-node-deployment.md)。

## 开始使用

1. 打开站点，进入登录或控制台页面，按提示完成初始化并创建管理员。
2. 在后台添加渠道和凭据，获取并确认可用模型，再配置需要对外提供的模型与价格。
3. 创建用户 API Key，为客户端填写站点地址和 API Key。
4. 在站点的 `/api` 页面或客户端发起一次调用，查看调用日志与用量。

不同客户端要求的 Base URL 格式可能不同。OpenAI 兼容客户端通常填写 `https://你的域名/v1`；若客户端会自行追加 `/v1`，则填写站点根地址。

## 文档

| 内容 | 链接 |
| --- | --- |
| 下载、安装、初始化和域名访问 | [二进制安装](docs/deployment-and-configuration.md) |
| 负载均衡、共享依赖和升级顺序 | [多节点部署](docs/multi-node-deployment.md) |
| Docker Compose 源码部署 | [容器部署](docs/docker-compose.md) |
| 环境变量、数据库和密钥 | [配置参考](docs/configuration.md) |
| 备份、升级和回滚 | [升级维护](docs/migrations-and-release.md) |
| 版本标签和发布阶段 | [版本标签规范](docs/release-tagging.md) |
| 账号、API Key 和注册邀请 | [认证与访问](docs/authentication-and-access.md) |
| 其余使用和开发文档 | [文档目录](docs/README.md) |

## 参与开发

后端使用 Rust、Axum 和 SeaORM；两套前端使用 React 和 TypeScript。从 `main` 创建分支并提交 Pull Request，开发环境和检查命令见 [贡献指南](CONTRIBUTING.md)。社区开发不需要访问私有仓库。

## 许可证

本项目采用 [GNU AGPL v3](LICENSE)。
