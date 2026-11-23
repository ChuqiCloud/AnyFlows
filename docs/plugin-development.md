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
| 页面补充内容 | `uiViews` / `uiContributions` / `uiPages` | 静态文本、指标，或绑定只读能力的管理员扩展页 | 宿主组件渲染；扩展页也可调用已授权的只读能力 |

插件不能直接访问用户会话、数据库、钱包、API Key、渠道地址、上游凭据或任意 URL。当前实时外部读取只支持宿主固定的 `account.profile` 和 `account.balance` 回调，要求 `GET` 和空查询参数；用户每次运行都要单独同意。

当前没有向插件开放系统 SMTP 投递或平台用户收件人查询能力。批量邮件插件不能直接复用 AnyFlows 邮件配置或枚举用户地址，相关能力需要由宿主提供带权限控制的投递入口。

需要自定义 HTTP 入口时，在清单的 `routes` 中声明路由和受众。管理员、普通用户和模型 Key 分别使用宿主提供的固定前缀，宿主负责认证与能力授权。声明 `access=read`、`sideEffect=none` 且带有效输入/输出 Schema 的自定义能力，可以通过宿主通用入口调用。管理员会话或拥有对应权限码的 PAT 可调用该入口。

```http
POST /api/admin/plugins/{plugin_id}/capabilities/{capability_id}/invoke
Content-Type: application/json

{
  "capability_version": 1,
  "input": { "query": "example" }
}
```

部署必须已启用、制品已批准、该能力版本已授权；宿主会再次校验 Schema、输入输出大小和部署代次。此接口只执行无副作用的只读能力，不替代外部读取、写入、任务提交或业务动作的专用宿主入口。

自定义能力的输入和输出不限于对象，也可以是数组、字符串、数字、布尔值或 `null`，具体由各自的 Schema 决定。调用 API 时，把所需值放在上例的 `input` 字段中。控制台按 Schema 显示表单。需要在页面上填写的输入，请使用控制台支持的字段类型。

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
    "description": {"zh": "搜索和浏览模型目录。", "en": "Search and browse the model catalogue."},
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
| `description` | 已安装插件卡片显示的中英文简介；遵守 `PluginUiText` 的纯文本限制。 |
| `description` | 插件卡片中的简介，可用纯文本或 `{"zh":"...","en":"..."}`。最多 512 字符；不接受 HTML、链接或控制字符。 |
| `pluginVersion` | 插件版本。代码或清单变化后递增。 |
| `capabilities` | 插件需要的固定能力。通用能力使用 `access: read` 和 `sideEffect: none`。 |
| `authenticationModes` | 无凭据插件使用 `["none"]`。这不会授予用户会话或密钥。 |
| `configSchema` | 管理员可填写的公开配置格式。不要放密码、Key 或 Cookie。 |
| `limits` | 插件自己的并发、超时、输出和分页上限。宿主可以进一步收紧。 |
| `uiContributions` | 插件管理页的静态信息卡，最多 16 张。 |
| `uiViews` | 固定业务页面的静态视图，最多 8 项。 |
| `uiPages` | 插件管理页中的管理员扩展页入口，最多 32 项。页面本身绑定只读能力；页面动作可另外绑定具备幂等语义的写能力。 |
| `routes` | 插件自己的声明式 HTTP 入口，统一挂在 `/api/plugins/{plugin_id}` 下，最多 64 条；每条路由绑定一个已声明能力。 |

管理页根据 `configSchema` 显示文本、数字、开关、枚举、嵌套对象和数组字段。点击插件卡片上的「配置」即可读取和修改公开设置。保存会保留已有能力授权并让各节点重新加载插件。开发插件时，请用可渲染的类型声明必填设置。

表单只使用宿主提供的控件。插件不能上传 HTML、JavaScript 或自定义组件。可选的界面提示字段：

| Schema 扩展 | 作用 |
| --- | --- |
| `x-ui-group` | 将字段放入一个小组；可写普通文本，或 `{"zh":"连接","en":"Connection"}`。 |
| `x-ui-order` | 同一层级内的显示顺序，数值越小越靠前。 |
| `x-ui-visible-when` | 按同级字段控制显示，例如 `{"field":"mode","equals":"advanced"}`；支持 `notEquals`。隐藏值会保留。 |
| `x-ui-label`、`x-ui-description` | 中英文界面标签和说明；使用对象格式，如 `{"zh":"区域","en":"Region"}`。 |
| `x-ui-placeholder` | 输入框占位说明，可写中英文对象。 |
| `x-ui-enum-labels` | 与 `enum` 同顺序的选项名称，可写中英文对象数组。 |

