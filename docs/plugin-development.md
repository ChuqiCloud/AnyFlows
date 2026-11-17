# 插件开发

本文说明如何制作、打包和安装 AnyFlows 插件。插件通过标准输入输出与宿主通信，可以使用 Python、Go、Rust 等语言实现。最容易上手的方式是复制仓库中的 Python 示例。

## 1. 先确认能力范围

上传插件只能使用宿主已经接入的固定入口。插件清单中的 capability 只声明需求，不会自动创建页面、接口或后台任务。

| 入口 | capability | 插件收到的数据 | 插件可以返回什么 |
| --- | --- | --- | --- |
| 模型资料目录 | `catalog.models.read`、`catalog.models.search`（v1） | 插件自己的公开模型资料、搜索词和分页游标 | 模型名称、提供商、描述、能力标签等展示资料 |
| 商户服务检查 | `merchant.service.review.read`（v1） | 商户选中的服务说明和公开模型能力 | 摘要和检查项 |
| 渠道监控检查 | `channel.monitor.events.review.read`（v1） | 管理员选中的监控事件摘要 | 摘要、检查项和建议 |
| 监控动作建议 | `channel.monitor.target.action-propose.read`（v1） | 受限的监控事件和当前启用状态 | 固定的启用/停用建议；管理员确认后才会执行 |
| 渠道模型映射检查 | `channel.model_mapping.review.read`（v1） | 管理员选中的公开模型名和映射值 | 摘要、检查项和建议 |
| 渠道健康事件 | `events.channel-health.changed.consume`（v1） | `eventKey`、状态变化和时间 | 有界的处理结果 |
| 商户服务事件 | `events.merchant-service.changed.consume`（v1） | `eventKey`、服务 ID、版本、状态和时间 | 有界的处理结果 |
| 已保存余额快照分析 | `account.balance.snapshot.review.read`（v1） | 本人最多 20 条余额快照 | 摘要、检查项和建议 |
| 当前余额读取 | `account.balance.snapshot.live-review.read` + `external.account.balance.read`（均为 v1） | 用户本次同意后由宿主返回的当前余额投影 | 摘要、检查项和建议 |
| 固定页面补充内容 | `uiViews` / `uiContributions` | 插件清单中的静态文本和指标 | 宿主组件渲染的卡片、徽标、指标或表格 |

插件不能直接访问用户会话、数据库、钱包、API Key、渠道地址、上游凭据或任意 URL。当前实时外部读取只支持宿主固定的 `account.profile` 和 `account.balance` 回调，要求 `GET` 和空查询参数；用户每次运行都要单独同意。

插件不能创建模型、渠道、价卡、菜单、路由、HTTP API、Webhook、计划任务或自定义页面。涉及新模块时，必须先由宿主增加固定入口、输入输出 Schema、权限和页面接线。

## 2. 最快开始：打包现有示例

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

示例打包器会复制示例和 Python SDK，生成 `.afplugin` 文件。它不会安装服务器依赖，也不会启动插件。

## 3. 制作自己的插件

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

## 4. 安装和运行

管理员在“插件管理”中上传 `.afplugin`：

1. 上传包并安装。
2. 如果页面提示需要授权，进入“高级管理”，授予示例要求的 capability。
3. 保持示例要求的 `publicConfig`。模型目录可使用公开配置，其他示例通常要求 `{}`。
4. 启用部署。
5. 等节点状态显示“就绪”。
6. 打开对应业务页面，选择插件并运行。

手工登记的制品需要先审批。普通自有插件可以不签名；签名只用于显示发布者或官方来源，不会自动增加权限。安装和多节点处理见[插件安装与运维](plugin-sidecars.md)。

## 5. Manifest 清单

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

`uiViews` 只能使用宿主组件：`text`、`badge`、`metric`、`table` 和 `info_card`。固定槽位是 `plugin_management`、`channel_monitor`、`channel_model_mapping`、`merchant_service` 和 `external_account`。不能放 HTML、CSS、脚本、链接或按钮。

## 6. Python 处理函数

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

## 7. 各入口的关键约束

### 模型资料目录

输入包括搜索词、页大小和游标。输出必须包含 `models` 和 `nextCursor`。模型资料只用于展示，不会创建可调用模型、渠道、价格或 Key。宿主会检查模型 ID、字段长度、分页游标和重复项。

### 商户服务检查

插件只收到服务名称、说明、公开协议和模型能力。结果包含 `summary` 和 `checks`。每项检查必须有 `code`、`title`、`status`、`message`、`recommendation`；状态只能是 `pass`、`info`、`warning`、`fail`。

### 渠道监控和模型映射检查

插件处理管理员选择的数据，返回文字建议。不能连接渠道、读取 Key、发起探测或修改配置。动作建议仍需管理员确认，详见[插件动作确认](plugin-actions.md)。

### 持久事件

渠道健康事件使用 `down` / `recovered`；商户服务事件使用 `active` / `paused`。输入包含稳定的 `eventKey`，重试时保持不变。插件输出会进入持久队列结果页，不会自动变成通知或业务写入。管理员结果接口需要对应的管理 PAT 权限。

### 外部账户

快照分析只读取 AnyFlows 已保存的数据。实时读取必须同时满足：用户本次勾选同意、插件拥有分析 capability、部署拥有 `external.account.balance.read@1`。余额读取会保存新的快照，但不会充值、扣款或修改 AnyFlows 钱包。

## 8. 调试和排错

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
