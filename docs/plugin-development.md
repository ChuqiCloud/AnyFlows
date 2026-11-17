# 插件开发

本文说明如何制作、打包和安装 AnyFlows 插件。插件通过标准输入输出与宿主通信，可以使用 Python、Go、Rust 等语言实现。最容易上手的方式是复制仓库中的 Python 示例。

## 1. 先确认能力范围

插件可以使用宿主固定业务入口，也可以声明自己的 namespaced 只读能力。清单不会自动创建页面、服务端路由或后台任务；自定义能力通过统一的宿主能力入口执行。

“上传并安装”会自动授予清单中通过校验的自定义只读 capability，条件是同时提供有效的输入和输出 JSON Schema，且 `access` 为 `read`、`sideEffect` 为 `none`。这只代表插件获准响应宿主发起的通用能力调用，不会自动接入某个业务模块。`external_read`、写能力以及外部任务提交仍需管理员在完整权限审核中明确授权；任务提交还必须声明写副作用恢复语义。

| 入口 | capability | 插件收到的数据 | 插件可以返回什么 |
| --- | --- | --- | --- |
| 模型资料目录 | `catalog.models.read`、`catalog.models.search`（v1） | 插件自己的公开模型资料、搜索词和分页游标 | 模型名称、提供商、描述、能力标签等展示资料 |
| 商户服务检查 | `merchant.service.review.read`（v1） | 商户选中的服务说明和公开模型能力 | 摘要和检查项 |
| 渠道监控检查 | `channel.monitor.events.review.read`（v1） | 管理员选中的监控事件摘要 | 摘要、检查项和建议 |
| 监控动作建议 | `channel.monitor.target.action-propose.read`（v1） | 受限的监控事件和当前启用状态 | 固定的启用/停用建议；管理员确认后才会执行 |
| 渠道模型映射检查 | `channel.model_mapping.review.read`（v1） | 管理员选中的公开模型名和映射值 | 摘要、检查项和建议 |
| 渠道健康事件 | `events.channel-health.changed.consume`（v1） | `eventKey`、状态变化和时间 | 有界的处理结果 |
| 商户服务事件 | `events.merchant-service.changed.consume`（v1） | `eventKey`、服务 ID、版本、状态和时间 | 有界的处理结果 |
| 自定义只读能力 | 清单中的 namespaced capability | 符合 `inputSchema` 的 JSON 值 | 符合 `outputSchema` 的 JSON 值；管理员可通过宿主通用入口调用 |
| 已保存余额快照分析 | `account.balance.snapshot.review.read`（v1） | 本人最多 20 条余额快照 | 摘要、检查项和建议 |
| 当前余额读取 | `account.balance.snapshot.live-review.read` + `external.account.balance.read`（均为 v1） | 用户本次同意后由宿主返回的当前余额投影 | 摘要、检查项和建议 |
| 固定页面补充内容 | `uiViews` / `uiContributions` | 插件清单中的静态文本和指标 | 宿主组件渲染的卡片、徽标、指标或表格 |

插件不能直接访问用户会话、数据库、钱包、API Key、渠道地址、上游凭据或任意 URL。当前实时外部读取只支持宿主固定的 `account.profile` 和 `account.balance` 回调，要求 `GET` 和空查询参数；用户每次运行都要单独同意。

插件不能自行注册服务端路由、模型、渠道、价卡、Webhook 或计划任务。声明 `access=read`、`sideEffect=none` 且带有效输入/输出 Schema 的自定义能力可以直接由宿主通用入口执行，无须为每个能力新增 Rust handler。管理员会话或拥有对应管理员权限码的 PAT 可调用：

```http
POST /api/admin/plugins/{plugin_id}/capabilities/{capability_id}/invoke
Content-Type: application/json

{
  "capability_version": 1,
  "input": { "query": "example" }
}
```

