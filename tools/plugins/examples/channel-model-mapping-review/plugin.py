#!/usr/bin/python3
"""只读取模型名和映射关系，生成静态检查建议；不访问上游或修改渠道。"""

from __future__ import annotations

import json
from pathlib import Path
import sys
from typing import Any

from anyflows_plugin import Plugin, PluginError


# Manifest 和公共 SDK 与入口文件一起放在插件目录中。
ROOT = Path(__file__).resolve().parent
CAPABILITY = "channel.model_mapping.review.read"
INPUT_FIELDS = {"models", "mappings"}
MAPPING_FIELDS = {"model", "upstreamModel"}
ALLOWED_STATUSES = {"pass", "info", "warning", "fail"}


def _valid_model_name(value: object) -> bool:
    """只接受非空、无首尾空格和控制字符且不超过 256 UTF-8 字节的名称。"""
    return (
        isinstance(value, str)
        and bool(value)
        and value == value.strip()
        and len(value.encode("utf-8")) <= 256
        and not any(ord(char) < 32 or ord(char) == 127 for char in value)
    )


def _check(
    code: str,
    title: str,
    status: str,
    message: str,
    recommendation: str | None = None,
) -> dict[str, Any]:
    """统一生成契约规定的五个字段，避免某条规则漏掉 recommendation。"""
    return {
        "code": code,
        "title": title,
        "status": status,
        "message": message,
        "recommendation": recommendation,
    }


def review_mapping(
    capability_id: str,
    public_config: dict[str, Any],
    value: object,
) -> dict[str, Any]:
    """检查宿主提供的模型清单和映射，不执行探测或写入。"""
    # 能力名由宿主固定。Manifest 声明别的能力不会自动把数据交给本函数。
    if capability_id != CAPABILITY:
        raise PluginError("unsupported")

    # 本示例不需要管理配置；拒绝非空配置，避免把凭据误当作普通选项传入。
    if not isinstance(public_config, dict) or public_config:
        raise PluginError("invalid_configuration")

    # 输入必须恰好包含两个字段，拒绝上游地址、令牌等未约定数据。
    if not isinstance(value, dict) or set(value) != INPUT_FIELDS:
        raise PluginError("invalid_configuration")
    models = value["models"]
    mappings = value["mappings"]
    if not isinstance(models, list) or len(models) > 512:
        raise PluginError("invalid_configuration")
    if not isinstance(mappings, list) or len(mappings) > 512:
        raise PluginError("invalid_configuration")

    # 再校验一次输入，便于读懂业务处理函数；宿主也会按 Manifest Schema 检查。
    if any(not _valid_model_name(model) for model in models):
        raise PluginError("invalid_configuration")
    for mapping in mappings:
        if (
            not isinstance(mapping, dict)
            or set(mapping) != MAPPING_FIELDS
            or not _valid_model_name(mapping["model"])
            or not _valid_model_name(mapping["upstreamModel"])
        ):
            raise PluginError("invalid_configuration")

    checks: list[dict[str, Any]] = []
    model_set = set(models)

    # 规则一：空列表只说明没有可检查的公开模型，不代表渠道本身不可用。
    if not models:
        checks.append(
            _check(
                "models.empty",
                "渠道没有声明模型",
                "warning",
                "宿主提供的渠道模型清单为空。",
                "确认渠道模型列表是否已配置；本插件不会替你添加模型。",
            )
        )
    else:
        checks.append(
            _check(
                "models.present",
                "渠道已声明模型",
                "pass",
                f"本次快照包含 {len(models)} 个模型名称。",
            )
        )

    # 规则二：模型名大小写敏感，完全相同的项目可能导致列表重复或配置困惑。
    duplicate_models = sorted(
        model for model in model_set if models.count(model) > 1
    )
    if duplicate_models:
        checks.append(
            _check(
                "models.duplicate",
                "模型清单包含重复名称",
                "warning",
                f"发现 {len(duplicate_models)} 个重复模型名称。",
                "检查渠道模型列表并移除重复项。",
            )
        )
    else:
        checks.append(
            _check(
                "models.unique",
                "模型名称没有重复",
                "pass",
                "本次渠道模型清单中的名称均不重复。",
            )
        )

    # 规则三：映射键不在公开模型列表时给信息提示。某些配置允许别名，
    # 因此不把它判为错误，也不自动删除该映射。
    unlisted_sources = sorted(
        {mapping["model"] for mapping in mappings if mapping["model"] not in model_set}
    )
    if unlisted_sources:
        checks.append(
            _check(
                "mappings.source-not-listed",
                "部分映射来源未列入模型清单",
                "info",
                f"有 {len(unlisted_sources)} 个映射来源名称不在本次模型清单中。",
                "确认这些名称是否是有意保留的别名。",
            )
        )
    else:
        checks.append(
            _check(
                "mappings.sources-listed",
                "映射来源都在模型清单中",
                "pass",
                "本次提供的映射来源名称都能在渠道模型清单中找到。",
            )
        )

    # 规则四：多个来源指向同一个上游模型可能是有意设置，只提示不阻止使用。
    targets: dict[str, set[str]] = {}
    for mapping in mappings:
        targets.setdefault(mapping["upstreamModel"], set()).add(mapping["model"])
    shared_targets = sum(len(sources) > 1 for sources in targets.values())
    if shared_targets:
        checks.append(
            _check(
                "mappings.shared-target",
                "存在多个来源共用上游模型",
                "info",
                f"有 {shared_targets} 个上游模型分别承接多个映射来源。",
                "确认这是有意的别名关系。",
            )
        )
    elif mappings:
        checks.append(
            _check(
                "mappings.targets-distinct",
                "映射目标没有合并",
                "pass",
                "每个上游模型目前只对应一个映射来源。",
            )
        )
    else:
        checks.append(
            _check(
                "mappings.empty",
                "没有配置显式映射",
                "info",
                "宿主提供的映射表为空；请求可能直接使用模型名称。",
            )
        )

    # 输出只包含计数和固定提示，避免将渠道配置原文写进结果或日志。
    result = {
        "summary": f"已检查 {len(models)} 个模型和 {len(mappings)} 条映射。",
        "checks": checks[:64],
    }
    try:
        encoded = json.dumps(result, ensure_ascii=False, allow_nan=False).encode("utf-8")
    except (TypeError, ValueError):
        raise PluginError("invalid_response") from None
    if len(encoded) > 16 * 1024:
        raise PluginError("invalid_response")
    if any(check["status"] not in ALLOWED_STATUSES for check in result["checks"]):
        raise PluginError("invalid_response")
    return result


def main() -> int:
    # SDK 负责握手和 JSON Lines 通信；标准输出不能用于调试日志。
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    return Plugin(manifest, review_mapping).serve()


if __name__ == "__main__":
    sys.exit(main())