Schema 仍须是有效的 JSON Schema；扩展只影响页面呈现，不改变服务端校验。例如：

```json
{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "mode": { "type": "string", "enum": ["simple", "advanced"], "default": "simple" },
    "region": {
      "type": "string",
      "title": "Region",
      "x-ui-label": { "zh": "区域", "en": "Region" },
      "enum": ["us", "eu"],
      "x-ui-group": { "zh": "连接", "en": "Connection" },
      "x-ui-order": 10,
      "x-ui-enum-labels": [
        { "zh": "美国", "en": "United States" },
        { "zh": "欧洲", "en": "Europe" }
      ]
    },
    "endpoint": {
      "type": "string",
      "format": "uri",
      "x-ui-group": { "zh": "连接", "en": "Connection" },
      "x-ui-visible-when": { "field": "mode", "equals": "advanced" }
    },
    "headers": {
      "type": "array",
      "maxItems": 16,
      "items": {
        "type": "object",
        "required": ["name", "value"],
        "properties": {
          "name": { "type": "string" },
          "value": { "type": "string", "format": "textarea" }
        }
      }
    }
  },
  "required": ["mode", "region"]
}
```

不要把密码、API Key、令牌、Cookie 等放入 `configSchema` 或公开配置。密钥应使用 `credential.lifecycle`，由宿主加密保存和注入。

`uiViews` 只能使用宿主组件：`text`、`badge`、`metric`、`table`、`list`、`notice`、`progress` 和 `info_card`。固定槽位是 `plugin_management`、`channel_monitor`、`channel_model_mapping`、`merchant_service` 和 `external_account`。不能放 HTML、CSS、脚本、链接或按钮。`list` 最多 16 项，`notice` 使用 `tone` 标记提示级别，`progress` 使用 0–100 的 `percent`。需要更复杂的页面时，把数据拆成多个视图和组件组合，不要把前端代码塞进插件包。

`uiPages` 为插件的只读能力提供独立页面。管理员从插件管理打开页面，宿主按能力的 `inputSchema` 显示表单，并用固定组件展示结果。被绑定的能力必须声明为 `read` + `none`，并在当前部署中明确授权。清单省略 `input` 时，表单的初始值为 `{}`。例如，模型目录示例可以声明：

```json
{
  "uiPages": [
    {
      "id": "model-catalog",
      "schemaVersion": 1,
      "title": { "zh": "模型目录", "en": "Model catalogue" },
      "description": { "zh": "查看插件提供的模型资料。", "en": "Browse model information provided by this plugin." },
      "capabilityId": "catalog.models.read",
      "capabilityVersion": 1,
      "input": { "query": null, "cursor": null, "limit": 20 }
    }
  ]
}
```

`uiPages` 不能注册任意路由、脚本或菜单项。页面调用仍经过宿主授权接口；插件停用、撤销或取消能力授权后，页面不能继续调用。能力可以返回任意合法 JSON；若返回值为 `{ "blocks": [...] }`，页面会用上面的宿主组件渲染，否则以转义后的 JSON 显示。返回内容必须符合能力自己的 `outputSchema`。新增图表、普通用户页面需要扩展宿主组件和权限契约。

页面可以声明 `actions` 提供写操作。动作必须绑定 `access: write`、`sideEffect: external_write` 且 `writeSemantics.nativeIdempotencyKey: true` 的能力，并在当前部署中明确授权。动作输入按能力 `inputSchema` 校验，页面先创建短期提案，管理员确认后才执行。宿主保存幂等键、插件制品摘要、部署 generation 和输入摘要；输入原文不写入动作表。执行状态可能是 `submitted` 或 `outcome_unknown`，未知结果不能自动重试。插件写能力应使用现有 `PluginTaskOperation::Submit` 格式：

```json
{
  "kind": "submit",
  "model": "vendor-model",
  "request": { "prompt": "..." },
  "idempotency_key": "request-123"
}
```

钱包、任务 ID、凭据、重试和最终结算仍由宿主负责。动作不会直接获得数据库句柄、钱包权限、任意 URL 或宿主后台 API。除任务提交外，动作也可以绑定自定义 JSON 写能力；该能力必须声明完整的 `inputSchema`、`outputSchema` 和 `writeSemantics.nativeIdempotencyKey: true`。通用写动作收到的 `InvocationMetadata.requestId` 固定为 `plugin-action:<proposalKey>`，插件应使用它做幂等去重。

