# 插件开发与运行隔离

AnyFlows 通过版本化 JSON Lines 协议调用独立插件进程。插件可以使用 Python、Go、Rust 或其他能够读写标准输入输出的语言；宿主不依赖 Rust 动态库 ABI。

安装包格式、可选签名和安装操作见 [插件安装与 Sidecar 协议](plugin-sidecars.md)。

## 运行完整示例

仓库包含一个无网络、无凭据、无外部写入的模型资料插件：

```text
tools/plugins/
├── sdk/python/anyflows_plugin.py
├── examples/catalog/
│   ├── manifest.json
│   ├── models.json
│   └── plugin.py
├── package-example.py
└── package-plugin.py
```

在开发环境中使用 Python 3.10 或更新版本打包：

```bash
python tools/plugins/package-example.py dist/catalog-example.afplugin
```

该命令将示例和 SDK 复制到临时分发目录，再调用通用打包工具；输出文件必须尚不存在。生成的包没有发布者签名，可以直接上传安装。示例的 Linux 入口使用 `/usr/bin/python3`，部署节点需要提供这个解释器。Windows 部署应将插件及其运行时打包为 Windows 可执行文件，不能直接使用这个 Linux 脚本入口。

安装后，在登录后的模型广场进入“插件模型资料”，选择 Example model catalogue，搜索 `vision` 或翻页。也可以使用登录会话或具备 `models:read` 权限的管理 API Token 调用：

```text
GET /api/account/plugin-models/sources
GET /api/account/plugin-models/org.anyflows.example-catalog?q=vision&limit=20
```

这些接口返回插件提供的模型资料。示例中的模型是开发数据，安装示例不会创建推理渠道、生成价卡或提供模型调用能力。模型调用 API Key 与上述管理接口的认证体系保持独立。

## 使用 Python SDK

SDK 只使用 Python 标准库。分发自己的插件时，将 `anyflows_plugin.py` 放在入口能够导入的位置，传入完整 Manifest 和只读处理函数：

```python
from anyflows_plugin import Plugin, PluginError


def handle(capability_id, public_config, value):
    if capability_id != "catalog.models.read":
        raise PluginError("unsupported")
    return {"models": [], "nextCursor": None}


raise SystemExit(Plugin(manifest, handle).serve())
```

SDK 负责 Manifest 与健康响应、请求身份和版本检查、部署 generation 与请求 ID 回传、deadline 检查、JSON 行帧大小限制及固定错误投影。异常文本、路径和处理函数中的值不会作为错误详情返回给宿主。SDK 同时接受旧版本宿主使用的 snake_case 调用字段，输出协议保持当前公开契约的 camelCase 字段。

SDK 的完整协议帧上限为 1 MiB；Manifest 的 `maxResponseBytes` 单独限制能力输入、输出值的紧凑 UTF-8 JSON 大小，不包含协议元数据，也不限制 Manifest 或健康响应。两类限制同时生效。

处理函数仍应检查自己的业务输入，并按 Manifest 中的 `inputSchema`、`outputSchema` 和 `configSchema` 设计数据。宿主会独立执行 Schema 和能力授权检查。不要通过公开配置传递密码、令牌、Cookie 或其他凭据，也不要向标准输出打印日志；标准输出仅用于协议帧。

## 模型资料能力

模型资料入口只调用已授权的 `catalog.models.read` 或 `catalog.models.search` v1 能力。输入为：

```json
{"query":null,"cursor":null,"limit":20}
```

处理函数返回 `CapabilityResult.output` 的内容：

```json
{
  "models": [
    {
      "id": "example-text-small",
      "displayName": "Example Text Small",
      "provider": "example",
      "contextWindow": 32768,
      "inputModalities": ["text"],
      "outputModalities": ["text"],
      "tags": ["example"]
    }
  ],
  "nextCursor": null
}
```

`limit` 范围为 1–50。每页模型 ID 必须唯一；模型信息只包含公开资料，不接受价格、路由、账号或任意附加字段。宿主提供来源标识，插件不能冒充平台价卡或其他来源。分页游标由插件解释，宿主限制其长度和传输范围。

## 宿主隔离策略

隔离策略由服务配置选择，插件包无法放宽：

```toml
[server]
plugin_isolation = "auto"
```

| 策略 | Linux 行为 | 其他平台行为 |
| --- | --- | --- |
| `auto` | 系统提供可信的 `setpriv` 和 `prlimit` 时启用 no-new-privileges、禁用 core dump，并收紧文件大小和打开文件数；缺少工具时保留进程管理 | Windows 使用 Job Object 管理整个进程树；其他系统保留进程管理 |
| `trusted` | 使用运营方信任的宿主账户权限运行 | Windows 仍保留 Job Object 进程树清理 |
| `restricted` | 要求上述工具，并额外将每个进程的地址空间限制为不超过 4 GiB、累计 CPU 时间不超过 300 秒 | 当前不支持，启动会明确失败 |
| `require_sandbox` | 要求 `setpriv`、`prlimit`、Bubblewrap 和内核命名空间支持，在资源限制外隔离文件系统、网络和进程命名空间 | 当前不支持，启动会明确失败 |

所有资源上限只会收紧已有宿主限制，不会提高服务账户的软、硬限制。`auto` 不设置地址空间限制，以兼容预留较大虚拟地址空间的运行时；显式 `restricted` 和 `require_sandbox` 可能不适合这类运行时。需要超过当前固定资源上限的插件，应由管理员评估后选择合适的执行策略。

Linux 只从固定系统目录查找 root 所有、不可被组或其他用户修改的工具，不使用插件提供的 `PATH`。强隔离仅挂载系统运行库、系统解释器和校验后的插件副本，使用只读根文件系统及独立的 `/tmp`；不挂载宿主用户目录和服务工作目录。网络命名空间独立，插件不能直接访问外部网络。强隔离缺少依赖或内核不支持时会失败，不会回退为普通进程并显示“已隔离”。

Windows 在子进程恢复执行前将其放入 Job Object，停用、异常终止或句柄释放会清理后代进程。Job Object 的进程树管理不等于文件系统或网络沙箱，界面会显示实际采用的隔离级别。

节点插件状态中的 `isolation` 表示实际运行模式：`process`、`resource_limited`、`sandboxed` 或 `job_object`，尚未启动时可能为 `unknown`；概览顶层的 `isolation` 表示本节点配置的期望策略。应结合节点健康、部署 generation 和更新时间判断插件是否已经就绪。

## 契约与验证

跨语言契约位于 `contracts/plugins/v1/`。变更协议字段时，应同步 Rust DTO、Schema 和 SDK，并覆盖真实宿主请求到插件响应的完整往返。当前回归包含 Rust 序列化调用经 Python SDK 处理后再反序列化，以及 SDK 错误帧、超时、分页和 Windows 后代进程清理检查。

此项目的构建和测试通过远端流水线执行。插件开发应保留当前协议版本和 Manifest 身份检查，不应通过兼容处理跳过包摘要、能力授权或部署 generation 校验。