部署必须已启用、制品已批准、该能力版本已授权；宿主会再次校验 Schema、输入输出大小和部署代次。此接口只执行无副作用的只读能力，不替代外部读取、写入、任务提交或业务动作的专用宿主入口。

自定义能力的输入和输出不限于对象，也可以是数组、字符串、数字、布尔值或 `null`，具体由各自的 Schema 决定。请求外层仍是上例的对象；把所需 JSON 值放在 `input` 字段中。管理页面对对象 Schema 提供字段编辑器，其他类型使用 JSON 输入框；对象编辑器中的“高级 JSON 配置”也可以编辑完整输入。

固定业务页面仍只支持清单声明的宿主渲染视图。需要新业务页面、用户操作或可改变平台状态的功能时，宿主仍需提供对应的数据投影、权限边界和动作确认流程。

## 2. 宿主实际发送和接收的数据

开发插件时，先看示例目录中的 `manifest.json`。其中每个 capability 的 `inputSchema` 定义宿主会传入的 `input`，`outputSchema` 定义插件必须返回的 `output`。Schema 不能自动生成正确的业务数据，所以每个示例都提供了可直接使用的 `fixtures/input.json`。

### 请求外层

每次调用都是一行 JSON。下面是固定的请求外层；`pluginId`、`pluginVersion`、`capabilityId` 和版本号来自清单，不能自行改名：

```json
{
  "protocolVersion": 1,
  "pluginId": "org.anyflows.example-catalog",
  "pluginVersion": "1.0.0",
  "artifactSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "deploymentGeneration": 1,
  "requestId": "local-plugin-test",
  "deadlineUnixMs": 1890000000000,
  "operation": {
    "operation": "invoke_capability",
    "connectionId": "local-test",
    "publicConfig": {},
    "capabilityId": "catalog.models.search",
    "capabilityVersion": 1,
    "input": {
      "query": "vision",
      "cursor": null,
      "limit": 20
    }
  }
}
```

握手阶段还会收到两种请求：

```json
{"operation":"manifest"}
{"operation":"health"}
```

实际请求仍包含上一段列出的外层关联字段。插件必须先正确响应 `manifest` 和 `health`，宿主才会发送能力调用。

### 响应外层

插件响应必须回显 `protocolVersion`、`pluginId`、`pluginVersion`、`artifactSha256`、`deploymentGeneration` 和 `requestId`，然后在 `result` 中返回结果。

Manifest 响应：

下面为了便于阅读省略了完整的 `manifest` 内容和部分回显字段。实际响应必须回显上一段列出的关联字段，`result.value` 必须是完整的 Manifest。

```json
{
  "protocolVersion": 1,
  "pluginId": "org.anyflows.example-catalog",
  "pluginVersion": "1.0.0",
  "artifactSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "deploymentGeneration": 1,
  "requestId": "local-plugin-test-manifest",
  "result": {"kind": "manifest", "value": {"pluginId": "org.anyflows.example-catalog"}}
}
```

健康响应的 `result.kind` 是 `health`，能力成功响应的格式是：

```json
{
  "result": {
    "kind": "capability",
    "value": {
      "output": {
        "summary": "检查完成",
        "checks": []
      }
    }
  }
}
```

能力失败时不会返回 Python 异常堆栈，而是返回固定错误码：

```json
{"result":{"kind":"error","value":{"code":"invalid_response","outcome":"not_applied"}}}
```

常见错误码包括 `invalid_configuration`、`unsupported`、`forbidden`、`rate_limited`、`temporarily_unavailable`、`upstream_failure` 和 `invalid_response`。

### 每个示例的输入和输出

