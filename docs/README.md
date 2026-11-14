# AnyFlows 文档

首次部署阅读 [二进制部署](deployment-and-configuration.md)。

| 文档 | 内容 |
| --- | --- |
| [系统能力与架构说明](project-strengths.md) | 请求处理、路由、计费、部署架构与功能状态 |
| [二进制部署](deployment-and-configuration.md) | 下载、校验、手动配置、systemd 和初始化 |
| [Docker Compose](docker-compose.md) | 源码构建容器、配置和数据卷 |
| [多节点部署](multi-node-deployment.md) | 共享依赖、负载均衡和升级顺序 |
| [配置参考](configuration.md) | 环境变量、密钥和代理 |
| [升级与备份](migrations-and-release.md) | 数据备份、迁移、升级和回退 |
| [账号与 API](authentication-and-access.md) | 登录、API Key、主体认证和接口调试 |
| [系统 API Token](personal-access-tokens.md) | 脚本认证、权限、有效期和应急撤销 |
| [插件 Sidecar 协议](plugin-sidecars.md) | 已签名本地插件包、受限 JSONL 进程协议和当前信任边界 |
| [渠道配置](channels.md) | 客户端档案、代理池和 NVIDIA 上游 |
| [渠道质量评分](channel-routing.md) | 成功率与延迟、模型隔离、软避让和可选上游价值配置 |
| [对话协议](conversation-compatibility.md) | 四类协议转换、历史回放和兼容范围 |
| [运行状态](runtime-status.md) | 节点、依赖健康和用量统计 |
| [前端开发](frontend-template-development.md) | 静态模板、构建扩展和接口约定 |
| [版本规范](release-tagging.md) | 版本阶段、标签与社区发布 |

开发环境、渠道适配和检查命令见 [贡献指南](../CONTRIBUTING.md)。

- [版本检测与手动在线升级](online-updates.md)：部署识别、发行包匹配、二进制更新与容器更新边界。