除任务提交外，动作也可以绑定自定义 JSON 写能力。该能力必须声明完整的 `inputSchema`、`outputSchema` 和 `writeSemantics.nativeIdempotencyKey: true`；通用写动作收到的 `InvocationMetadata.requestId` 固定为 `plugin-action:<proposalKey>`，插件应使用它做幂等去重。自定义写入只能作用于插件自己拥有的外部资源，宿主会在确认前后重新检查部署代次、制品摘要、能力授权和输入摘要。执行结果为 `unknown` 时，插件可能已经收到请求，宿主不会自动重试。

动态视图示例：

```json
{
  "blocks": [
    { "type": "metric", "label": { "zh": "模型数量" }, "value": "42" },
    { "type": "table", "columns": [{ "zh": "模型" }, { "zh": "提供商" }], "rows": [[{ "zh": "示例模型" }, { "zh": "示例厂商" }]] }
  ]
}
```

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

槽位表示 AnyFlows 哪个页面可以展示视图；插件不能自行注册 URL 或任意导航项。平台要增加新的业务页面槽位时，需要在契约中增加槽位，并在对应页面接入宿主渲染器。管理员扩展页通过 `uiPages` 单独接入，只能调用已授权的只读能力。两种方式都不执行第三方前端脚本。

### 插件路由

需要让管理员或普通用户通过 HTTP 调用插件自有业务时，在清单中声明 `routes`。`path` 是用于文档和客户端生成的逻辑路径，当前宿主地址使用 `route_id`，例如 `/api/account/plugins/{plugin_id}/routes/{route_id}`。请求方法和 JSON 输入会按清单中的路由声明校验，再转交绑定能力；插件只收到路由绑定能力的 JSON 输入，返回值必须符合该能力的 `outputSchema`。路由不能声明完整 URL、脚本或任意宿主路径。

```json
{
  "routes": [
    {
      "id": "preview",
      "schemaVersion": 1,
      "method": "POST",
      "path": "/preview",
      "audience": "user",
      "capabilityId": "com.example.preview.run",
      "capabilityVersion": 1,
      "maxBodyBytes": 262144,
      "timeoutMs": 30000
    }
  ]
}
```

`GET` 只能绑定 `access=read` 能力。GET 查询参数会作为字符串组成 JSON 对象传给插件，重复参数会组成数组，并受数量和长度上限约束。其他方法使用 JSON 请求体。`POST`、`PUT`、`PATCH` 和 `DELETE` 必须绑定写能力，并继续遵守现有幂等、确认和授权规则。宿主负责认证、限流、输入大小、超时、审计和节点代次校验；插件不能选择上游主机、读取认证头、修改钱包或访问数据库。

### 插件状态存储

插件可以通过 `PluginStorageOperation` 使用宿主提供的命名空间 JSON 存储。每个插件只能读写自己的键空间，键最长 128 字节，值最长 64 KiB，可设置过期时间；支持 `get`、`put`、`delete` 和带前缀的 `list`。用户路由和模型 Key 路由的存储会按当前用户隔离，管理员路由使用插件共享空间。过期值不会从读取结果中返回，后台每分钟按批次清理过期记录。状态适合保存同步游标、Webhook 去重键和任务辅助状态，不适合保存凭据、钱包事实或宿主任务记录。插件不能执行 SQL 或获取数据库连接。

Python SDK 在能力处理函数和路由处理函数中提供 `context.storage(operation)`。它通过当前 Sidecar 调用的关联帧访问宿主存储，宿主会重新校验插件 ID、generation、键和值大小，并由宿主确定用户级隔离空间；插件不能指定其他用户或插件的命名空间。存储不可用时返回固定错误码，插件不应把存储当作计费或资金事实来源。TypeScript、Go 和 Rust SDK 目前提供 JSON Lines 信封和标准错误结果封装，尚未封装存储回调。

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

当前内置事件目录可通过插件契约的 `plugin_event_catalog()` 查询，包含事件名、输入 Schema、能力名和可过滤字段。过滤条件由宿主在入队前执行；插件不会收到不符合条件的事件。清单中的事件必须存在于目录，并且对应 capability 必须同时声明为 `read` + `none`。当前目录包括：`channel.health.changed`、`merchant.service.changed`、`model.catalog.changed`、`channel.config.changed`、`channel.credential.changed`、`model.pricing.changed`、`external.account.balance.changed`、`plugin.deployment.changed` 和 `plugin.task.changed`。事件只包含脱敏引用、版本、状态和时间，不包含 Key、Cookie、钱包金额或任务原文。新增宿主事件时，需要同步登记契约目录、Schema、数据库投递映射和安全投影，再增加业务 producer；不需要改持久队列表结构。

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