| 示例 | 测试输入 | capability | 输出格式 |
| --- | --- | --- | --- |
| 模型目录 | `examples/catalog/fixtures/input.json` | `catalog.models.read` 或 `catalog.models.search` | `{ "models": [...], "nextCursor": string/null }` |
| 商户服务检查 | `examples/merchant-service-review/fixtures/input.json` | `merchant.service.review.read` | `{ "summary": string, "checks": [...] }` |
| 渠道监控检查 | `examples/channel-monitor-review/fixtures/input.json` | `channel.monitor.events.review.read` | `{ "summary": string, "checks": [...], "actionProposal": ... }` |
| 模型映射检查 | `examples/channel-model-mapping-review/fixtures/input.json` | `channel.model_mapping.review.read` | `{ "summary": string, "checks": [...] }` |
| 渠道健康事件 | `examples/channel-health-event/fixtures/input.json` | `events.channel-health.changed.consume` | `{ "summary": string, "status": "info"/"warning", "recommendation": string }` |
| 商户服务事件 | `examples/merchant-service-event/fixtures/input.json` | `events.merchant-service.changed.consume` | `{ "summary": string, "status": "info"/"warning", "recommendation": string }` |
| 余额快照分析 | `examples/account-balance-snapshot-review/fixtures/input.json` | `account.balance.snapshot.review.read` | `{ "summary": string, "checks": [...] }` |
| 实时余额分析 | `examples/account-balance-live-review/fixtures/input.json` | `account.balance.snapshot.live-review.read` | `{ "summary": string, "checks": [...] }` |

检查类输出中的每个 `checks` 项都必须包含 `code`、`title`、`status`、`message` 和 `recommendation`。`status` 只能使用 `pass`、`info`、`warning`、`fail`。宿主会再次检查字段、长度、数量和总大小；通过 Manifest 不能放宽宿主限制。

## 3. 本地测试

不需要启动完整 AnyFlows。仓库提供的测试工具会启动插件进程，依次发送 `manifest`、`health` 和 `invoke_capability` 三个请求，并检查响应类型、关联字段、退出码和可选的期望输出。

先在仓库根目录生成示例包，或直接测试示例源码目录。直接测试模型目录示例：

```bash
python tools/plugins/test-plugin.py \
  --root tools/plugins/examples/catalog \
  --manifest tools/plugins/examples/catalog/manifest.json \
  --capability catalog.models.search \
  --input tools/plugins/examples/catalog/fixtures/input.json
```

测试商户服务示例：

```bash
python tools/plugins/test-plugin.py \
  --root tools/plugins/examples/merchant-service-review \
  --manifest tools/plugins/examples/merchant-service-review/manifest.json \
  --capability merchant.service.review.read \
  --input tools/plugins/examples/merchant-service-review/fixtures/input.json
```

测试实时余额示例时，还要模拟宿主的余额回调：

```bash
python tools/plugins/test-plugin.py \
  --root tools/plugins/examples/account-balance-live-review \
  --manifest tools/plugins/examples/account-balance-live-review/manifest.json \
  --capability account.balance.snapshot.live-review.read \
  --input tools/plugins/examples/account-balance-live-review/fixtures/input.json \
  --proxy-response tools/plugins/examples/account-balance-live-review/fixtures/proxy-account-balance.json
```

成功时会输出类似 `{"manifest":"ok","health":"ok","result":"capability"}` 并返回退出码 0。插件返回错误、协议帧格式错误、没有及时退出或回调关联失败时，命令返回非 0。`--expect expected.json` 可以把 `result.value.output` 与期望 JSON 做完全比较；测试拒绝分支时可加 `--allow-error`。

运行 SDK 和示例工具的回归测试：

```bash
python -m unittest discover -s tools/plugins/tests -p "test_*.py"
```

自定义插件还可以直接导入业务函数做单元测试，再用 `test-plugin.py` 做一次协议测试。前者检查业务规则，后者检查清单、握手、JSON Lines、输出格式和进程退出行为。实时余额测试必须模拟 `account.balance` continuation 响应，不能让插件直接访问上游。

## 4. 最快开始：打包现有示例

准备 Python 3.10 或更新版本。进入仓库根目录，运行：

```bash
mkdir -p dist
python tools/plugins/package-example.py dist/catalog-example.afplugin --example catalog
```

