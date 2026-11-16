# 插件包与协议参考

安装操作见[插件安装与运维](plugin-sidecars.md)，开发步骤和模块接入见[插件开发](plugin-development.md)。本页供手工制作安装包、实现其他语言 SDK 或排查协议问题时查阅。

## 包格式

`.afplugin` 使用 ZIP 格式，目录结构如下：

```text
example.afplugin
├── anyflows-plugin.json
└── payload/
    ├── bin/example-plugin
    └── assets/catalog.json
```

`anyflows-plugin.json` 包含清单和入口路径。`executablePath` 相对 `payload/`，不含 `payload/` 前缀：

```json
{
  "manifest": {
    "pluginId": "org.example.catalog",
    "displayName": "Example catalog",
    "pluginVersion": "1.0.0",
    "contractRange": {
      "minimum": { "major": 1, "minor": 0 },
      "maximum": { "major": 1, "minor": 0 }
    },
    "capabilities": [
      {
        "id": "catalog.models.read",
        "version": 1,
        "access": "read",
        "sideEffect": "none",
        "inputSchema": { "type": "object" },
        "outputSchema": { "type": "object" }
      }
    ],
    "authenticationModes": ["none"],
    "limits": {
      "maxConcurrency": 1,
      "requestTimeoutMs": 1000,
      "maxResponseBytes": 1048576,
      "maxPageSize": 100
    },
    "configSchema": { "type": "object", "additionalProperties": false }
  },
  "executablePath": "bin/example-plugin"
}
```

清单中的 Schema 仅作结构示例；实际目录数据还需符合宿主的[模型目录格式](plugin-development.md)。包内不要放入密钥、访问令牌、`.env` 或用户数据。

### Manifest 字段

| 字段 | 填写方法 |
| --- | --- |
| `pluginId` | 插件身份，最多 128 字节，至少两段，例如 `com.example.catalog`；各段只用小写字母、数字和连字符，以字母或数字开头、结尾。升级时保持不变 |
| `displayName` | 显示名称，最多 128 字节；省略时目录使用插件 ID |
| `pluginVersion` | 语义化版本，例如 `1.0.0`；发布新内容时更新，启动响应须与安装清单一致 |
| `contractRange` | 可接受的契约版本范围，当前宿主要求范围包含 `1.0` |
| `capabilities` | 声明能力名、版本、读写及副作用类型；通用进程能力要提供 `inputSchema`、`outputSchema`，并采用 `read`、`none` |
| `authenticationModes` | 认证方式声明；当前无凭据的进程插件填写 `["none"]`。填写其他名称不会取得用户会话、密钥或外部认证入口 |
| `configSchema` | 管理员公开配置的 JSON Schema；配置会保存并传给插件，不用于秘密数据 |
| `limits` | 插件能接受的上限；宿主和具体模块可以进一步收紧，不能用它提高宿主限制 |

`limits` 四项均为必填正整数：

| 字段 | 允许值与实际效果 |
| --- | --- |
| `maxConcurrency` | 1–1024；当前宿主对每个插件进程串行交换请求，调大不会创建并发进程 |
| `requestTimeoutMs` | 1–300000 毫秒；执行时间还受宿主超时及请求 deadline 限制，排队时间也计入预算 |
| `maxResponseBytes` | 1 字节至 16 MiB；分别限制能力输入、输出 JSON 值，完整协议帧还有独立上限 |
| `maxPageSize` | 1–10000；模型目录接口只接受 1–50，并进一步取它与此值的较小值 |

| 限制 | 上限 |
| --- | --- |
| 压缩包 | 128 MiB |
| 展开文件与元数据合计 | 512 MiB |
| ZIP 条目 | 100,000 |
| `anyflows-plugin.json` | 256 KiB |
| 路径 / 单个路径段 | 900 / 255 个 UTF-8 字节 |
| 路径深度 | 32 段 |

路径使用 POSIX 相对路径。解包器拒绝链接、特殊文件、重复或大小写冲突路径、越界路径及不支持的 ZIP 特性。打包工具会将入口标记为可执行；宿主不会自动安装解释器或转换目标架构。

## 可选签名

未签名包省略 `signature`。已有签名时，在 `anyflows-plugin.json` 根对象中增加：

```json
{
  "signature": {
    "signerKeyId": "publisher-example",
    "signatureBase64": "<Base64 编码的 64 字节 Ed25519 签名>"
  }
}
```

用仓库工具附加现成签名：