### Provider 生命周期适配

Provider 插件可以实现模型发现、请求构造、响应投影、失败分类和重试建议。请求构造只能返回相对路径、普通 Header 和 JSON 请求体；宿主负责凭据、上游主机、实际网络请求、路由、计费和最终重试决定。插件不能读取 Key、选择主机或返回钱包事实。

### 异步任务适配

`invoke_task` 支持 `submit`、`status`、`cancel` 和 `result` 四种任务操作。插件负责把宿主投影的 Provider 响应转换为统一任务状态；宿主负责公开任务 ID、凭据、发送请求、轮询、持久化、取消是否对用户可见、最终重试和结算。`status` 失败时可以返回 `retryable` 和不超过 86400 秒的 `retryAfterSeconds` 建议，宿主可以缩短或忽略该建议。插件不得返回价格、配额、钱包或扣费事实。

### 请求处理

使用 `request.processing.read` 时，先根据输入的 `stage` 判断当前阶段。

- `ingress` 和 `request` 在 Chat 进入 Relay 前运行。插件收到模型、协议、流式标记和内容项数量。宿主把 `ingress` 用作观察入口，`request` 可以修改模型或阻止请求。
- `outbound` 处理 Relay 候选即将发送给上游的 JSON 请求体。
- `egress` 和 `reply` 依次处理 Relay 收到的 JSON 响应体或 SSE 的 JSON 事件。经过 WebSocket 转发的响应也会先转成 SSE。超过 48 KiB 的单份 JSON 只提供省略内容的标记。
- `report` 返回审计发现，输入包含流是否完成和可取得的用量。客户端提前取消或上游断流时，报告标记为未完成。

JSON Patch 支持 `add`、`replace`、`remove`，宿主会拒绝无效路径或受保护字段。Relay 目前只把 JSON 正文的修改应用到转发链路。原始认证 Header、凭据和钱包由宿主管理。

### 凭据生命周期

需要登录外部平台的插件声明 `credential.lifecycle` 能力。宿主按当前凭据调用 `login`、`refresh`、`revoke`、`sign`、`web_socket_auth` 和 `balance` 操作。插件收到的是脱敏的操作参数，不会收到其他用户的凭据、钱包或会话。登录结果可以是授权地址和短期 state；刷新和撤销必须幂等；签名结果只能包含受限 Header、Query 和 Body，不能覆盖 `Host`、`Cookie` 等宿主保护字段。模型渠道的认证 Header 由凭据插件生成；向额外 endpoint 发送时，宿主只带该 endpoint 专用签名结果，不带原模型渠道的认证 Header。余额只返回快照，不得修改 AnyFlows 钱包。

### 普通用户路由

清单中的 `routes[].audience` 可以设置为 `user`。这类路由通过 `/api/account/plugins/{plugin_id}/routes/{route_id}` 调用，使用当前用户会话或 PAT，并且只获得宿主授予的 capability。`admin` 路由只能通过 `/api/admin/plugins/...` 调用。两种受众的输入、输出 Schema、超时、部署代次和存储命名空间都由宿主复核；插件不会收到用户会话、模型 Key 或钱包密文。

### 任务文件和报告

任务进入 `succeeded` 后，可以在 `result.output` 中返回 `taskId`、`files` 和 `report`。文件以 base64 传输，单个文件和总数量受宿主限制；宿主校验 SHA-256 后写入受控存储，客户端得到短期下载引用。报告包含标题、摘要和有限数量的 finding，不能携带凭据、价格或内部路径。

### Relay Provider

声明 `provider.lifecycle` 后，插件会在候选发送前收到规范化的模型、请求方法、相对路径、非敏感 Header 和 JSON Body。插件可以返回新的相对路径、Body 和普通 Header。宿主重新绑定上游主机和认证，并过滤保护字段。响应结束后宿主会回调 `response`；非成功响应还会回调 `classify_failure`。插件的失败不会绕过 Relay 的重试、路由或计费权。

#### 多步 Provider 会话

