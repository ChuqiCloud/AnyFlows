# 插件包与协议参考

本文用于制作安装包、编写其他语言的 SDK 和排查协议问题。安装步骤见[插件安装与运维](plugin-sidecars.md)，开发步骤见[插件开发](plugin-development.md)。表格中的字段名、接口路径、能力 ID 和错误码必须保持原样。

## 1. 安装包格式

`.afplugin` 本质上是 ZIP 文件，必须包含清单文件和 `payload/` 目录：

```text
example.afplugin
├── anyflows-plugin.json
└── payload/
    ├── bin/example-plugin
    └── assets/catalog.json
```

`anyflows-plugin.json` 包含 `manifest` 和入口路径。`executablePath` 是相对于 `payload/` 的路径，不能写 `payload/` 前缀：

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

包内禁止放密码、访问令牌、Cookie、`.env` 文件和用户数据。清单中的 Schema 只说明结构；模型目录等业务入口还要符合宿主的固定数据格式。

### 清单字段

| 字段 | 规则 |
| --- | --- |
| `pluginId` | 唯一 ID，最多 128 字节，至少两段。升级时保持不变。 |
| `displayName` | 控制台显示名称，最多 128 字节。 |
| `pluginVersion` | 语义化版本。代码、清单或资源变化后递增。 |
| `contractRange` | 支持的协议版本范围。当前必须包含 `1.0`。 |
| `capabilities` | 能力 ID、版本、访问类型、副作用和输入输出 Schema。普通插件使用 `read` + `none`。 |
| `authenticationModes` | 无凭据插件填写 `["none"]`。这不会授予会话、Key 或外部认证。 |
| `configSchema` | 管理员公开配置的 JSON Schema。不要用来保存秘密。 |
| `limits` | 插件自身的并发、超时、输出和分页上限。宿主可以进一步收紧。 |
| `uiContributions` | 插件管理页静态信息卡，最多 16 张。 |
| `uiViews` | 固定业务页面的静态视图，最多 8 项。 |
| `uiPages` | 管理员扩展页，最多 32 项；页面数据绑定只读能力，`actions[]` 可绑定经确认、具备幂等语义的 `external_write` 能力。 |
| `routes[].audience=model_key` | 使用模型调用 Key 的宿主路由，地址为 `/api/plugins/{plugin_id}/routes/{route_id}` 或 `/v1/plugins/{plugin_id}/{route_id}`。 |

视图使用宿主组件渲染，支持 `text`、`badge`、`metric`、`table`、`list`、`notice`、`progress` 和 `info_card`。`list` 最多 16 项，`progress.percent` 必须在 0 到 100 之间；插件不能注入 HTML、CSS、脚本、链接或按钮。复杂内容应组合多个视图，保持数据和展示分离。

`limits` 的实际范围：

事件订阅使用清单中的 `eventSubscriptions`。`eventKind`、`capabilityId` 和版本必须来自宿主事件目录；`filters` 只允许目录声明的字段和值。当前事件目录包括 `channel.health.changed`、`merchant.service.changed`、`model.catalog.changed`、`channel.config.changed`、`channel.credential.changed`、`model.pricing.changed`、`external.account.balance.changed`、`plugin.deployment.changed` 和 `plugin.task.changed`。宿主在写入持久队列前应用过滤条件，插件只会收到匹配的投影事件。每个事件带稳定 `eventKey`，插件应按它幂等处理；投影不包含凭据、钱包金额、会话或上游任务原文。

| 字段 | 范围 |
| --- | --- |
| `maxConcurrency` | 1–1024。当前单个插件进程仍按顺序处理请求。 |
| `requestTimeoutMs` | 1–300000 毫秒，排队时间也计入。 |
| `maxResponseBytes` | 1 字节–16 MiB，限制能力输入和输出 JSON。 |
| `maxPageSize` | 1–10000；模型目录实际最多 50。 |

包本身还受以下限制：压缩包最多 128 MiB，展开后最多 512 MiB，最多 100,000 个 ZIP 条目；清单最多 256 KiB；单个路径最多 900 字节，单个路径段最多 255 字节，路径最多 32 段。路径必须是 POSIX 相对路径。宿主会拒绝链接、特殊文件、重复路径、大小写冲突、越界路径和不支持的 ZIP 特性。

## 2. 可选签名