```bash
python tools/plugins/package-plugin.py build/example-plugin manifest.json bin/example-plugin dist/example-signed.afplugin --signer-key-id publisher-example --signature-file signature.base64
```

四个位置参数依次为分发目录、Manifest 文件、目录内的入口路径和输出文件。Manifest 文件只保存上例的 `manifest` 对象。输出需位于分发目录外，且文件尚不存在。工具会打包目录中的普通文件，固定时间戳，不保留所有者、扩展属性或 setuid 权限；不会生成密钥或签名。

### V2 包摘要

对下列字节序列计算 SHA-256：

1. 前缀 `ANYFLOWS_PLUGIN_PACKAGE_V2\0`，末尾为一个 NUL 字节。
2. `payload/` 下所有条目按相对路径排序，逐条写入类型字节：目录为 `0`，普通文件为 `1`。
3. 写入路径的 UTF-8 字节长度（little-endian u64）和路径字节。路径不含 `payload/` 前缀。
4. 文件条目再写入内容长度（little-endian u64）和内容。

空目录计入摘要；`payload/` 根目录自身和 `anyflows-plugin.json` 不计入该树摘要。清单和入口通过签名的其他字段绑定。

### V2 签名消息

Ed25519 按下列顺序签署字节：

```text
ANYFLOWS_PLUGIN_ARTIFACT_V2\0
plugin ID + LF
语义化版本 + LF
64 位小写十六进制包摘要 + LF
相对 payload/ 的入口路径 + LF
Manifest 的紧凑 UTF-8 JSON
```

第一行的 `\0` 表示 NUL，后面不额外加 LF。其余 `LF` 为单个换行字节。Manifest 使用宿主契约类型序列化后的值：对象键递归排序，数组保持顺序，不加空格，字符串转义与 `serde_json` 一致。签名者需按此形式处理默认值和省略字段，不能直接签署任意格式的原始 JSON。V1 签名不再接受。

## 进程协议

宿主写标准输入，插件写标准输出，每行一条 UTF-8 JSON。标准输出只用于协议，不要写启动提示或日志。协议类型定义在 [value.rs](../crates/af-plugin-contract/src/value.rs)，Schema 位于 [`contracts/plugins/v1/`](../contracts/plugins/v1/)。

请求公共字段为 `protocolVersion`、`pluginId`、`pluginVersion`、`artifactSha256`、`deploymentGeneration`、`requestId`、`deadlineUnixMs` 和 `operation`。响应原样回传前六项，用 `result` 返回结果。

| `operation.operation` | 内容 | 响应 `result.kind` |
| --- | --- | --- |
| `manifest` | 无额外字段 | `manifest`，值为完整 Manifest |
| `health` | 无额外字段 | `health`，值含 `status` 和 `checkedAtUnixMs` |
| `invoke_capability` | `connectionId`、`publicConfig`、`capabilityId`、`capabilityVersion`、`input` | `capability`，值为 `{"output": ...}` |

业务错误返回 `result.kind = "error"`，错误码与结果状态见 [error.rs](../crates/af-plugin-contract/src/error.rs)。当前协议版本为 `1`。Python SDK 同时兼容旧宿主的调用字段 snake_case 别名，新实现应使用上述 camelCase 字段。

启动时先交换 Manifest，再执行健康检查；插件身份、版本、摘要、清单必须一致，且健康状态须为 `healthy`。每个进程串行处理请求，排队也计入 deadline。宿主不会自动重放失败调用。

完整帧默认上限为 **1 MiB**。`maxResponseBytes` 另外限制能力输入、输出值的紧凑 JSON 大小，不限制 Manifest 或健康响应；最终还受宿主调用方的上限约束。执行时间取宿主、清单与请求 deadline 的最小值。

现有模型目录入口每个宿主服务最多同时处理 8 个查询；每用户允许连续 8 次请求，之后每 500 毫秒补充一次额度，额度不足时等待。等待和插件调用合计受 5 秒预算限制。这些是目录模块限制，自定义模块应自行设置调用预算和限额。

通用能力调用要求 `access = "read"`、`sideEffect = "none"`、输入输出 Schema 均存在，且能力已获部署授权。身份、凭据、数据库和钱包权限不会随协议传入。Manifest 类型还保留了 `write` 与外部副作用恢复语义，用于未来宿主专用的类型化工作流；这不表示当前 Sidecar 可调用写能力。外部连接器、受控写操作、事件任务和声明式 UI 的目标边界见[插件能力扩展设计](plugin-capability-expansion.md)。