需要登录、创建网页线程、等待任务完成或多次轮询的上游，可以在部署的 Provider 渠道绑定中打开 `sessionEnabled`。普通 Provider 不要打开此选项；关闭时只调用一次 `request`，不会产生额外的会话尝试。

1. `session_start` 返回 `session`，初始动作可以是 `request`、`web_socket_connect` 或 `complete`。
2. 宿主执行动作，再把结果交给 `session_step`。HTTP 响应使用 `{status, body}`；WebSocket 事件使用 `{kind: "websocket", event: {...}}`。
3. `session_step` 可返回 `request`、`web_socket_connect`、`web_socket_send_text`、`web_socket_send_binary`、`web_socket_receive`、`web_socket_receive_batch`、`web_socket_authenticate`、`web_socket_close`、`wait`、`complete` 或 `abort`。插件用 `state` 保存上游任务 ID、阶段和解析游标。
4. `request` 和 `web_socket_connect` 只引用管理员为当前 Provider 绑定配置的 endpoint ID，并提交相对路径。HTTPS endpoint 还会校验方法；两种 endpoint 都会校验路径前缀和最终网络目标。宿主通过受控网络栈处理 TLS、代理、DNS 和 SSRF；插件不能提供完整 URL，也不能覆盖握手或代理 Header。一个会话可以在 WebSocket 和 HTTP 请求之间切换。
5. `web_socket_receive` 等待一条消息；`web_socket_receive_batch` 最多返回 16 条，消息大小和单批负载均有限制。超出本批负载的帧保留给后续读取。单次等待到时会返回 `timeout`，会话总时限到期则失败。`complete` 返回统一 JSON 响应；`abort` 结束本次候选并进入 Relay 故障分类。
6. `session_close` 在成功、失败、超时或中止后调用。宿主先关闭仍打开的 WebSocket，再调用插件清理；插件可以返回一个 `request`，让宿主再发送一次清理请求。省略 `endpoint_id` 时使用原渠道上游；提供 `endpoint_id` 时使用该 Provider 绑定中授权的 HTTPS endpoint、方法和路径。清理失败不会覆盖已确定的请求结果。客户端直接取消时，宿主会尽力关闭连接并调用 `session_close`，但不会等待清理结果。

会话总时限默认为 30 秒，可在渠道绑定中设置为 1 至 360 秒；每轮最多执行 128 个动作。每个状态、响应和结果最多 256 KiB，每个 WebSocket 消息最多 128 KiB，请求 Header 最多 64 个，路径最多 2048 字节并必须以单个 `/` 开头。二进制帧使用标准 Base64。HTTP 会话步骤使用完整响应缓冲，适合短请求和轮询；长期事件流可由插件通过 WebSocket 批量读取并自行缓冲。第一版不提供通用下游流式桥接，最终结果只在 `complete` 后交给 Relay，不会把中间帧作为客户端输出。

WebSocket 动作字段：

| 动作 | 字段 | 宿主返回给 `session_step` 的事件 |
| --- | --- | --- |
| `web_socket_connect` | `endpoint_id`、`path`、可选 `headers` | `connected` |
| `web_socket_send_text` | `text` | `sent`，包含帧类型和字节数 |
| `web_socket_send_binary` | `data_base64` | `sent`，包含帧类型 |
| `web_socket_receive` | `timeout_ms`，范围 1–30000 | `message`、`ping`、`pong`、`closed` 或 `timeout` |
| `web_socket_receive_batch` | `timeout_ms`、`max_messages`（1–16） | 一次返回多条 `frames`，带 `closed`、`timed_out`、`more_pending` |
| `web_socket_authenticate` | 凭据插件需要的公开 challenge/context | `authenticated`；认证帧由宿主发送，不会返回 Provider 插件 |
| `web_socket_close` | 无 | `closed` |

连接失败会返回 `error` 事件和稳定错误码。握手成功前的网络错误会带 `retryable`，插件可在安全时切换到 HTTP；策略阻止会返回 `retryable: false`。发送或接收阶段的错误会带 `outcome_unknown: true`，表示上游可能已收到请求，插件不要直接重放生成请求，应先查询任务状态或依据已收事件完成结果。

每一步完成后，插件都会收到该事件，再决定继续收发、改用 HTTP、等待或完成响应。二进制入站事件使用 `data_base64`；文本超出 JSON 字符串容量时也会带 `encoding: "base64"`。

