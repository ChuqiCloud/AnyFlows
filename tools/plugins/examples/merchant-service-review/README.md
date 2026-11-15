# 商户服务资料检查：新手示例

这个例子会检查商户填写的服务说明和模型能力，并在商户工作台显示摘要与建议。它是一个只读插件：不访问外网，不连接数据库，不接收 API Key、上游地址、上游模型映射、价格或财务资料，也不能替商户修改服务或替管理员批准服务。

你不需要理解 AnyFlows 的 Rust 代码。开发时只需认识三个文件：`manifest.json` 描述输入输出格式和运行限制；`plugin.py` 写检查规则；Python SDK 负责与 AnyFlows 通信。宿主的业务页面、权限和插件运行过程由 AnyFlows 管理。

## 1. 准备环境

准备一个 AnyFlows 源码副本，并安装 Python 3.10 或更新版本。运行插件的每个 AnyFlows 节点也需要能执行清单里指定的 Python 入口。

示例默认使用 Linux 入口 `/usr/bin/python3`。插件包不会替服务器安装 Python。若节点使用 Windows 或其他路径，需要按项目的插件打包说明调整可执行文件和 `manifest.json` 中的入口；同一个部署包目前不会按操作系统自动选择不同入口。

在源码仓库根目录打开终端，先看一遍这些文件：

```text
tools/plugins/examples/merchant-service-review/
├── manifest.json   # 宿主和插件共同遵守的数据约定
├── plugin.py       # 本例的检查规则
└── README.md       # 本说明
```

打包工具会把 SDK 自动复制到临时分发目录，因此无需自己下载依赖或改 Python 导入路径。

## 2. 先打包并安装示例

在仓库根目录运行：

```bash
python tools/plugins/package-example.py dist/merchant-service-review.afplugin --example merchant-service-review
```

输出位置可以换成其他新文件名，但目标文件必须还不存在。命令会从示例目录读取文件、复制 Python SDK，并生成 `.afplugin` 安装包；不需要签名。

接着用管理员账号打开控制台的“插件管理”：

1. 上传 `dist/merchant-service-review.afplugin`。
2. 如果安装结果提示能力需要授权，打开该插件的“高级管理”，授予 `merchant.service.review.read` v1 并部署。
3. 等插件节点状态显示就绪。
4. 用商户账号进入“我的服务”，在服务行中点“插件检查”，选择 `Example merchant service review`，再点“运行检查”。

此能力的部署 `publicConfig` 必须是空对象 `{}`。插件使用本次调用传入的商户服务资料；不需要在管理页面填写额外配置。只有已启用、已批准且明确授予上述能力的插件才会列在商户可选清单中。

若用系统 API Token 调用：获取可选插件清单使用 `merchant_plugins:read`；执行检查使用 `merchant_plugins:execute`。模型调用 API Key 不具备这两项系统 API 权限。

## 3. 宿主会传入什么

前端提交的请求体只有被选中的插件 ID。商户身份、服务归属和服务版本都由宿主验证；插件既看不到身份 ID，也不能通过请求选择另一个能力或版本。

`plugin.py` 的 `review_service()` 收到如下 `value` 对象：

```json
{
  "service": {
    "name": "Example relay",
    "description": "A service description visible in the merchant catalog.",
    "protocol": "openai_chat",
    "models": [
      {
        "model": "example-text",
        "context_window": 32768,
        "max_output_tokens": 4096,
        "supports_streaming": true,
        "supports_tools": false,
        "supports_vision": false
      }
    ]
  }
}
```

| 字段 | 含义 | 约束 |
| --- | --- | --- |
| `service.name` | 商户填写的服务名称 | 字符串，必填 |
| `service.description` | 服务说明 | 字符串，必填；可以为空 |
| `service.protocol` | 服务使用的公开协议类型 | 字符串，必填 |
| `service.models` | 服务公开提供的模型能力列表 | 数组，最多 128 项 |
| `models[].model` | 展示给买家的公开模型 ID | 非空字符串 |
| `context_window` | 模型上下文窗口 | 正整数或 `null` |
| `max_output_tokens` | 最大输出 token 数 | 正整数或 `null` |
| `supports_streaming` | 是否声明支持流式输出 | 布尔值 |
| `supports_tools` | 是否声明支持工具调用 | 布尔值 |
| `supports_vision` | 是否声明支持图像能力 | 布尔值 |

这些是插件输入字段的完整集合；`manifest.json` 设置 `additionalProperties: false`，未知字段会被拒绝。宿主不会传入用户或商户 ID、上游地址、上游模型映射、API Key、价格、倍率或财务数据。不要为了让插件做检查而把这些资料写入公开服务说明或其他插件配置。

## 4. 看懂检查结果

插件返回的业务 JSON 结构如下：

```json
{
  "summary": "Reviewed Example relay (openai_chat) and found 4 documentation checks.",
  "checks": [
    {
      "code": "models.present",
      "title": "Models are listed",
      "status": "pass",
      "message": "The service lists 1 model(s).",
      "recommendation": null
    }
  ]
}
```

每个检查项都必须有 `code`、`title`、`status`、`message` 和 `recommendation`。没有建议时也要传 `null`，不能省略该字段。

| 字段 | 作用 | 限制 |
| --- | --- | --- |
| `summary` | 对本次检查的简短概述 | 非空字符串，最多 2048 字节 |
| `checks` | 单项检查结果 | 最多 64 项 |
| `code` | 检查项的稳定机器标识 | 非空，最多 64 字节；同一响应不能重复 |
| `title` | 检查项标题 | 非空，最多 128 字节 |
| `status` | 展示状态 | 只能是 `pass`、`info`、`warning`、`fail` |
| `message` | 检查发现 | 非空，最多 2048 字节 |
| `recommendation` | 给商户的改进建议 | 非空字符串或 `null`，最多 2048 字节 |

