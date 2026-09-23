# 认证与访问指南

本文面向普通用户、平台管理员和企业管理员，说明 AnyFlows 当前各类凭据的用途、生命周期和权限边界。管理会话、网关 API Key、平台管理员令牌、企业 API Key 与 SCIM 令牌彼此独立，不要交叉使用。

## 管理会话

### 密码登录

向 `POST /api/auth/login` 提交用户名和密码；启用双因素认证时同时提交 `totp_code`，启用 Turnstile 时提交服务端要求的 `turnstile_token`。成功响应包含短期 `Bearer` 管理会话、过期时间和脱敏用户资料。

登录后可用 `GET /api/auth/session` 读取当前会话。会话只用于管理端 HTTP，不是调用模型的网关 API Key。收到未授权响应或会话过期后，应重新登录，不要把旧令牌继续重试。

密码重置使用以下两个接口：

- `POST /api/auth/password-reset/request` 请求重置邮件。
- `POST /api/auth/password-reset/confirm` 使用一次性材料设置新密码。

密码、TOTP、重置材料和管理会话都不得放入 URL、浏览器历史、前端持久化存储或日志。生产环境应让反向代理提供 HTTPS，并限制管理端来源。

### Passkey 和 OAuth 登录

Passkey 登录先调用 `POST /api/auth/passkey/options` 获取挑战，再调用 `POST /api/auth/passkey/verify` 验证并签发同一种管理会话。挑战具有短期有效期和一次性语义，失败后重新获取，不要复用旧挑战。

OAuth 登录先调用对应 Provider 的 `/api/auth/oauth/{provider}/start`，回调由服务端完成交换和身份校验。自定义 Provider 使用 `/api/auth/oauth/custom/{provider_key}/start`；Provider 未启用、回调地址不受信任或状态校验失败时，服务端拒绝登录。OAuth access token、refresh token 和上游响应正文不作为管理会话返回，也不应写入客户端存储。

## 用户 API Key

用户在登录后使用以下接口管理自己的网关 API Key：

| 操作 | 接口 |
| --- | --- |
| 列表 | `GET /api/tokens` |
| 签发 | `POST /api/tokens` |
| 详情 | `GET /api/tokens/{id}` |
| 更新 | `PUT /api/tokens/{id}` |
| 删除 | `DELETE /api/tokens/{id}` |

签发响应中的 `api_key` 是完整密钥，只出现一次；列表和详情只返回不可用于鉴权的 `key_prefix`。创建后立即把完整密钥放入部署方的密钥管理工具，关闭页面或丢失响应后无法从 API 恢复明文。

更新时可以调整名称、启停状态、剩余额度、过期时间、模型白名单和 IP/CIDR 白名单。需要轮换时，先签发新 Key 并验证新 Key，再删除旧 Key；不要直接把展示前缀当作密钥，也不要把完整 Key 提交到 issue、聊天、截图或日志。

调用网关时使用完整 API Key 的 Bearer 认证。管理会话只能访问管理 API，不能替代网关 API Key；网关 API Key 也不能获得管理端权限。

## API 目录与在线调试

公开首页的 `/api` 页面展示当前身份可以看到的接口。游客可以查看公开接口和统一模型网关入口；登录用户还可以查看个人及所属企业的会话接口；平台管理员可以查看完整管理目录。服务端在返回目录时完成权限过滤，不依赖前端隐藏菜单来实现访问控制。

页面对 `GET` 接口和 `/v1/...` 网关 `POST` 接口提供受控调试。管理端写入、OAuth、SSO、Webhook 以及要求特殊凭据的接口只展示 OpenAPI 契约，不会从页面发起请求。网关调试必须输入 API Key，管理端调试使用当前管理会话。

调试页面不会持久化输入的 API Key，也不会把它写入 URL、服务端目录或日志。管理会话不能替代网关 API Key；需要长期调用时，应在控制台签发并在部署方的密钥管理工具中保存完整 Key。接口字段、分页参数和可调试范围见 [API 目录与在线调试](./api-explorer.md)。

## 平台管理员令牌

平台管理员使用管理会话访问 `/api/admin/tokens` 及其条目接口，管理指定用户的管理员令牌。管理员令牌可以额外固定用户、分组、模型、IP、请求窗口和跨分组重试等策略，但响应同样只返回展示前缀，完整 `api_key` 只在签发响应中出现一次。

管理员操作应遵循最小权限：

