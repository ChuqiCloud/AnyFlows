# 商户服务状态事件示例

本示例处理商户服务发布、版本更新、暂停和恢复事件。事件与业务状态变更在同一数据库事务中产生。

插件收到五个公开字段：`eventKey`、`serviceId`、已发布的 `revision`、`status`（`active` 或 `paused`）和 `observedAt`。插件不会收到商户身份、服务说明、模型列表、价格、上游地址或凭据。示例只生成短报告，不修改服务，也不发送通知。

事件至少投递一次。重试会复用同一个 `eventKey`；如果以后接入外部副作用，应先用它做幂等键。当前处理函数只读、可重复运行。

## 打包

在仓库根目录运行：

```bash
python tools/plugins/package-example.py dist/merchant-service-event.afplugin --example merchant-service-event
```

输出文件必须尚不存在。

## 安装和授权

管理员在“插件管理”上传包，启用部署并授予 `events.merchant-service.changed.consume@1`。手工登记的制品需要先审批。

只有启用、已审批、未撤销且拥有精确 capability 的部署会收到事件。事件由持久队列投递，失败时按上限重试并检查部署代次。管理员可在插件事件结果页面或接口查看结果，PAT 需要 `admin:list_admin_plugin_event_results:execute`。

## 修改示例

保持输入输出 Schema 不变，只根据五个公开字段编写逻辑。不要添加 HTTP 请求、Shell 命令、商户查询或写入操作。需要新的事件字段时，先由宿主定义数据投影和权限。
## 本地测试

事件输入样例在 `fixtures/input.json`：

```bash
python tools/plugins/test-plugin.py --root tools/plugins/examples/merchant-service-event --manifest tools/plugins/examples/merchant-service-event/manifest.json --capability events.merchant-service.changed.consume --input tools/plugins/examples/merchant-service-event/fixtures/input.json
```

输出必须包含 `summary`、`status` 和 `recommendation`。本地测试不会写入 AnyFlows 数据库。