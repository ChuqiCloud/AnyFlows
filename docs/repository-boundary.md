# 公共核心与扩展边界

本仓库提供可以独立部署的社区核心。私有企业实现和发行装配分别维护，公共代码通过扩展契约与它们协作。

## 仓库职责

| 仓库 | 负责内容 |
| --- | --- |
| 公共核心 | 模型协议、渠道路由、个人账号和 API Key、公共管理、计费基础能力、日志及两套前端宿主 |
| 企业扩展 | 企业空间、成员与部门、SSO、SCIM、企业套餐、服务账号及对应管理页面 |
| distribution | 固定兼容的公共核心与企业扩展提交，装配前后端并生成发行包 |

社区开发不需要访问私有仓库。公共历史和源码树不包含企业实现或私有配置；distribution 记录版本组合，不维护第二份业务源码。

## 启动与 HTTP

默认 `init_authentication` 使用 `RuntimeExtensions::default()`，不注册私有运行时服务。发行版通过 `Bootstrap::init_authentication_with_extensions` 提供工厂，在公共数据库和认证服务准备好后装配扩展；初始化失败会终止启动。

`RuntimeExtensionContext` 提供公共数据库连接池、已校验配置、会话及资料服务、可选 Passkey 服务、系统密钥能力和共享日志仓储。扩展通过 `RuntimeExtensions` 注入认证验证器、合同价格、额度和充值适配、服务账号计费及用量投影、终态写入、后台任务和 HTTP 工厂。

`Bootstrap::with_http_extensions` 也可预先注册 `HttpExtensions`。每个 `HttpExtension` 返回已完成状态绑定及必要鉴权的公开、管理和回执路由，以及可选 OpenAPI 片段。核心合并路由和文档，不构造扩展的企业服务。

`GET /api/extensions` 仅返回扩展 ID、名称和能力键，供前端决定显示哪些页面。能力元数据不提供资源访问授权，每个接口仍需校验身份、归属和权限。

## 数据库迁移与访问

公共迁移使用 `seaql_migrations`。发行版通过 `Bootstrap::with_database_migration_extension` 注册 `DatabaseMigrationExtension`，公共迁移完成后在同一迁移连接上运行扩展迁移，再打开应用连接池。扩展通常用 `MigratorExtension` 包装自己的 `MigratorTrait`，必须使用独立的迁移历史表。

旧版企业历史接管使用 `MigrationHistoryAdoption` 和 `MigratorExtension::with_legacy_history`。只接管扩展注册表中明确声明且已执行的版本，保留 `applied_at`；旧表不存在时直接执行扩展迁移。不要复制整张公共迁移表或删除旧记录。

企业版降级为社区版时，社区迁移器仅在读取迁移状态时忽略拆分前遗留的 47 个企业版本：`000061-000067`、`000093-000122`、`000124-000125`、`000129-000133` 和 `000136-000138`。数据库中的迁移记录、时间戳、企业表及业务数据都保留，不会被降级过程改写或删除。此兼容不代表社区版可以使用企业功能；不在清单内的未知版本仍会阻止启动。

`DatabasePool::extension_connection` 向扩展借出 `ConnectionTrait + TransactionTrait` 接口，不转交连接池生命周期。扩展仓储负责输入校验、归属、查询截止时间和错误脱敏；公共用户、令牌及分组仍通过核心仓储管理。

## 认证和计费

个人令牌读取公共用户、令牌和分组状态。企业令牌由注入的 `OrganizationTokenAuthValidator` 校验归属及授权，返回主体必须匹配原始所有权上下文。验证器缺失、拒绝或失败均不能降级为个人身份。

公共计费通过 `ContractPriceSource` 接收 `BillingContractPriceSnapshot`，包含归属、版本及价格分量，不依赖企业 ORM 实体。额度预留与重放保存对应快照，不能在重试时换成新的价格或主体。

服务账号使用独立身份、模型授权、资金和日志契约，不借用创建者或成员的个人 Token。具体要求见 [服务账号身份](service-identity.md) 和 [模型授权](service-model-authorization.md)。

## 前端与同步

公共仓库提供 Classic、Next 宿主及扩展注册机制。企业模块由发行版在构建时装配，按后端能力和当前角色显示导航，所有页面仍处于宿主的会话边界内。见 [前端扩展](frontend-extensions.md)。

Gitea `develop` 导出公共核心，GitHub `main` 接收社区贡献；合并后的变更通过评审 PR 回传。导出会阻止覆盖尚未回传的公共改动，见 [公共核心导出](public-core-export.md) 和 [公共 PR 同步](public-pr-sync.md)。
