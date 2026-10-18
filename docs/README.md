# AnyFlows 文档

首次部署从二进制安装开始。需要多台服务器时，再阅读多节点部署；完整参数和开发契约分别放在独立文档中。

## 安装与维护

| 文档 | 内容 |
| --- | --- |
| [二进制安装](deployment-and-configuration.md) | 下载、校验、安装、初始化和域名访问 |
| [多节点部署](multi-node-deployment.md) | 共享数据库和 Redis、负载均衡、节点配置与升级顺序 |
| [Docker Compose 部署](docker-compose.md) | 从源码构建容器、查看日志和管理数据卷 |
| [配置参考](configuration.md) | 配置文件、环境变量、密钥和可选服务 |
| [升级与备份](migrations-and-release.md) | 数据备份、数据库迁移、升级和回滚 |
| [版本标签规范](release-tagging.md) | Alpha、Beta、RC、正式版标签及三仓发布对应关系 |

## 使用与管理

| 文档 | 内容 |
| --- | --- |
| [认证与访问](authentication-and-access.md) | 登录、API Key、注册与邀请奖励 |
| [个人与企业主体认证](account-and-enterprise-verification.md) | 人工审核、材料查看和支付宝实名认证 |
| [API 目录与在线调试](api-explorer.md) | 接口查找、访问权限和请求调试 |
| [运行状态](runtime-status.md) | 节点负载、数据库与 Redis 健康、上下文缓存统计 |
| [客户端档案](client-profiles.md) | 身份配置、修订管理、渠道绑定和 OpenCode 入口 |
| [前端模板](frontend-template-development.md) | 内置前端、外部模板、预览和切换 |

## 开发与仓库维护

| 文档 | 内容 |
| --- | --- |
| [贡献指南](../CONTRIBUTING.md) | 开发环境、提交规范和检查范围 |
| [仓库与扩展边界](repository-boundary.md) | 公共核心和私有扩展的职责 |
| [前端扩展](frontend-extensions.md) | 扩展页面、导航和能力发现 |
| [服务账号身份](service-identity.md) | 独立身份、认证和运行时契约 |
| [服务账号模型授权](service-model-authorization.md) | 模型授权和请求权限 |
| [公共核心导出](public-core-export.md) | 可复现导出和内容保护 |
| [公共 PR 回传](public-pr-sync.md) | GitHub 合并后的同步流程和维护配置 |