Windows PowerShell：

```powershell
New-Item -ItemType Directory -Force dist
python tools/plugins/package-example.py dist/catalog-example.afplugin --example catalog
```

输出文件必须不存在。可用的示例名称：

- `catalog`
- `merchant-service-review`
- `channel-monitor-review`
- `channel-model-mapping-review`
- `channel-health-event`
- `merchant-service-event`
- `account-balance-snapshot-review`
- `account-balance-live-review`

示例打包器会复制示例和 Python SDK，生成 `.afplugin` 文件；示例目录中的 `fixtures/` 测试文件不会放进安装包。它不会安装服务器依赖，也不会启动插件。

## 5. 制作自己的插件

复制一个最接近需求的示例目录，例如：

```text
my-merchant-review/
├── manifest.json
├── plugin.py
└── anyflows_plugin.py
```

修改顺序：

1. 把 `manifest.json` 的 `pluginId` 改成自己的唯一 ID，例如 `com.example.merchant-review`。
2. 修改 `displayName`。
3. 每次发布新包时递增 `pluginVersion`。
4. 按清单中的输入和输出 Schema 修改 `plugin.py`。
5. 保留 capability ID、版本和字段结构；需要改契约时先由宿主发布新版本。
6. 把 `tools/plugins/sdk/python/anyflows_plugin.py` 复制到分发目录。
7. 使用通用打包器生成新文件：

```bash
python tools/plugins/package-plugin.py \
  my-merchant-review \
  my-merchant-review/manifest.json \
  plugin.py \
  dist/my-merchant-review.afplugin
```

Windows PowerShell：

```powershell
Copy-Item tools/plugins/sdk/python/anyflows_plugin.py my-merchant-review/anyflows_plugin.py
python tools/plugins/package-plugin.py my-merchant-review my-merchant-review/manifest.json plugin.py dist/my-merchant-review.afplugin
```

参数依次是：分发目录、清单文件、相对分发目录的入口文件、尚不存在的输出包。

## 6. 安装和运行

管理员在“插件管理”中上传 `.afplugin`：

1. 上传包并安装。
2. 如果页面提示需要授权，进入“高级管理”，授予示例要求的 capability。
3. 保持示例要求的 `publicConfig`。模型目录可使用公开配置，其他示例通常要求 `{}`。
4. 启用部署。
5. 等节点状态显示“就绪”。
6. 打开对应业务页面，选择插件并运行。

手工登记的制品需要先审批。普通自有插件可以不签名；签名只用于显示发布者或官方来源，不会自动增加权限。安装和多节点处理见[插件安装与运维](plugin-sidecars.md)。

## 7. Manifest 清单

最小清单示例：

```json
{
  "manifest": {
    "pluginId": "org.example.catalog",
    "displayName": "Example catalog",
    "pluginVersion": "1.0.0",
    "contractRange": {
      "minimum": {"major": 1, "minor": 0},
      "maximum": {"major": 1, "minor": 0}
    },
    "capabilities": [
      {
        "id": "catalog.models.read",
        "version": 1,
        "access": "read",
        "sideEffect": "none",
        "inputSchema": {"type": "object"},
        "outputSchema": {"type": "object"}
      }
    ],
    "authenticationModes": ["none"],
    "limits": {
      "maxConcurrency": 1,
      "requestTimeoutMs": 1000,
      "maxResponseBytes": 1048576,
      "maxPageSize": 100
    },
    "configSchema": {"type": "object", "additionalProperties": false}
  },
  "executablePath": "bin/example-plugin"
}
```

常用字段：