签名不是安装必需项。未签名包省略 `signature`。签名包在 `anyflows-plugin.json` 根对象增加：

```json
{
  "signature": {
    "signerKeyId": "publisher-example",
    "signatureBase64": "<Base64 编码的 64 字节 Ed25519 签名>"
  }
}
```

打包工具可以附加已有签名：

```bash
python tools/plugins/package-plugin.py build/example-plugin manifest.json bin/example-plugin dist/example-signed.afplugin --signer-key-id publisher-example --signature-file signature.base64
```

自己的插件可以不签名。签名只用于识别发布者和官方来源，不会增加能力，也不会改变隔离策略。配置了受信公钥后，签名者 ID 命中但签名无效时，安装会失败；未配置的签名者显示为未验证来源。

## 3. JSON Lines 进程协议

宿主向插件写标准输入，插件向标准输出写数据，每行一条 UTF-8 JSON。标准输出只能写协议，日志请写标准错误。

请求公共字段：`protocolVersion`、`pluginId`、`pluginVersion`、`artifactSha256`、`deploymentGeneration`、`requestId`、`deadlineUnixMs` 和 `operation`。响应原样返回前六项，并在 `result` 中返回结果。

| 操作 | 参数 | `result.kind` |
| --- | --- | --- |
| `manifest` | 无 | `manifest`，完整清单 |
| `health` | 无 | `health`，包含状态和检查时间 |
| `invoke_capability` | `connectionId`、`publicConfig`、`capabilityId`、`capabilityVersion`、`input` | `capability`，包含 `output` |

业务错误使用 `result.kind = "error"`，错误码见协议源码中的 `error.rs`。当前协议版本为 `1`。Python SDK 兼容旧宿主的 snake_case 调用字段；新插件应使用上面的 camelCase 字段。

启动顺序固定为：交换清单、执行健康检查、等待调用。插件 ID、版本、摘要、清单和健康状态必须一致。每个进程按顺序处理请求，排队时间也计入 deadline。宿主不会自动重放失败调用。

完整协议帧默认最多 1 MiB。`maxResponseBytes` 另行限制能力输入和输出 JSON；清单、健康响应和调用方的限制仍分别生效。

## 4. 宿主固定回调

只有在处理 `invoke_capability` 时，插件才可以在返回最终响应前发送 `PluginProxyRequest`。宿主会检查请求关联的插件、制品摘要、部署代次、调用 ID 和 capability grant，然后返回 `PluginProxyResponse`。这不是插件的 HTTP 客户端，也不是通用网络代理。

当前固定端点：

| endpoint | 方法 | 查询参数 | 返回内容 |
| --- | --- | --- | --- |
| `account.profile` | `GET` | 必须为空 | 允许显示的账户名称 |
| `account.balance` | `GET` | 必须为空 | 金额、单位、状态和观察时间 |

插件不能传入 URL、路径、请求头、请求体、凭据或账户 ID。宿主按当前登录用户重新检查账户归属，使用内部 NewAPI/Sub2API 连接器读取数据。余额读取会保存新的快照，但不会修改 AnyFlows 钱包或账本。

实时余额读取需要同时满足：用户本次明确同意、部署拥有 `external.account.balance.read@1`、插件拥有对应分析 capability。一次调用最多四次回调，所有回调共用原请求 deadline。宿主在每次回调前后复核部署代次、审批/撤销状态和能力授权；发现变化时丢弃投影。身份、关联 ID 或协议帧校验失败会终止插件进程。

## 5. 运行目录和隔离

插件运行的是受控副本，不是持久数据库。宿主启动时复制到随机的 `.runtime` 目录并重新计算摘要。Unix 入口权限为 `0500`，其他文件为 `0400`；工作目录固定为副本，环境变量默认清空。

Unix 使用独立进程组清理插件。Windows 使用 Job Object 管理进程树，停用、超时、异常和句柄释放时清理后代进程。

服务配置：

```toml
[server]
plugin_isolation = "auto"
```

| 策略 | 行为 |
| --- | --- |
| `trusted` | 按服务账号运行，不附加资源限制；只适合明确受信任的代码。 |
| `auto` | Linux 在工具可用时启用 no-new-privileges、关闭 core dump、限制文件句柄和单文件大小；缺少工具时使用普通进程。Windows 使用 Job Object。 |
| `restricted` | Linux 必须具备资源限制工具；地址空间最多 4 GiB，累计 CPU 时间最多 300 秒。 |
| `require_sandbox` | 在 `restricted` 基础上使用 Bubblewrap、只读根文件系统、独立临时目录、网络和进程命名空间。 |

