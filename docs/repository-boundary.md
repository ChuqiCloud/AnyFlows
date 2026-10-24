# 公共核心与扩展接口

社区核心可独立部署，默认不注册企业服务。扩展开发只依赖公共接口，私有实现和配置由各自仓库维护。

| 仓库 | 职责 |
| --- | --- |
| 公共核心 | 模型协议、渠道路由、个人账号、API Key、计费、日志及双前端宿主 |
| 企业扩展 | 企业空间、成员与部门、SSO、SCIM、企业套餐、服务账号及管理页面 |
| distribution | 固定兼容提交，装配扩展与前端，生成发行包 |

## 运行时与 HTTP

`Bootstrap::init_authentication_with_extensions` 在数据库和公共认证服务就绪后构造扩展，初始化失败时终止启动。默认入口使用空的 `RuntimeExtensions`。

[`RuntimeExtensionContext`](../crates/af-server/src/runtime_extensions.rs) 提供数据库、配置、会话、资料、可选 Passkey、系统密钥与日志服务。`RuntimeExtensions` 注册认证、价格、额度、充值、服务账号计费、用量投影、终态写入、后台任务及 HTTP 工厂。

`Bootstrap::with_http_extensions` 可预先注册 HTTP 扩展。扩展负责构造服务、绑定状态和鉴权，核心合并路由及 OpenAPI。`GET /api/extensions` 仅提供前端入口所需的 ID、名称与能力键，各接口独立校验身份、归属和权限。

## 数据库

公共迁移使用 `seaql_migrations`。`Bootstrap::with_database_migration_extension` 注册扩展迁移，在公共迁移完成后使用同一迁移连接执行，再打开应用连接池。`MigratorExtension` 包装扩展的 `MigratorTrait`，必须使用独立历史表。

历史接管通过 `MigrationHistoryAdoption` 和 `with_legacy_history` 声明，只导入扩展已声明且已执行的版本并保留 `applied_at`。不得复制整张公共迁移表或删除旧记录。

社区迁移器读取历史时兼容拆分前的 47 个企业版本：`000061-000067`、`000093-000122`、`000124-000125`、`000129-000133`、`000136-000138`。企业表、数据及历史记录保留，其他未知迁移阻止启动。具体清单见 [迁移实现](../crates/af-db/src/migration/mod.rs)。

`DatabasePool::extension_connection` 借出 `ConnectionTrait + TransactionTrait`，核心管理连接池生命周期。扩展负责输入、归属、超时和错误脱敏；公共用户、令牌及分组通过核心仓储访问。

## 身份与价格

企业令牌通过 `OrganizationTokenAuthValidator` 校验，返回主体须匹配原始归属。验证器缺失、拒绝或失败均拒绝请求，不回退为个人身份。

`ContractPriceSource` 返回包含归属、版本和价格分量的 `BillingContractPriceSnapshot`。预留与重放保存该快照，重试不得更换价格或主体。

## 服务账号

服务账号默认不在社区版启用。扩展实现账号管理、凭据存储、授权、计费和日志，不借用个人用户、Token 或创建者身份。

- `GatewayIdentity` 绑定企业、服务账号和凭据 ID。`PresentedServiceAccountKey` 生成带域分离的 SHA-256 摘要，认证接口只接收 `ServiceAccountKeyDigest`；明文及时释放，日志不得包含密钥或摘要。
- `ServiceAccountAuthenticator` 校验身份；`ServiceAccountModelAuthenticator` 通过同一存储快照校验凭据、账号状态、企业权限、路由分组和 `ServiceModelPolicy`，经 `RuntimeExtensions::service_account_models` 注册。
- 模型白名单精确且区分大小写，空名单拒绝全部模型，下级策略求交收窄。构造时先检查数量与文本大小再去重，推理后缀规范化后检查授权。
- RPM、并发上限须为正数，按企业和账号独立计数；凭据轮换不重置限制，准入存储缺失时拒绝请求。粘性会话按企业、账号和凭据在独立哈希域生成。

服务凭据只通过单个 `Authorization: Bearer` 请求头传递，可访问 `/v1/models` 和同步 Chat Completions、Responses、Anthropic Messages、Gemini 生成接口，支持流式响应。音频、Embeddings、Images、Rerank、Compact 和异步 Video 仅接受个人 Token。

`BillingRequestPlan` 通过 `ServiceAccountBillingPort` 绑定原始身份。重试保留预约、金额和绝对到期时间，结算与取消绑定请求最初使用的凭据。`UsageRecordProjection` 将用量写入扩展存储；失败保留待投递记录，不写入个人日志。

`RuntimeExtensions::service_account_outcomes` 注册 `ServiceAccountOutcomeSink`，独立记录成功、失败和中断。记录包含准入身份、请求 ID、模型、协议、操作、分组、渠道、耗时及失败类别，不含正文或密钥。存储须幂等追加、拒绝冲突；临时错误最多尝试三次，不改变响应或结算。流取消后异步写入一次，重启后的结算恢复、日志恢复与查询由扩展实现。

前端构建扩展复用宿主的会话与权限边界，见 [前端模板与扩展](frontend-template-development.md#构建扩展)。