| 字段 | 说明 |
| --- | --- |
| `pluginId` | 插件唯一 ID。升级时不能改变。 |
| `displayName` | 控制台显示名称。 |
| `pluginVersion` | 插件版本。代码或清单变化后递增。 |
| `capabilities` | 插件需要的固定能力。通用能力使用 `access: read` 和 `sideEffect: none`。 |
| `authenticationModes` | 无凭据插件使用 `["none"]`。这不会授予用户会话或密钥。 |
| `configSchema` | 管理员可填写的公开配置格式。不要放密码、Key 或 Cookie。 |
| `limits` | 插件自己的并发、超时、输出和分页上限。宿主可以进一步收紧。 |
| `uiContributions` | 插件管理页的静态信息卡，最多 16 张。 |
| `uiViews` | 固定业务页面的静态视图，最多 8 项。 |

`uiViews` 只能使用宿主组件：`text`、`badge`、`metric`、`table`、`list`、`notice`、`progress` 和 `info_card`。固定槽位是 `plugin_management`、`channel_monitor`、`channel_model_mapping`、`merchant_service` 和 `external_account`。不能放 HTML、CSS、脚本、链接或按钮。`list` 最多 16 项，`notice` 使用 `tone` 标记提示级别，`progress` 使用 0–100 的 `percent`。需要更复杂的页面时，把数据拆成多个视图和组件组合，不要把前端代码塞进插件包。

一个视图示例：

```json
{
  "id": "service-status",
  "schemaVersion": 1,
  "slot": "merchant_service",
  "blocks": [
    { "type": "notice", "title": { "zh": "检查完成" }, "description": { "zh": "发现两项需要关注的配置。" }, "tone": "warning" },
    { "type": "list", "items": [{ "zh": "模型映射完整" }, { "zh": "价格仍待确认" }] },
    { "type": "progress", "label": { "zh": "检查进度" }, "percent": 75 }
  ]
}
```

槽位表示 AnyFlows 哪个页面可以展示视图；插件不能自行注册 URL 或菜单。平台要开放新页面时，需要在契约中增加槽位，并在对应页面接入宿主渲染器；组件只需扩展 Manifest 校验和宿主渲染器。这样插件可以自由组合已支持的展示内容，同时不需要执行第三方前端脚本。

## 8. Python 处理函数

SDK 负责启动握手、健康检查、JSON Lines 协议、超时和结果校验。业务代码只需要实现处理函数。

三参数处理函数：

```python
def review_service(capability_id, public_config, value):
    service = value["service"]
    return {
        "summary": f"已检查：{service['name']}",
        "checks": [],
    }
```

需要外部账户固定回调时，声明第四个 `context` 参数：

```python
def review_live_balance(capability_id, public_config, value, context):
    current = context.proxy_read("account.balance")
    return make_report(value["snapshots"], current)
```

回调只能使用宿主登记的 endpoint ID。插件拿不到 URL、路径、请求头、请求体、凭据或账户 ID。一次调用最多四次回调，所有回调共用原请求的截止时间。

标准输出只允许写协议 JSON。调试信息写标准错误，不要输出用户资料、凭据或完整异常堆栈。

## 9. 各入口的关键约束

### 模型资料目录

输入包括搜索词、页大小和游标。输出必须包含 `models` 和 `nextCursor`。模型资料只用于展示，不会创建可调用模型、渠道、价格或 Key。宿主会检查模型 ID、字段长度、分页游标和重复项。

### 商户服务检查

插件只收到服务名称、说明、公开协议和模型能力。结果包含 `summary` 和 `checks`。每项检查必须有 `code`、`title`、`status`、`message`、`recommendation`；状态只能是 `pass`、`info`、`warning`、`fail`。

### 渠道监控和模型映射检查

插件处理管理员选择的数据，返回文字建议。不能连接渠道、读取 Key、发起探测或修改配置。动作建议仍需管理员确认，详见[插件动作确认](plugin-actions.md)。

### 持久事件

渠道健康事件使用 `down` / `recovered`；商户服务事件使用 `active` / `paused`。输入包含稳定的 `eventKey`，重试时保持不变。插件输出会进入持久队列结果页，不会自动变成通知或业务写入。管理员结果接口需要对应的管理 PAT 权限。

