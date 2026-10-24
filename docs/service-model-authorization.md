# 服务账号模型授权

本文面向扩展开发者，服务账号默认不在社区版启用。身份及存储职责见 [服务账号身份接口](service-identity.md)。

`af-admin::ServiceAccountModelAuthenticator` 通过同一存储快照校验服务凭据及当前模型授权。接口接收 `ServiceAccountKeyDigest`，不使用个人用户 ID、个人 Token 或创建者身份。

## 授权策略

返回的 `ServiceAccountModelAuthentication` 包含服务身份、授予的路由分组、`ServiceModelPolicy`、RPM 和并发上限。RPM 和并发上限均须为正数，并在执行请求前按服务账号独立检查。

模型白名单使用精确、区分大小写的规范模型名，空名单拒绝所有模型。下级策略通过求交收窄授权。构造策略时先检查条目数、单项名称长度和文本总量，再去重。

扩展通过 `RuntimeExtensions::service_account_models` 注册认证器，并配置服务计费和用量存储。Redis 限流按企业和服务账号计数，轮换凭据不会重置 RPM 或并发限制。缺少准入存储时拒绝请求。

## 支持的请求

服务凭据可访问 `GET /v1/models`，以及同步 Chat Completions、Responses、Anthropic Messages 和 Gemini 生成接口，支持流式请求。凭据仅通过单个 `Authorization: Bearer` 请求头传递，不接受查询参数或厂商专用 Key 请求头。

模型目录和推理请求使用服务账号获授的分组和模型白名单。模型推理后缀规范化后再检查授权，不继承创建者分组或 Playground 回退规则。

音频、Embeddings、Images、Rerank、Compact 和异步 Video 仅接受个人 Token。服务请求结果由独立的 `ServiceAccountOutcomeSink` 存储，不能归入创建者或成员名下。未完成计费的进程重启恢复由发行版实现。

## 重试与归属

预扣重试须保留原预约、身份、金额和绝对到期时间。结算和取消操作绑定请求最初使用的凭据，轮换凭据不改变已开始请求的归属。用量写入扩展存储。

服务账号的粘性会话由企业、账号和凭据 ID 在独立哈希域生成，与个人会话隔离。
