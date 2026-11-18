# 插件动作确认

插件动作分为两类：宿主内置动作（例如渠道监控目标的启停）和插件清单通过 `uiPages.actions[]` 声明的自定义写动作。自定义动作绑定一个已授权的 `write` + `external_write` capability，并且必须声明 `writeSemantics.nativeIdempotencyKey: true`、输入 Schema 和输出 Schema。

插件只能返回“启用”或“停用”的建议。它不能直接修改目标，也不能看到目标 ID、渠道、凭据或模型。管理员必须在控制台再次确认，宿主才会执行修改。

## 执行流程

1. 管理员选择已启用的渠道监控插件。
2. 宿主把目标的当前版本和状态固定下来，调用插件生成建议。
3. 宿主保存一条有效期为 5 分钟的动作提案。
4. 管理员查看目标、原状态和新状态，明确确认。
5. 宿主再次检查管理员权限、插件部署代次、制品审批和撤销状态、能力授权、目标版本及当前状态。
6. 所有检查通过后，宿主在同一个数据库事务中更新目标并保存提案结果。

目标被编辑、删除，插件部署被替换或停用，能力授权被撤销，或者提案过期时，确认都会失败。重复提交同一个确认键会返回已保存的结果；使用不同确认键重复提交会被拒绝。

## 管理接口

创建提案：

```http
POST /api/admin/channel-monitor/targets/{id}/action-proposals
Content-Type: application/json

{"plugin_id":"org.example.monitor","expected_target_version":7,"idempotency_key":"f3e9ad05-62e1-4801-9a4e-28c03d225c25"}
```

确认提案：

```http
POST /api/admin/channel-monitor/action-proposals/{proposal_key}/confirm
Content-Type: application/json

{"confirmation_key":"c930c70b-aee9-40fd-9c77-c77db728c02a"}
```

两个接口都要求管理员会话，或管理员个人访问令牌（PAT）及对应权限：

- `admin:propose_admin_channel_monitor_target_action:execute`
- `admin:confirm_admin_channel_monitor_action_proposal:execute`

## 自定义写动作

自定义动作不会直接获得宿主数据库、钱包、凭据或后台接口。宿主只把清单声明的 JSON 输入交给插件，并在执行前后校验部署代次、制品摘要、能力授权和输入摘要。页面先创建 5 分钟提案，管理员确认后才执行；插件收到的 `InvocationMetadata.requestId` 固定为 `plugin-action:<proposalKey>`，用于幂等去重。任务适配插件可以继续使用 `PluginTaskOperation::Submit`，其他插件可以定义自己的 JSON 输入。

确认结果有三种常见状态：`completed` 表示插件返回成功，`submitted` 表示异步任务已经提交，`outcome_unknown` 表示宿主无法确认上游结果。`outcome_unknown` 不能自动重试，需要插件或管理员根据外部系统状态处理。

## 插件限制

宿主内置动作仍只修改它明确负责的业务字段；自定义动作只能写入插件自己拥有的外部资源。

插件不能：

- 自定义动作 ID、目标字段或接口地址；
- 访问凭据、数据库、SQL 或任意网络；
- 修改钱包、余额、价格、结算、权限或路由；
- 发送通知、执行命令或直接提交业务写入。

Sidecar 的普通能力仍只允许宿主授予的只读能力。渠道监控示例见[渠道监控事件检查](../tools/plugins/examples/channel-monitor-review/README.md)。
