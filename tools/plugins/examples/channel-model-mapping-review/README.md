# 渠道模型映射检查：新手示例

这个示例读取 AnyFlows 宿主提供的公开模型名和映射关系，返回静态检查摘要。它不会连接上游、读取 API Key 或地址，也不会写入渠道配置。检查结果只是提示，是否修改映射仍由管理员决定。

不需要了解 AnyFlows 的 Rust 代码。这个目录中，`manifest.json` 描述宿主会传入和接收的数据格式，`plugin.py` 实现检查规则，Python SDK 负责和宿主交换协议数据。

## 1. 准备环境

- 安装 Python 3.10 或更新版本，用于本机打包。
- 准备 AnyFlows 源码目录，并从仓库根目录运行下面的命令。
- 实际运行插件的每个 AnyFlows 节点都要有清单中指定的 Python 入口。本例使用 Linux 的 `/usr/bin/python3`。

插件包不会替服务器安装 Python。当前一个部署包不会按操作系统自动切换入口；运行节点的操作系统、架构和依赖必须与包兼容。

## 2. 打包原始示例

在仓库根目录执行。如果还没有 `dist` 文件夹，先创建：

```bash
mkdir -p dist
python tools/plugins/package-example.py dist/channel-model-mapping-review.afplugin --example channel-model-mapping-review
```

Windows PowerShell：

```powershell
New-Item -ItemType Directory -Force dist
python tools/plugins/package-example.py dist/channel-model-mapping-review.afplugin --example channel-model-mapping-review
```

目标 `.afplugin` 文件必须尚不存在。示例打包脚本会复制目录文件和仓库提供的 Python SDK，再生成安装包；它不会启动插件，也不会安装服务器运行依赖。

## 3. 安装并运行

1. 管理员进入 AnyFlows“插件管理”，上传 `dist/channel-model-mapping-review.afplugin` 并安装。
2. 普通上传安装会完成制品登记。对手工登记的制品，先在插件管理中审批该制品。然后在部署的“高级管理”中明确授予 `channel.model_mapping.review.read` v1，再启用部署。部署 `publicConfig` 必须保持空对象 `{}`。
3. 等待插件节点状态显示就绪。
4. 管理员进入渠道页，选择一个渠道运行模型映射检查。
5. 页面只显示摘要和建议；它不会替管理员改写渠道模型或映射。所选渠道的模型名和映射值会发送给插件，请只安装你信任的代码，并检查渠道中没有不应向插件披露的自定义文本。

本能力是管理员渠道管理入口。调用 API 时必须使用管理员会话，或持有对应权限的管理员 PAT：读取可选插件需要 `admin:list_admin_channel_model_mapping_plugin_reviewers:execute`，运行检查需要 `admin:review_admin_channel_model_mapping_with_plugin:execute`。模型调用 API Key 不用于管理接口。

## 4. 宿主会提供什么

一次调用的数据形状如下：

```json
{
  "models": ["gpt-4o", "gpt-4o-mini"],
  "mappings": [
    {"model": "gpt-4o", "upstreamModel": "vendor-chat-large"},
    {"model": "gpt-4o-mini", "upstreamModel": "vendor-chat-small"}
  ]
}
```

| 字段 | 含义 | 限制 |
| --- | --- | --- |
| `models` | 渠道声明的模型名称列表 | 必填，最多 512 项 |
| `mappings` | 渠道的精确模型映射 | 必填，最多 512 项 |
| `mappings[].model` | 映射来源名称 | 非空，最多 256 UTF-8 字节 |
| `mappings[].upstreamModel` | 映射目标名称 | 非空，最多 256 UTF-8 字节 |

插件只收到这两个数组。不会收到渠道 ID、名称、上游地址、凭据、请求头、参数覆盖、余额或运行日志。未知字段会被输入 Schema 拒绝。

## 5. 输出格式

返回值固定为摘要和检查列表：

```json
{
  "summary": "已检查 2 个模型和 2 条映射。",
  "checks": [
    {
      "code": "models.unique",
      "title": "模型名称没有重复",
      "status": "pass",
      "message": "本次渠道模型清单中的名称均不重复。",
      "recommendation": null
    }
  ]
}
```

- `summary` 是简短总体说明，最多 2048 字节。
- `checks` 最多 64 项。
- 每项必须包含 `code`、`title`、`status`、`message`、`recommendation` 五个字段。
- `status` 只能是 `pass`、`info`、`warning`、`fail`。
- 没有建议时，`recommendation` 仍要传 `null`。
- 检查项的 code 应稳定；调整文案时尽量不改 code。

宿主会再次校验字段、文字长度和输出内容，整个结果最多 16 KiB。页面将插件文字当作纯文本展示。

## 6. 阅读和修改代码

从 `plugin.py` 的 `review_mapping()` 开始：

1. 检查宿主调用的是固定能力，并确认部署没有非空配置。
2. 精确校验输入对象的两个字段、数组数量和模型名称字节长度。
3. 分别检查空模型清单、重复名称、映射来源未列入清单，以及多个来源是否共用一个上游目标。
4. 生成契约要求的摘要和检查项，检查结果大小后返回。
5. 文件底部的 `main()` 读取随包的 Manifest 并启动 SDK 协议循环。

所有规则都只对本次输入做本地计算。特别是“来源未列入模型清单”和“多个来源共用目标”可能是有意配置，所以示例分别使用 `info` 提示，不会阻止保存或自动改变任何数据。

例如，可添加一条检查，提示映射数量超过自定义阈值：

```python
if len(mappings) > 100:
    checks.append(
        _check(
            "mappings.review-volume",
            "映射项较多",
            "info",
            f"当前有 {len(mappings)} 条映射。",
            "确认这些映射仍然需要维护。",
        )
    )
```

新规则使用新的稳定 `code`。修改检查结果前，确认仍满足状态枚举、字段长度、最多 64 项和 16 KiB 总上限。不要把网络请求、自动探测或配置写入加入这个处理函数。

## 7. 打包自己的版本

`package-example.py` 总是打包仓库中的原始示例。要发布自己的修改，请把整个示例目录复制到新目录，例如 `my-channel-mapping-review`，修改 `manifest.json` 中唯一的 `pluginId`、展示名称和版本号，然后调整 `plugin.py`。

把 SDK 复制进你的分发目录，再打包：

```bash
cp tools/plugins/sdk/python/anyflows_plugin.py my-channel-mapping-review/anyflows_plugin.py
python tools/plugins/package-plugin.py my-channel-mapping-review my-channel-mapping-review/manifest.json plugin.py dist/my-channel-mapping-review.afplugin
```

PowerShell：

```powershell
Copy-Item tools/plugins/sdk/python/anyflows_plugin.py my-channel-mapping-review/anyflows_plugin.py
python tools/plugins/package-plugin.py my-channel-mapping-review my-channel-mapping-review/manifest.json plugin.py dist/my-channel-mapping-review.afplugin
```

通用打包命令的四个参数依次是：只包含要发布文件的目录、Manifest 文件路径、目录内入口路径、尚不存在的输出包路径。分发目录至少要有 `manifest.json`、`plugin.py` 和 `anyflows_plugin.py`。模型映射输入输出格式和能力 ID 属于宿主固定契约；若要改变字段，需要先由 AnyFlows 维护者发布新契约。
