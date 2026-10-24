# 公共核心与扩展边界

本仓库提供可独立部署的社区核心，通过扩展接口与企业实现及发行装配协作。

## 仓库职责

| 仓库 | 负责内容 |
| --- | --- |
| 公共核心 | 模型协议、渠道路由、个人账号和 API Key、公共管理、计费基础能力、日志及两套前端宿主 |
| 企业扩展 | 企业空间、成员与部门、SSO、SCIM、企业套餐、服务账号及对应管理页面 |
| distribution | 固定兼容的公共核心与企业扩展提交，装配前后端并生成发行包 |

社区开发无需私有仓库权限。企业实现和私有配置保留在私有仓库，distribution 记录版本组合。

## 启动与 HTTP

默认 `init_authentication` 使用 `RuntimeExtensions::default()`。发行版通过 `Bootstrap::init_authentication_with_extensions` 提供工厂，在公共数据库和认证服务准备后装配扩展；初始化失败时终止启动。

`RuntimeExtensionContext` 提供公共数据库连接池、已校验配置、会话与资料服务、可选 Passkey 服务、系统密钥能力及日志仓储。`RuntimeExtensions` 接收认证验证器、合同价格、额度与充值适配、服务账号计费、用量投影、终态写入、后台任务和 HTTP 工厂。

`Bootstrap::with_http_extensions` 可预先注册 `HttpExtensions`。每个 `HttpExtension` 提供已绑定状态与鉴权的公开、管理和回执路由，以及可选 OpenAPI 片段。核心负责合并路由和文档，扩展负责构造其服务。

`GET /api/extensions` 返回扩展 ID、名称和能力键，供前端选择页面入口。每个接口独立校验身份、归属和权限。

## 数据库迁移与访问

公共迁移使用 `seaql_migrations`。发行版通过 `Bootstrap::with_database_migration_extension` 注册 `DatabaseMigrationExtension`，在公共迁移完成后使用同一迁移连接运行扩展迁移，再打开应用连接池。扩展以 `MigratorExtension` 包装自己的 `MigratorTrait`，使用独立的迁移历史表。

旧版企业迁移历史通过 `MigrationHistoryAdoption` 和 `MigratorExtension::with_legacy_history` 接管。只导入扩展注册表已声明且已执行的版本，保留 `applied_at`；旧表不存在时直接执行扩展迁移。禁止复制整张公共迁移表或删除旧记录。

企业版切换为社区版时，社区迁移器读取迁移状态会忽略拆分前的 47 个企业版本：`000061-000067`、`000093-000122`、`000124-000125`、`000129-000133` 和 `000136-000138`。对应迁移记录、时间戳、企业表与数据保留，社区版不提供这些功能。其他未知迁移版本会阻止启动。

`DatabasePool::extension_connection` 向扩展借出 `ConnectionTrait + TransactionTrait` 接口，连接池生命周期由核心管理。扩展仓储负责输入校验、归属、查询超时和错误脱敏；公共用户、令牌及分组由核心仓储管理。

## 认证和计费

个人令牌读取公共用户、令牌和分组状态。企业令牌由 `OrganizationTokenAuthValidator` 校验归属及授权，返回主体须匹配原始所有权上下文。验证器缺失、拒绝或失败时均拒绝请求。

公共计费通过 `ContractPriceSource` 接收 `BillingContractPriceSnapshot`，包含归属、版本和价格分量。额度预留及重放保存该快照，重试时不得替换价格或主体。

服务账号使用独立身份、模型授权、资金和日志契约。要求见 [服务账号身份](service-identity.md) 和 [模型授权](service-model-authorization.md)。

## 前端与同步

公共仓库提供 Classic、Next 宿主及扩展注册接口。发行版在构建时装配企业模块，按后端能力和当前角色显示导航，页面复用宿主会话。见 [前端扩展](frontend-extensions.md)。

Gitea `develop` 导出公共核心，GitHub `main` 接收社区贡献；合并后的变更通过评审 PR 回传。导出会阻止覆盖尚未回传的公共改动，见 [公共核心导出](public-core-export.md) 和 [公共 PR 同步](public-pr-sync.md)。
