# 插件动作确认

当前只有一个可执行的插件动作：根据渠道监控事件，建议启用或停用一个监控目标。

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

## 插件限制

这是宿主登记的固定动作，不是通用写权限。当前动作只允许修改监控目标的 `enabled` 状态。

插件不能：

- 自定义动作 ID、目标字段或接口地址；
- 访问凭据、数据库、SQL 或任意网络；
- 修改钱包、余额、价格、结算、权限或路由；
- 发送通知、执行命令或直接提交业务写入。

Sidecar 的普通能力仍只允许宿主授予的只读能力。渠道监控示例见[渠道监控事件检查](../tools/plugins/examples/channel-monitor-review/README.md)。