`restricted` 和 `require_sandbox` 在不支持的平台或缺少系统能力时启动失败，不会自动降级。`require_sandbox` 下插件不能直接访问外网。Windows Job Object 负责清理进程树，不提供文件系统或网络沙箱。

## 6. 归档和节点状态

上传包按制品 ID 保存为不可变归档。归档摘要、总长度、分块数和内容在事务内校验。共享部署每次变更都会增加 `generation`。

节点租约和插件观察租约都是 120 秒。只有同时满足节点在线、观察未过期、代次和摘要匹配、状态为 `ready`，节点才会计入就绪数量。`enabled` 标志单独存在不代表插件已经启动。

## 7. 管理接口

以下接口都以 `/api/admin/plugins` 开头，需要管理员会话或获授权的管理员 PAT：

| 方法和路径 | 用途 |
| --- | --- |
| `POST /install` | 上传包并安装；multipart 字段为 `package`、可选 `config` 和 `expected_generation`。 |
| `GET /artifacts` | 查看制品列表。 |
| `POST /artifacts` | 登记已有本地制品；不会生成共享 ZIP 归档。 |
| `POST /artifacts/{artifact_id}/approve` | 审批制品。 |
| `POST /artifacts/{artifact_id}/revoke` | 撤销制品并填写原因。 |
| `GET /deployments` | 查看当前部署。 |
| `POST /{plugin_id}/capabilities/{capability_id}/invoke` | 通过宿主通用入口调用已授权、带 Schema 的只读能力。写能力只能从页面动作的提案确认流程进入。 |
| `GET|POST|PUT /{plugin_id}/routes/{route_id}` | 调用清单中声明的管理员路由；宿主按路由方法、能力授权、输入输出 Schema 和部署代次校验。 |
| `POST /{plugin_id}/deployment` | 部署指定制品、配置和能力授权。 |
| `GET /{plugin_id}/deployment/history` | 查看部署历史。 |
| `POST /{plugin_id}/disable` | 停用部署。 |
| `POST /{plugin_id}/rollback` | 回滚到历史部署。 |
| `GET /tasks/unknown` | 分页查看提交或状态结果仍待核对的异步任务。 |
| `POST /tasks/{task_id}/reconcile` | 核对上游后填写实际用量结算；确认未执行时用 `actualUnits: 0` 释放预留额度，并填写核对原因。 |
| `GET /runtime` | 查看节点和插件运行状态。 |
| `POST /{plugin_id}/uninstall` | 卸载插件并清理节点缓存。 |
| `POST /artifacts/{artifact_id}/cleanup` | 清理已撤销且没有启用引用的制品。 |

常见响应：`422 / plugin_lifecycle_capability_review_required` 表示包已保存但还要授权能力；`409 / plugin_lifecycle_conflict` 表示代次或当前状态发生冲突，应先刷新状态。完整参数以 [OpenAPI 文档](../web-next/openapi/openapi.json) 为准。
## 数据面扩展契约

`invoke_task`、`provider.request.transform`、Provider 生命周期操作和 `request.processing.read` 已纳入 v1 契约。任务插件的状态、进度、失败码和幂等键由宿主校验；终态 `result` 可返回大小受限的结果文件和报告，文件由宿主保存并生成下载引用，插件不能指定宿主路径。任务取消可能因恢复重试而重复发送，插件应将同一 `taskId` 的重复取消视为幂等操作。状态查询、结果读取和取消连续失败八次后，任务会进入管理员核对队列。Provider 插件不能选择上游主机、读取认证、修改模型计费字段或返回钱包事实。生命周期结果覆盖模型发现、受限请求构造、响应投影、失败分类和重试建议，宿主保留最终发送、路由、重试和结算权。在实时模型流量中，Chat 的 `request` 阶段已接入同步处理，`ingress` 继续提供脱敏观察；插件可在安全路径上修改模型或返回拒绝，超时和运行时错误按 fail-open 继续模型请求。Provider 请求转换已挂到 Relay 候选发送前，响应和失败分类在候选完成后回调。插件运行时仍不能直接访问数据库、钱包、用户会话或持久化任务状态。
