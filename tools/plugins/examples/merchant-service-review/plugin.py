#!/usr/bin/python3
"""商户服务资料检查示例。

这个文件只包含检查规则，不需要导入 AnyFlows 的 Rust crate，也不访问网络、
数据库、API Key、上游地址或价格。宿主通过标准输入输出与本进程通信。
"""

from __future__ import annotations

# 以下三个模块都属于 Python 标准库，无需另外安装第三方依赖。
import json
from pathlib import Path
import sys
from typing import Any

# 这是 AnyFlows 提供的 Python 协议 SDK。打包时必须把这个文件也放进插件包；
# README 说明了如何从 tools/plugins/sdk/python/ 复制它。
from anyflows_plugin import Plugin, PluginError


# manifest.json 与 plugin.py 放在同一目录。Path(__file__) 指向当前文件，
# 因此无论从哪个工作目录启动，都能找到随包发布的清单。
ROOT = Path(__file__).resolve().parent

# 能力 ID 是本示例支持的业务入口。要改成别的能力，不能只改这里；宿主也必须
# 实现对应调用入口，并使用相同的能力名称、版本和数据 Schema。
CAPABILITY = "merchant.service.review.read"

# 一条公开模型资料必须包含这些字段。本次检查只使用公开模型 ID 与能力标记，
# 不读取上游模型映射、价格倍率、接口地址或密钥。
MODEL_FIELDS = {
    "model",
    "context_window",
    "max_output_tokens",
    "supports_streaming",
    "supports_tools",
    "supports_vision",
}


def review_service(
    capability_id: str,
    public_config: dict[str, Any],
    value: object,
) -> dict[str, Any]:
    """检查宿主传来的公开服务资料，并返回纯 JSON 建议。

    这三个参数由 SDK 提供：

    - capability_id：本次调用的能力名称。
    - public_config：管理员为插件部署填写的公开配置。本示例不用配置，要求它为空。
    - value：商户服务的公开资料，格式详见同目录 README 与 manifest.json。
    """
    # 即使 SDK 已按 Manifest 限制能力，处理函数仍明确检查名称，避免未来复制代码
    # 后误把不相关能力的输入交给本函数处理。
    if capability_id != CAPABILITY:
        raise PluginError("unsupported")

    # 这个宿主入口当前要求 publicConfig 是空对象；业务资料只从 value 传入。
    # 外层只允许一个 service 字段，拒绝多余字段，避免无意中接受未约定的数据。
    if public_config or not isinstance(value, dict) or set(value) != {"service"}:
        raise PluginError("invalid_configuration")

    service = value["service"]
    service_fields = {"name", "description", "protocol", "models"}
    if not isinstance(service, dict) or set(service) != service_fields:
        raise PluginError("invalid_configuration")

    # Schema 已定义这些字段类型；插件仍检查基本结构，独立运行或以后修改时也
    # 能尽早发现输入不符合约定的问题。
    if not all(
        isinstance(service[key], str)
        for key in ("name", "description", "protocol")
    ):
        raise PluginError("invalid_configuration")

    models = service["models"]
    if not isinstance(models, list) or len(models) > 128:
        raise PluginError("invalid_configuration")
    for model in models:
        # set(model) 检查字段名是否与 v1 Schema 完全一致。每个值的具体类型和
        # 长度由宿主在调用前按 Schema 校验。
        if not isinstance(model, dict) or set(model) != MODEL_FIELDS:
            raise PluginError("invalid_configuration")

    # 每个检查项都用固定结构返回。code 是程序可识别的稳定标识，title、message
    # 和 recommendation 是展示给人的文字。新增检查时应使用新的 code，不要复用
    # 现有 code 表示不同问题。
    checks: list[dict[str, Any]] = []

    # 规则一：服务至少应列出一个模型。warning 表示需要关注，但示例不会阻止提交。
    if not models:
        checks.append(
            {
                "code": "models.empty",
                "title": "No models configured",
                "status": "warning",
                "message": "This service does not list any models yet.",
                "recommendation": "Add at least one model before submitting the service.",
            }
        )
    else:
        checks.append(
            {
                "code": "models.present",
                "title": "Models are listed",
                "status": "pass",
                "message": f"The service lists {len(models)} model(s).",
                "recommendation": None,
            }
        )

    # 规则二：给每个模型检查公开 ID 是否非空。当前宿主 Schema 已拒绝空 ID；
    # 这里保留检查作为示例，也展示如何用列表推导式检查一组模型。
    named_models = [
        model
        for model in models
        if isinstance(model.get("model"), str) and model["model"].strip()
    ]
    if len(named_models) != len(models):
        checks.append(
            {
                "code": "models.names",
                "title": "Model names need attention",
                "status": "warning",
                "message": "At least one listed model has a blank name.",
                "recommendation": "Give each model a non-empty public model ID.",
            }
        )
    else:
        checks.append(
            {
                "code": "models.names",
                "title": "Model names are present",
                "status": "pass",
                "message": "Every listed model has a public model ID.",
                "recommendation": None,
            }
        )

    # 规则三：context_window 允许为 null，表示商户暂时没有填写。info 仅提示缺项，
    # 不代表插件已验证模型真实上下文长度。
    missing_context = [
        model["model"] for model in models if model.get("context_window") is None
    ]
    if missing_context:
        checks.append(
            {
                "code": "models.context-window",
                "title": "Context windows are incomplete",
                "status": "info",
                "message": f"{len(missing_context)} model(s) do not specify a context window.",
                "recommendation": "Add context-window details when they are known.",
            }
        )
    else:
        checks.append(
            {
                "code": "models.context-window",
                "title": "Context windows are specified",
                "status": "pass",
                "message": "Every listed model specifies a context window.",
                "recommendation": None,
            }
        )

    # 规则四：说明少于 20 个字符时提示商户补充内容。这个阈值只是示例业务规则，
    # 可按自己的审核标准调整；插件不能据此自动更改服务或替管理员作审批决定。
    description = service["description"].strip()
    if len(description) < 20:
        checks.append(
            {
                "code": "service.description",
                "title": "Service description is brief",
                "status": "info",
                "message": "The description may not explain the service clearly enough to buyers.",
                "recommendation": "Describe the service behavior and notable model capabilities.",
            }
        )
    else:
        checks.append(
            {
                "code": "service.description",
                "title": "Service description is present",
                "status": "pass",
                "message": "The description contains useful detail.",
                "recommendation": None,
            }
        )

    # summary 是给人看的总体说明。所有字段都必须符合 manifest.json 中的输出 Schema。
    summary = (
        f"Reviewed {service['name']} ({service['protocol']}) "
        f"and found {len(checks)} documentation checks."
    )
    result = {"summary": summary, "checks": checks}

    # 宿主对最终输出另有限制。这里提前检查，避免一次规则修改意外生成过大的响应。
    # 使用 UTF-8 字节数而不是字符数，因为传输大小按字节计算。
    output_bytes = json.dumps(result, ensure_ascii=False).encode("utf-8")
    if len(output_bytes) > 16 * 1024:
        raise PluginError("invalid_response")

    return result


def main() -> int:
    """读取随包 Manifest，并让 SDK 持续处理宿主发来的协议请求。"""
    manifest_path = ROOT / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

    # 不要自行读取 stdin 或向 stdout 打印调试信息。serve() 负责 JSON Lines
    # 握手、健康检查、请求关联、截止时间、错误码和响应帧格式。
    return Plugin(manifest, review_service).serve()


if __name__ == "__main__":
    # 只有直接启动这个文件时才进入协议循环；被其他 Python 文件导入时不会启动。
    sys.exit(main())
