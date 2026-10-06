# AnyFlows 文档

这里是面向部署者、管理员和运维人员的正式文档入口。

- [部署与配置参考](./deployment-and-configuration.md)：Docker Compose、systemd、环境变量、健康检查、升级和回滚。
- [认证与访问指南](./authentication-and-access.md)：管理会话、用户 API Key、平台管理员令牌、企业 SSO 和 SCIM 的使用边界。
- [账号认证与企业空间](./account-and-enterprise-verification.md)：个人及企业认证、材料预览、申请记录、企业空间开通和授权调整。
- [API 目录与在线调试](./api-explorer.md)：公开 API 目录、权限投影、分页查询和受控在线请求。
- [迁移与发布说明](./migrations-and-release.md)：版本归档、数据库迁移、升级验证和程序回滚边界。
- [前端模板开发规范](./frontend-template-development.md)：独立构建、OpenAPI 契约、模板扫描切换、部署路径和缓存限制。

架构设计、数据模型、开发 TODO 和安全不变量位于独立的 [AnyFlows-Design](https://cnb.cool/acmecloud/AnyFlows-Design) 仓库，不在产品仓库的 `docs/` 中维护。
