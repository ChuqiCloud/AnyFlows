#!/usr/bin/python3
"""模型资料目录示例：只读本地数据，不访问网络，也不需要任何凭据。

本文件只演示资料查找和分页。AnyFlows Python SDK 负责 JSON Lines 通信；
下面的处理函数只校验输入，并返回符合 manifest.json 的普通 Python 字典。
"""

from __future__ import annotations

import json
from pathlib import Path
import sys

from anyflows_plugin import Plugin, PluginError


# 按当前文件定位数据文件。宿主可以从任意工作目录启动插件进程，不能假定
# 当前工作目录就是插件文件所在目录。
ROOT = Path(__file__).resolve().parent


def read_catalog(capability_id: str, public_config: dict, value: object) -> dict:
    """筛选包内的公开目录资料，并返回一页结果。"""
    # 宿主只会调用插件声明的固定能力。两个能力共用相同的输入输出 Schema，
    # 所以本例用一个处理函数实现读取与搜索。
    if capability_id not in {"catalog.models.read", "catalog.models.search"}:
        raise PluginError("unsupported")

    # 本例不需要管理员配置。拒绝未约定字段，避免看似无害地接收了密码、密钥
    # 或其他本示例并未设计处理的数据。
    if (
        public_config
        or not isinstance(value, dict)
        or set(value) - {"query", "cursor", "limit"}
    ):
        raise PluginError("invalid_configuration")

    # 宿主在调用前也会校验输入 Schema；处理函数再做一次检查，便于单独阅读、
    # 调试和复用时尽早发现输入问题。
    query = value.get("query")
    cursor = value.get("cursor")
    # 模型页会发送 limit；默认值 20 只是方便开发者单独调用这个普通函数。
    limit = value.get("limit", 20)
    if query is not None and (not isinstance(query, str) or len(query.encode("utf-8")) > 128):
        raise PluginError("invalid_configuration")
    if cursor is not None and (
        not isinstance(cursor, str)
        or not cursor.isascii()
        or not cursor.isdecimal()
        or len(cursor) > 8
    ):
        raise PluginError("invalid_configuration")
    if type(limit) is not int or not 1 <= limit <= 50:
        raise PluginError("invalid_configuration")

    # models.json 只包含用于展示的示例资料，不会在平台创建可调用模型、路由、
    # 渠道凭据、价格卡或计费规则。
    models = json.loads((ROOT / "models.json").read_text(encoding="utf-8"))
    search = (query or "").strip().casefold()
    if search:
        # 搜索模型 ID、展示名称和提供商名称；只筛选资料，不修改资料本身。
        models = [
            model
            for model in models
            if search
            in " ".join((model["id"], model["displayName"], model["provider"])).casefold()
        ]

    # 为了让新手容易看懂，本例把游标当作列表偏移量。真实数据源也可以返回
    # 不透明游标；但必须符合宿主发布的长度和分页约定。
    offset = int(cursor) if cursor is not None else 0
    end = offset + limit
    # nextCursor 为 null 表示已经到最后一页；否则调用方会把字符串原样传回，
    # 插件据此从下一项继续读取。
    return {
        "models": models[offset:end],
        "nextCursor": str(end) if end < len(models) else None,
    }


def main() -> int:
    # 读取与 plugin.py 一起打包的清单，然后让 SDK 持续处理宿主请求。不要用
    # print() 向标准输出写调试信息，因为那里只传输协议帧。
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    return Plugin(manifest, read_catalog).serve()


if __name__ == "__main__":
    sys.exit(main())