`request` 选择 `endpoint_id` 后使用管理员授权的 HTTPS 端点。端点 ID、HTTP 方法和路径前缀必须匹配；显式 endpoint 请求可以携带 `Authorization` 与 `Cookie`，供用户配置的独立上游会话使用。不带 `endpoint_id` 的旧式相对请求仍仅限已选渠道的上游主机，且不能覆盖这两种认证头。

部署配置示例：

```json
{
  "providerSelection": {
    "channelIds": [12],
    "models": ["capy-free-model"],
    "priority": 100,
    "sessionEnabled": true,
    "sessionTimeoutMs": 60000,
    "endpoints": [
      {
        "id": "capy-api",
        "url": "https://api.example.invalid",
        "allowedPaths": ["/v1", "/oauth"],
        "allowedMethods": ["GET", "POST"],
        "websocketEnabled": false
      },
      {
        "id": "capy-realtime",
        "url": "wss://realtime.example.invalid:8443",
        "allowedPaths": ["/realtime"],
        "allowedMethods": [],
        "websocketEnabled": true
      }
    ],
    "upstreamResponseMode": "non_streaming"
  }
}
```

WebSocket 连接动作示例：

```json
{
  "kind": "session",
  "session_id": "thread-123",
  "state": { "phase": "streaming" },
  "action": {
    "kind": "web_socket_connect",
    "endpoint_id": "capy-realtime",
    "path": "/realtime/v1/events?thread=thread-123",
    "headers": { "x-client-version": "1" }
  }
}
```

收到事件后，`session_step.response` 的结构如下。文本内容通常放在 `data`；若 JSON 转义会让响应超过 256 KiB 上限，则宿主改用 `encoding: "base64"` 和 `data_base64`。二进制帧始终用 `data_base64`。Ping 会由宿主自动回复，插件只会收到事件类型；上游发送的 Pong 也会以事件类型报告。

```json
{
  "kind": "websocket",
  "event": {
    "type": "message",
    "frame_type": "text",
    "data": "{\"type\":\"delta\",\"text\":\"hello\"}"
  }
}
```

插件可继续返回 `web_socket_receive` 读取下一帧，或返回 `web_socket_close` 关闭连接。上游输出结束后，插件应把事件整理为 Provider 的标准响应，再返回 `complete`。

应用层认证动作示例：

```json
{
  "kind": "web_socket_authenticate",
  "request": { "challenge": { "nonce": "upstream-challenge" } }
}
```

认证帧由绑定该渠道凭据的 `credential.lifecycle` 插件生成。Provider 在收到公开 challenge 后返回 `web_socket_authenticate`，宿主将 endpoint ID、连接路径及 challenge 交给凭据插件，再通过已打开的 WebSocket 发送其返回的帧。私密认证帧不会进入 Provider 的 `session_step` 输入、结果或管理 API。凭据插件不得更改连接目标。

凭据插件的认证输入和返回结果形如：

```json
{
  "kind": "web_socket_auth",
  "request": {
    "endpoint_id": "capy-realtime",
    "path": "/realtime",
    "challenge": { "nonce": "upstream-challenge" }
  }
}
```

```json
{
  "kind": "web_socket_auth",
  "frames": [
    { "kind": "text", "text": "{\"type\":\"authenticate\",\"token\":\"<由凭据适配器生成>\"}" }
  ]
}
```

`web_socket_receive_batch` 最多一次读取 16 条消息，每帧不超过 128 KiB，单批负载不超过 128 KiB。批次中的文本和二进制内容都用 `data_base64`，避免多条文本转义后超过会话大小限制。超出批次负载的下一帧会留在宿主连接队列，供后续批次读取。`more_pending` 表示本批达到帧数或负载预算；它提示插件可以继续读取，不保证此时已有下一帧排队。批量收帧减少插件往返，但每个批次仍占用一次有界会话步骤。

`session_step` 的返回示例：

```json
{
  "kind": "session",
  "session_id": "thread-123",
  "state": { "phase": "polling", "attempt": 2 },
  "action": {
    "kind": "request",
    "method": "GET",
    "endpoint_id": "capy-api",
    "path": "/v1/thread/thread-123/status",
    "headers": {},
    "body": {},
    "stream": false
  }
}
```

把远端线程 ID、轮询游标等会话推进数据放在 `state` 中。同一 `sessionId` 的重复步骤和清理应保持幂等。插件若自行管理上游会话认证，可按其实现需要在状态中携带短期 token；AnyFlows 将该状态作为插件数据传递，不会解析其字段。会话数据大小受宿主限制。

#### 本地测试会话插件

