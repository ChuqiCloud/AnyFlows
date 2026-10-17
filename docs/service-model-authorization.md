# 服务账号模型授权

`af-admin::ServiceAccountModelAuthenticator` 在一次存储快照中解析独立服务凭据及其当前模型授权。它接收 `ServiceAccountKeyDigest`，不接收个人用户 ID、个人 Token 或创建者身份。

## 授权策略

返回的 `ServiceAccountModelAuthentication` 包含服务身份、明确授予的路由分组、`ServiceModelPolicy` 以及正数 RPM 和并发上限。这些是授权事实，执行请求前必须在服务账号命名空间中落实限制。

模型策略使用精确、区分大小写的规范模型名。空白名单拒绝所有模型，下级策略求交只能收窄授权。构造时先限制条目数、单项名称长度和文本总量，再去重。

宿主通过 `RuntimeExtensions::service_account_models` 注册此端口，并配置服务计费和用量存储。运行时以企业和服务账号组成 Redis 限流主体，凭据轮换不重置 RPM 或并发限制。准入存储缺失时拒绝服务请求。

## 支持的请求

服务凭据可访问 `GET /v1/models`，以及同步 Chat Completions、Responses、Anthropic Messages、Gemini 生成接口，包含流式请求。只接受单个 Bearer 头，拒绝查询 Key 和厂商专用 Key 头。

模型目录与请求使用明确的分组及规范模型白名单。推理后缀规范化后再检查白名单，不借用创建者分组或 Playground 回退规则。

音频、Embeddings、Images、Rerank、Compact 和异步 Video 当前仍限制为个人 Token。服务失败历史及未完成计费的持久恢复需发行版提供独立契约；不能把服务失败写到创建者或成员名下。

## 重试与归属

预扣重试保留原预约、身份、金额和绝对到期时间。结算及取消绑定最初凭据，凭据轮换不改变这次请求的归属。用量投影到发行版存储，不写个人记录。

服务粘性会话使用企业、账号和凭据 ID，在独立哈希域生成；个人会话哈希格式保持不变。