# 服务账号身份契约

本文面向扩展开发者。社区核心提供契约，服务账号的管理和存储由扩展实现。

## 身份校验

`GatewayIdentity` 区分个人主体和独立服务身份。个人主体保留用户、Token 和分组 ID；服务身份包含企业、服务账号及凭据 ID，不提供替代用户或成员 ID。

`af-admin::ServiceAccountAuthenticator` 对接发行版的凭据存储。调用方解析 `PresentedServiceAccountKey`，计算带域分离的 SHA-256 摘要，在等待认证前释放明文。明文和摘要类型在释放时清理自有字符串，并对 `Debug` 脱敏；已签发凭据格式保持兼容。

适配器返回 `ServiceAccountAuthentication`，携带经过校验的身份及可选团队、部门范围。每次请求检查凭据、账号生命周期和企业授权。存储失败与无效凭据在内部区分，公开响应不披露拒绝原因。

身份端口不创建登录会话，不选择路由分组，也不授权模型或预留资金。宿主必须显式处理服务身份的路由、准入、计费和日志；异步任务还需独立的所有权契约。

## 计费与用量

每条用量事实绑定一个 `GatewayIdentity`，服务事实不含个人用户或 Token 主体。`BillingRequestPlan` 使用独立的服务激活方法，将原始身份绑定到 `ServiceAccountBillingPort`，用于结算及非阻塞取消补偿。调用方需先确认预扣，重试沿用同一绝对到期时间。

`DatabaseUsageRecordSink` 将服务事实交给发行版的 `UsageRecordProjection`。缺少服务存储时返回错误并保留待投递记录；个人用量继续走 `UsageLogRepository`。服务事实不写入个人用量行。

计费注册不授权模型执行，同步模型还需 [模型授权端口](service-model-authorization.md)。服务身份始终不能转换为个人主体。

## 请求终态

同步服务请求将成功、失败及中断的终态事实发送到 `RuntimeExtensions::service_account_outcomes` 注册的 `ServiceAccountOutcomeSink`。事实保留准入时的身份，包含请求与模型标识、协议、操作、分组、可选渠道、耗时和有限失败类别；不含请求正文、上游响应正文或密钥。

写入端需要幂等追加并拒绝冲突重放。临时存储错误对同一事实最多尝试三次；日志错误不改变模型响应或结算。流取消后移交一次异步写入，进程退出后的恢复不由此接口保证。存储及后续授权查询由发行版负责。