握手、健康响应、身份或输出校验失败，以及超时、取消或协议异常时，宿主终止进程；普通能力业务错误不会自动退出进程。仍启用的部署由后续协调恢复。

## 执行目录与隔离

插件缓存是受控副本，不能用作持久数据库。启动时宿主复制到 `.runtime` 下的私有随机目录，再次计算摘要后执行。Unix 入口权限为 `0500`，其他文件为 `0400`，不继承 setuid 或组/其他用户写权限。工作目录固定为副本，环境变量被清空，仅保留 Windows 启动所需的系统变量。

Unix 使用独立进程组清理插件。Windows 在子进程继续执行前加入 Job Object，停用、超时、异常和句柄释放时清理后代进程。

### 隔离参数

Linux 只在固定系统目录查找 root 所有、不可被组或其他用户修改的工具，不搜索插件提供的 `PATH`。

| 策略 | Linux 的额外限制 |
| --- | --- |
| `trusted` | 不附加资源限制 |
| `auto` | 有 `setpriv`、`prlimit` 时启用 no-new-privileges，core dump 为 0，打开文件数不超过 1024，单文件不超过 64 MiB；缺少工具时采用普通进程 |
| `restricted` | 必须具备上述工具，另限制每进程地址空间不超过 4 GiB、累计 CPU 时间不超过 300 秒 |
| `require_sandbox` | 在 `restricted` 基础上使用 Bubblewrap：只读根文件系统、独立临时目录、网络和进程命名空间，只挂载系统运行文件和已校验插件副本 |

资源限制只会收紧已有软、硬限制。`auto` 不设置地址空间上限，以适配会预留大量虚拟地址的运行时。`restricted`、`require_sandbox` 在非 Linux 平台启动失败。`require_sandbox` 缺少工具或内核能力时也会失败，不会降级。

实际等级为 `process`、`resource_limited`、`sandboxed`、`job_object`，未启动时可为 `unknown`。Job Object 不限制文件系统或网络访问。

## 归档与节点状态

上传归档按 `artifact_id` 关联制品记录，以 **48 KiB** 分块保存。归档摘要、总长度、分块数和内容在事务内提交；读取时检查顺序、长度及 SHA-256。同一制品重传不会覆盖已保存归档。

共享部署每次变更推进 `generation`。卸载将停用代次、清理意图和审计写入同一事务；节点处理时检查代次，旧清理请求不能覆盖新部署。永久清理保留墓碑，阻止同制品重传后恢复归档。

节点租约与插件观察租约均为 **120 秒**。计算就绪数量时，必须同时满足节点在线、观察未过期、代次和摘要匹配、状态为 `ready`。心跳或部署 `enabled` 单独存在都不够。

## 管理接口

以下路径均以 `/api/admin/plugins` 开头，需要管理员会话或获准的管理 PAT：

| 方法与路径 | 用途 / 主要参数 |
| --- | --- |
| `POST /install` | multipart：`package` 必填，`config` 默认 `{}`，`expected_generation` 可选 |
| `GET /artifacts` | 制品列表，`before_id`、`limit` |
| `POST /artifacts` | 登记制品根目录下的现有包；不会生成共享 ZIP 归档 |
| `POST /artifacts/{artifact_id}/approve` | 批准制品 |
| `POST /artifacts/{artifact_id}/revoke` | 撤销制品，需原因 |
| `GET /deployments` | 当前部署，`cursor`、`limit` |
| `POST /{plugin_id}/deployment` | 部署制品，填写摘要、预期代次、启用状态、能力与配置 |
| `GET /{plugin_id}/deployment/history` | 部署历史 |
| `POST /{plugin_id}/disable` | 停用，JSON 含 `expected_generation` |
| `POST /{plugin_id}/rollback` | 恢复历史部署，JSON 含 `expected_generation`、`target_generation` |
| `GET /runtime` | 节点和插件运行状态 |
| `POST /{plugin_id}/uninstall` | 卸载，JSON 含 `expected_generation`；响应 `pendingNodes` 为尚未确认的在线节点数 |
| `POST /artifacts/{artifact_id}/cleanup` | 清理已撤销且无启用引用的制品 |

`POST /install` 返回 `422 / plugin_lifecycle_capability_review_required` 时，制品和归档已保存，需要显式授权后部署。`409 / plugin_lifecycle_conflict` 表示身份、代次或当前状态冲突，应刷新后处理。完整参数与响应以 [OpenAPI](../web-next/openapi/openapi.json) 为准。