可以先用 `tools/plugins/test-plugin.py` 完成 Manifest、health 和 Provider 操作三步握手，不需要启动 AnyFlows。准备一个 `session-start.json`：

```json
{
  "kind": "session_start",
  "model": "capy-free-model",
  "request": { "messages": [{ "role": "user", "content": "hello" }] }
}
```

运行：

```bash
python tools/plugins/test-plugin.py \
  --root path/to/plugin \
  --manifest path/to/plugin/manifest.json \
  --capability provider.lifecycle \
  --provider-operation session-start.json
```

脚本会检查协议帧、能力声明、Provider 结果类型和基本的会话动作边界。要测试轮询分支，把 `--provider-operation` 换成 `session-step.json`，内容使用 `kind: "session_step"`、`session_id`、`state` 和宿主投影的 `response`。真实上游请求只在 AnyFlows Relay 中执行；本地脚本不会访问网络，也不会读取渠道 Key。

### 市场发布

插件可以发布到 GitHub Release 或固定 commit 的仓库。市场索引格式和安装约束见 [插件市场](plugin-market.md)。Release 包必须提供完整 SHA-256；Git 源必须固定 40 位 commit。市场只负责发现，安装仍复用管理员审批、能力授权和多节点分发。生产节点只下载预构建 `.afplugin`，不会执行仓库中的构建脚本。
### 其他语言 SDK

仓库同时提供三种最小 SDK：

- `tools/plugins/sdk/typescript/anyflows-plugin.ts`
- `tools/plugins/sdk/go/anyflows_plugin.go`
- `tools/plugins/sdk/rust/anyflows_plugin.rs`

它们都实现相同的 JSON Lines 握手、健康检查和能力调用外层。业务代码只需要实现 handler；协议字段、能力 Schema 和错误边界仍以 `contracts/plugins/v1/schema.json` 为准。Rust SDK 的 `serve` 保持简洁字符串错误接口，发生错误时按结果未知处理；需要区分结果状态时用 `serve_with_outcomes` 返回 `PluginError`，只在能够确认操作未发送或已生效时填写对应状态。

```rust
use anyflows_plugin::{serve_with_outcomes, PluginError, PluginOperationOutcome};

serve_with_outcomes(manifest, |frame| {
    match handle(frame) {
        Ok(value) => Ok(value),
        Err(_) => Err(PluginError::new(
            "temporarily_unavailable",
            PluginOperationOutcome::NotApplied,
        )),
    }
})?;
```

协议文件由 Rust 契约导出，再用以下命令生成语言无关副本和 SDK 元数据：

```bash
cargo run --package af-plugin-contract --example export_contract -- export
python tools/plugins/generate-protocol-sdk.py
```

生成器只复制协议 Schema，不执行插件构建脚本。

### 模型 Key 路由

将路由的 `audience` 设为 `model_key`，可通过 `/v1/plugins/{plugin_id}/{route_id}` 或 `/api/plugins/{plugin_id}/routes/{route_id}` 调用。两条路径和下文的任务接口都使用模型调用 Key。控制台和管理 API 使用会话或相应权限的 PAT。宿主负责校验路由的输入、输出、授权和超时。

管理员可在「插件管理」的部署卡片中打开「Provider 渠道绑定」。选择渠道后，可再限定模型、设置优先级和上游响应模式。`auto` 沿用客户端模式；`non_streaming` 和 `streaming` 只适用于 OpenAI Chat Completions 兼容渠道。强制模式会在调用插件前设置请求中的 `stream` 字段，并由宿主在 Relay 内转换响应格式，因此客户端的流式选择、用量结算和响应格式保持一致。其他协议及非对话操作继续使用客户端模式。

```json
{
  "providerSelection": {
    "priority": 100,
    "upstreamResponseMode": "non_streaming",
    "models": ["vendor-model"],
    "channelIds": [12, 18]
  }
}
```

Provider 插件会在 `request` 操作中看到宿主设置后的请求正文。例如，绑定为 `non_streaming` 时，插件收到的 `request.stream` 为 `false`，且不会收到只适用于流式响应的 `stream_options`。插件应按这个字段选择上游请求方式，不要再自行覆盖 `stream`。旧部署未保存该字段时按 `auto` 处理。

同一渠道和模型有多个插件时，宿主按 `priority` 降序、插件 ID 升序选择。模型留空表示匹配所选渠道上的全部模型。清空全部渠道并保存会移除该部署的绑定。

## 数据面接入