1. 为单一用途设置明确名称、用户归属、分组和过期时间。
2. 不需要无限额度时关闭 `unlimited_quota`，并设置模型、IP 或请求窗口限制。
3. 人员离职、用途结束或密钥疑似泄露时立即软删除或停用，不依赖自然过期。
4. 不把管理员令牌交给普通用户或写入前端代码。

## 账号认证与企业空间

个人和企业认证统一在「账号认证」中提交并查看记录。企业认证通过后，账号获得申请企业空间的资格；空间开通仍需平台管理员审批。平台管理员可以在企业目录中继续调整已开通空间的能力、容量和有效期。材料可见范围、历史记录与 SSO 开关行为见 [账号认证与企业空间](./account-and-enterprise-verification.md)。

## 企业 SSO

企业 SSO 的公开登录流程不要求先建立管理会话。登录页先向 `POST /api/auth/organization-sso/discovery` 提交邮箱或企业公开标识；响应只包含通用的 `available`，未知、停用、过期、冲突或损坏事实统一返回不可用，不泄露企业名称、域名、Provider 或恢复信息。

发现可用后调用 `POST /api/auth/organization-sso/login/start`：

- OIDC 只返回受控授权地址和短期协议材料，由浏览器跳转到身份提供商。
- SAML 只返回受控 HTTPS POST action、`SAMLRequest` 和 `RelayState`，前端应立即提交隐藏表单，不把材料保存到 URL 或本地存储。

服务端会重新校验企业 Active 状态、商业授权、Provider 状态和已验证域名，并对公开启动执行 Redis 组合限流。Redis 不可用、配置损坏、状态过期或校验失败时保持失败关闭。

OIDC 回调成功后，受控的一次性 ticket 只能通过同源 `POST /api/auth/organization-sso/session/exchange` 兑换成既有 Bearer 管理会话；该接口只支持 OIDC ticket。SAML 使用独立回调流程，不得把 SAML 响应伪装成 OIDC ticket。ticket、subject、Provider 原文和配置版本不会写入响应或日志，也不能重放。

企业管理员在登录后按企业权限配置 SSO：

- `organization.sso.read`：读取脱敏 Provider、域名和策略。
- `organization.sso.manage`：管理 Provider、已验证域名和强制策略。
- `organization.sso.recovery`：管理恢复入口。

上述权限只在当前企业上下文中生效；跨企业读取、修改或恢复应被拒绝。Provider 密钥、受信证书和端点材料按部署与管理端的密钥边界保存，前端只接收脱敏投影。

## SCIM 同步

SCIM 使用企业专属 Bearer 令牌，不使用管理会话、用户 API Key 或平台管理员令牌。接口根路径固定为 `/scim/v2/{organization_id}`，当前支持服务能力和资源类型发现，以及 Users、Groups 的同步：

- `GET /scim/v2/{organization_id}/ServiceProviderConfig`
- `GET /scim/v2/{organization_id}/ResourceTypes`
- `POST /scim/v2/{organization_id}/Users`
- `PUT /scim/v2/{organization_id}/Users/{resource_id}`
- `POST /scim/v2/{organization_id}/Groups`
- `PUT|DELETE /scim/v2/{organization_id}/Groups/{resource_id}`

每次请求都要同时满足 Bearer 令牌有效、令牌属于路径中的企业、令牌具备对应 Users/Groups 范围以及资源版本条件。过期、撤销、跨企业、版本落后或生命周期不允许的请求会被拒绝；重复请求只能恢复同一事实，不应根据客户端状态自行重试不同版本。

SCIM 同步受企业和令牌两级 Redis 限流约束。Redis 或审计持久化不可用时，系统保持失败关闭，不降级为无锁或无审计写入。管理端可在 `GET /api/organizations/{organization_id}/scim/audit-logs` 查看脱敏审计事实，查询仍受企业上下文和权限边界约束。

SCIM 令牌、用户邮箱原文、同步请求正文和上游响应不得写入 URL、日志或客户端代码。部署方应将令牌交给身份目录系统的密钥管理能力，并在轮换时先验证新令牌，再撤销旧令牌。

## 凭据故障处理

看到 `401` 或 `403` 时，先确认使用的是正确凭据类型和企业路径，再检查会话、令牌状态、过期时间、权限和 IP 白名单。看到 `409` 时不要盲目重复写入，应重新读取服务端版本后按 CAS 规则提交。看到 `503` 时检查数据库、Redis、密钥和磁盘状态，不要通过关闭鉴权、改用其他凭据或把服务绑定到公网地址绕过安全边界。