插件可以在清单中声明 `eventSubscriptions`，让宿主和管理界面直接知道它要接收哪些事件。例如：

```json
{
  "eventKind": "channel.health.changed",
  "capabilityId": "events.channel-health.changed.consume",
  "capabilityVersion": 1,
  "filters": { "status": ["down"] }
}
```

当前内置事件目录可通过插件契约的 `plugin_event_catalog()` 查询，包含事件名、输入 Schema、能力名和可过滤字段。过滤条件由宿主在入队前执行；插件不会收到不符合条件的事件。清单中的事件必须存在于目录，并且对应 capability 必须同时声明为 `read` + `none`。新增宿主事件时，需要同步登记契约目录、数据库投递映射和安全投影，再增加业务 producer；不需要改持久队列表结构。

### 外部账户

快照分析只读取 AnyFlows 已保存的数据。实时读取必须同时满足：用户本次勾选同意、插件拥有分析 capability、部署拥有 `external.account.balance.read@1`。余额读取会保存新的快照，但不会充值、扣款或修改 AnyFlows 钱包。

## 10. 调试和排错

| 现象 | 检查方法 |
| --- | --- |
| 包被拒绝 | 检查 `manifest.json`、入口路径、输出文件是否已存在。 |
| 安装后看不到插件 | 检查部署是否启用、节点是否就绪、capability 是否精确匹配。 |
| `invalid_configuration` | 检查 `publicConfig` 是否符合 Schema，示例通常要求 `{}`。 |
| `invalid_response` | 检查必填字段、状态值、数量、长度和总输出大小。 |
| 启动失败 | 检查每个节点的解释器、架构、权限和依赖。 |
| 页面没有数据 | 确认登录用户有入口权限，插件部署代次和 capability 没有变化。 |
| 输出日志异常 | 标准输出只能写协议；调试文字改写标准错误。 |

修改 `manifest.json`、`plugin.py` 或资源后，递增 `pluginVersion` 并生成新包。不要覆盖已经登记的制品。

协议字段见[插件协议参考](plugin-protocol-reference.md)，安装、升级和节点问题见[插件安装与运维](plugin-sidecars.md)。

## 新增数据面契约

### Provider 请求转换

`provider.request.transform` 为渠道适配提供受限转换入口。插件只能处理宿主过滤后的协议、方法、路径、模型、普通请求头和 JSON 请求体，返回继续或拒绝，以及受限的路径、请求体和普通请求头变更。认证头、Cookie、渠道密钥、上游主机、模型计费字段、钱包和结算事实始终由宿主控制。本契约不授予插件任意 HTTP 客户端。

### 异步任务适配

`invoke_task` 支持 `submit`、`status` 和 `result` 三种任务操作。插件负责把宿主投影的 Provider 响应转换为统一任务状态；宿主负责公开任务 ID、凭据、发送请求、轮询、持久化、重试和结算。插件不得返回价格、配额、钱包或扣费事实。

### 请求处理

`request.processing.read` 支持 `ingress`、`request`、`outbound`、`egress`、`reply`、`report` 六个阶段。插件接收脱敏快照，可以返回审计发现和受限处理提案。宿主拒绝修改认证、计费和钱包字段；`reply` 与 `report` 阶段只能观察和审计。

当宿主显式执行多个阶段时，`continue` 结果中的 `json_patch` 和 `header_patch` 会按顺序应用到下一阶段输入。JSON Patch 只支持 `add`、`replace`、`remove`；中间路径必须存在，`replace` 和 `remove` 的目标项也必须存在，`add` 可新增对象字段或插入数组项。缺失路径、根替换和受保护字段都会使整批提案失败，且不会留下部分修改。头部修改按不区分大小写的名称合并，认证、Cookie、Host、内容长度和协议控制头不能修改。插件只能影响这条插件处理流水线的快照，真实转发、路由、计费和结算仍由宿主决定。