每次 Relay 尝试会冻结所选插件的部署代次。插件返回的请求、响应和失败分类只作用于这次候选。宿主负责上游地址、认证、模型策略、重试上限和计费。

凭据生命周期使用宿主绑定的 `channelId` 与 `credentialId`。`login` 首次调用返回授权地址，回调必须带一次性 `state`；`refresh`、`revoke`、`balance` 和 `sign` 都由宿主在 CAS 状态下执行。插件返回的会话材料由宿主加密保存，插件和普通能力不能读取数据库或钱包。

模型 Key 可调用以下固定入口：

- `POST /v1/plugin-tasks` 提交任务，必须带 `idempotencyKey`、模型和插件能力。
- `GET /v1/plugin-tasks/{taskId}` 查询状态、用量、报告和文件清单。
- `GET /v1/plugin-tasks/{taskId}/files/{fileId}` 下载已完成文件。
- `POST /v1/plugin-tasks/{taskId}/cancel` 请求取消。
- `/v1/plugins/{pluginId}/{routeId}` 调用清单中声明的 model-key 路由。

任务提交时宿主按管理员配置的费率预留额度；状态、文件、报告和 `PluginTaskUsage` 回来后，宿主校验上限并用同一预留号结算。额度不足等确定失败不会进入后台重试。插件确认操作未生效时，宿主结束任务并释放预留；提交结果不明时，宿主会加密保留请求，并用相同幂等键重试，直到插件确认接收。状态、结果读取和取消暂时失败时，宿主按递增间隔重试，最多八次；部署失效、授权变化、超过重试上限或结果结构异常会进入人工核对队列。取消意图会先持久化，插件应安全处理同一 `taskId` 的重复取消。无法自动恢复的任务可由管理员在上游核对后通过管理 API 结算，填写实际单位；确认上游未执行时填写 `0` 以释放预留额度。

### 绑定标准模型任务接口

管理员须在部署中明确授予任务能力。任务能力使用 `access: write`、`sideEffect: external_write`，提供输入和输出 Schema，并声明 `writeSemantics.nativeIdempotencyKey: true`。插件应使用宿主传入的幂等键对外部提交去重。

在部署配置的 `taskPricing` 中，用公开模型 ID 指定可调用的分组和费率。以下配置让分组 1 使用插件能力 `example.image.generate` 处理模型 `example-image` 的图像任务。

```json
{
  "taskPricing": {
    "example-image": {
      "groupIds": [1],
      "quotaPerUnit": 1000,
      "maxUnits": 4,
      "nativeProtocols": {
        "openai_image": "example.image.generate"
      }
    }
  }
}
```

`quotaPerUnit` 是每单位用量消耗的系统额度，`maxUnits` 是单任务的用量上限。宿主先按两者的乘积预留额度，再按插件返回且通过校验的用量结算。同一模型、分组和协议只配置一个已启用的绑定。仅使用 `/v1/plugin-tasks` 时可省略 `nativeProtocols`。

| 协议键 | 接口 | 请求和结果 |
| --- | --- | --- |
| `openai_responses` | `POST /v1/responses`、`GET /v1/responses/{response_id}` | 支持非流式 JSON 和 `background: true`。完成时从任务的 `output.output` 读取 Responses 输出数组。 |
| `openai_image` | `POST /v1/images/generations` | 支持 JSON 生成请求。URL 模式从任务的 `output.data[].url` 读取 HTTPS 地址。使用 `b64_json` 时，宿主编码已落库的图像文件。 |
| `openai_video` | `POST /v1/videos`、`GET /v1/videos/{task_id}`、`GET /v1/videos/{task_id}/content` | 支持 JSON 提交和异步查询。内容接口返回宿主保存的 MP4，或以 HTTP 303 跳转到任务结果的 `output.url`。 |

客户端可用 `Idempotency-Key` 在网络重试时复用原任务。图像请求等待超时后，响应的 `X-AnyFlows-Plugin-Task-Id` 可用于查询 `/v1/plugin-tasks/{taskId}`。标准任务入口目前仅接入上表中的请求方式。

协议生成器运行：

```text
python tools/plugins/generate-protocol-sdk.py
```

它会生成 JSON Schema、结果类型映射、帧大小和 schema 摘要到 `tools/plugins/sdk/generated/`；TypeScript、Go、Rust 和 Python SDK 都使用相同的 `manifest`、`health`、`invoke_capability`、`invoke_task`、`invoke_provider`、`invoke_credential`、`plugin_route` 七种操作。