所有文字都不能有首尾空白或控制字符，整个结果最多 16 KiB。界面会把插件文字作为纯文本显示，不会当作 HTML 执行。`pass`、`info`、`warning`、`fail` 只用于呈现结果；这些建议不会直接改变服务审核状态。

## 5. 修改检查规则

打开 `plugin.py`，在 `review_service()` 中找到 `# 规则一` 到 `# 规则四`。每条规则用 Python 的 `if` 判断服务资料，并通过 `checks.append({...})` 增加一项结果。

例如，如果想检查是否声明流式支持，可以在模型检查附近增加：

```python
streaming_models = [model for model in models if model["supports_streaming"]]
if not streaming_models:
    checks.append(
        {
            "code": "models.streaming",
            "title": "Streaming is not listed",
            "status": "info",
            "message": "No model is marked as supporting streaming.",
            "recommendation": "If streaming is available, mark the supported models.",
        }
    )
```

添加前先确认输出仍符合 Manifest：状态只能使用四个规定值；五个字段都必须存在；数组不能超过 64 项；文字不能超过各自长度上限。每个结果都使用新的稳定 `code`，以后修改提示语时尽量保留原来的 code，方便日志、界面和自动化识别同一项检查。

改成自己的插件时，通常按这个顺序：

1. 复制整个 `merchant-service-review` 示例目录到自己的工作目录。
2. 把 `manifest.json` 中的 `pluginId` 改成自己的唯一名称，例如 `com.example.merchant-review`。不要和示例或已安装插件使用相同 ID。
3. 改 `displayName` 作为管理员和商户看到的插件名称；每次发布新包时更新 `pluginVersion`。
4. 根据检查规则调整 `plugin.py`，保持输入输出字段不变；若要更改字段或限制，则要让 Manifest、插件代码和 AnyFlows 宿主契约同步兼容。
5. 用第 2 节的打包命令生成新包，上传后在高级管理中明确授予 `merchant.service.review.read`。

运行规则不要放在模块顶层的协议循环里。保持 `review_service()` 为一个接收资料并返回字典的普通函数，SDK 会负责和宿主的通信。

## 6. Python 文件分工

`plugin.py` 开头导入 Python 标准库里的 `json`、`pathlib` 和 `sys`，再导入 SDK 的 `Plugin` 与 `PluginError`。无需导入 AnyFlows Python/Rust 私有包。

`review_service(capability_id, public_config, value)` 是业务代码。`PluginError("unsupported")` 表示能力不匹配；`PluginError("invalid_configuration")` 表示输入不符合约定；`PluginError("invalid_response")` 表示插件准备的结果无效或超出上限。这些是固定错误码，不要把异常堆栈、文件路径或密钥内容塞进错误消息。

文件底部的 `main()` 读取同目录的 `manifest.json`，然后调用 `Plugin(manifest, review_service).serve()`。SDK 会处理启动握手、健康检查、JSON Lines 请求、超时和响应校验。协议数据写在标准输出，不能用 `print()` 打调试日志；如需调试输出，只写标准错误，并且不要输出服务隐私资料。

## 7. 把自定义插件接入其他 AnyFlows 模块

上传 `.afplugin` 不会自动创建页面、菜单、HTTP 接口或后台任务。新增业务入口需要 AnyFlows 开发者在宿主增加固定 capability 和调用代码，确定调用者与数据边界，校验插件输出，再按需要增加界面和 API 文档。完成宿主接入后，插件作者才可以使用对应契约开发插件。

不用了解 AnyFlows 内部代码也可以按已有示例开发“商户服务资料检查”或“公开模型资料目录”。若要新增别的模块，先请 AnyFlows 维护者提供该模块的输入字段、输出 Schema、授权方式和安装/测试入口；不要猜 capability 名称或复制一个看似相近的接口。具体宿主接入方式参见 [`docs/plugin-development.md`](../../../../docs/plugin-development.md) 的“针对其他模块开发”。

目前通用插件只开放 `read` + `none` 的无副作用能力。任何影响模型定价、渠道凭据、钱包、余额、结算、登录身份、数据库或核心权限的操作都由宿主控制，不可以通过插件输出建议或自行调用外网来替代宿主授权。

## 8. 常见问题

| 现象 | 检查办法 |
| --- | --- |
| 打包时报 Manifest 或入口错误 | 确认 `manifest.json` 存在、`plugin.py` 是目录内入口，并且输出文件还不存在 |
| 安装后商户列表里没有插件 | 插件必须启用、通过审核、部署中包含 `merchant.service.review.read` v1，且节点运行正常 |
| 返回 `invalid_configuration` | 对照第 3 节字段名和类型；特别检查空 `publicConfig`、模型字段和额外字段 |
| 返回 `invalid_response` 或运行时不可用 | 检查是否返回规定的五个字段、合法状态、最多 64 项和 16 KiB 以内的结果 |
| 插件启动失败 | 确认每个节点都有清单中指定的 Python 可执行文件，并查看“节点运行状态”的错误信息 |
| 日志里看不到 Python 的输出 | 协议只使用标准输出；调试文字不要写到 stdout，可写到 stderr |
| 页面显示检查失败或版本冲突 | 刷新服务草稿后重试；服务内容在检查过程中发生变化时，宿主会丢弃旧版本结果 |

如果更改了 `manifest.json`、`plugin.py` 或公开资源，要打一个新包并提升 `pluginVersion`。安装包发布内容不会自动同步到运行节点之外的 Python 包管理器。
