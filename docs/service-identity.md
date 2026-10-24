# 服务账号身份接口

本文面向扩展开发者。社区核心提供服务账号的身份、计费和用量接口；账号管理与存储由发行版扩展实现，社区版默认不启用。

## 身份校验

`GatewayIdentity` 区分个人主体与服务账号。个人主体包含用户、Token 和分组 ID；服务身份包含企业、服务账号和凭据 ID，不关联替代用户或成员 ID。

`af-admin::ServiceAccountAuthenticator` 对接扩展的凭据存储。调用方解析 `PresentedServiceAccountKey`，计算带域分离的 SHA-256 摘要，再释放明文并发起认证。明文和摘要类型在释放时清理持有的字符串，`Debug` 输出脱敏。

认证器返回 `ServiceAccountAuthentication`，包含已校验的身份及可选团队、部门范围。实现方须在每次请求中检查凭据、账号状态和企业授权，并区分存储故障与无效凭据，避免向客户端泄露内部拒绝原因。

身份认证仅确认主体。扩展还需配置路由、模型准入、计费和日志接口；后台登录及异步任务所有权分别处理。

## 计费与用量

每条用量记录绑定一个 `GatewayIdentity`。`BillingRequestPlan` 通过服务账号专用激活方法，将原始身份绑定到 `ServiceAccountBillingPort`，用于结算和非阻塞取消补偿。调用方需先确认预扣，重试时保留原绝对到期时间。

`DatabaseUsageRecordSink` 将服务账号用量交给扩展的 `UsageRecordProjection`。缺少服务存储时返回错误并保留待投递记录。个人用量使用 `UsageLogRepository`，两类记录分别存储。

模型调用还需配置 [模型授权接口](service-model-authorization.md)。服务身份不能转换为个人主体。

## 请求终态

同步服务请求通过 `RuntimeExtensions::service_account_outcomes` 注册的 `ServiceAccountOutcomeSink` 记录成功、失败和中断结果。记录保留准入时的身份，以及请求 ID、模型、协议、操作、分组、可选渠道、耗时和失败类别，不含请求正文、上游响应正文或密钥。

存储实现须支持幂等追加并拒绝冲突记录。发生临时存储错误时，同一记录最多尝试写入三次。记录失败不改变模型响应或结算。流取消后异步写入一次，进程退出后的恢复和记录查询由扩展实现。
