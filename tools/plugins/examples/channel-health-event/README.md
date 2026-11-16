# 渠道健康事件插件示例

这个示例消费 AnyFlows 在渠道监控状态从正常变为 `down`、或从 `down` 恢复时创建的事件。宿主将事件放入持久队列，在 Sidecar 可用时交给插件处理，并保存有界结果供管理员查看。插件不会运行探测、访问渠道凭据、读取完整日志、修改渠道状态或触发通知。

## 输入和输出

插件只收到以下字段：

```json
{
  "targetId": 8,
  "channelId": 12,
  "status": "down",
  "observedAt": 1800000010
}
```

输入由宿主 schema 固定。`status` 只能是 `down` 或 `recovered`；时间是 Unix 秒。输入不含 URL、模型名、凭据、响应正文或用户资料。

插件必须返回 `summary`、`status` 和 `recommendation` 三个字段。宿主会验证输出 schema，限制结果大小，并且只把文本作为报告显示；输出不会被当作通知、命令或渠道配置。

## 打包与安装

在 AnyFlows 仓库根目录运行：

```sh
python tools/plugins/package-example.py dist/channel-health-event.afplugin --example channel-health-event
```

上传包后，管理员需启用部署并明确授予 `events.channel-health.changed.consume@1`。没有精确授权、制品未审核/已撤销或部署代次不匹配时，事件不会投递。已受理的事件会使用持久队列，失败后按有界退避重试；到达次数或保留期限后进入死信。

管理员可调用 `GET /api/admin/plugins/events/results?before_id=<deliveryId>&limit=20` 分页读取未过期结果；所需 PAT scope 为 `admin:list_admin_plugin_event_results:execute`。普通用户不能读取。结果仅是文本数据，不影响健康分、实时路由、钱包或用户通知。

## 修改规则

在 `plugin.py` 的 `review_event` 中增加分析逻辑时，保持固定输入字段与固定输出结构，并继续对输入做本地校验。修改代码后增加 `pluginVersion`，重新打包并升级。不要在插件中实现 HTTP 客户端、Shell 命令或常驻任务；需要新事件类型时，应先由宿主定义数据投影和权限。