#!/usr/bin/python3
"""只分析 AnyFlows 已记录的渠道监控事件，不执行网络请求。"""

from __future__ import annotations

import json
from pathlib import Path
import sys
from typing import Any

from anyflows_plugin import Plugin, PluginError


# 运行时清单跟随插件包放置。它描述本插件支持的固定能力和 JSON Schema。
ROOT = Path(__file__).resolve().parent
CAPABILITY = "channel.monitor.events.review.read"
ACTION_PROPOSAL_CAPABILITY = "channel.monitor.target.action-propose.read"
EVENT_FIELDS = {"kind", "latency_ms", "occurred_epoch_seconds"}
ALLOWED_KINDS = {"down", "recovered"}


def _is_non_negative_integer(value: object) -> bool:
    """bool 在 Python 中是 int 子类，因此用 type(...) 精确排除 true/false。"""
    return type(value) is int and value >= 0


def _check(
    code: str,
    title: str,
    status: str,
    message: str,
    recommendation: str | None = None,
) -> dict[str, Any]:
    """集中构造固定格式的检查项，避免不同规则漏掉某个必填字段。"""
    return {
        "code": code,
        "title": title,
        "status": status,
        "message": message,
        "recommendation": recommendation,
    }


def review_events(
    capability_id: str,
    public_config: dict[str, Any],
    value: object,
) -> dict[str, Any]:
    """按最近的监控事件给管理员提供一份只读摘要和建议。

    新规则应在下面的“检查规则”部分添加稳定的 code 和检查项；不要把
    探测请求、渠道修改或路由权重写入这里。宿主只会给本插件提供事件快照。
    """
    # 宿主固定调用这个能力。拒绝意外传入的能力名称，避免复用处理器时混淆。
    if capability_id not in {CAPABILITY, ACTION_PROPOSAL_CAPABILITY}:
        raise PluginError("unsupported")

    # 当前业务接入不需要部署设置；配置必须保持空对象。
    if public_config:
        raise PluginError("invalid_configuration")

    # 只接受契约声明的输入对象和字段，不猜测、不吞掉额外数据。
    if not isinstance(value, dict) or set(value) != {"events", "targetEnabled"}:
        raise PluginError("invalid_configuration")
    if type(value["targetEnabled"]) is not bool:
        raise PluginError("invalid_configuration")
    events = value["events"]
    if not isinstance(events, list) or len(events) > 100:
        raise PluginError("invalid_configuration")

    # 再做一次本地校验，便于开发者独立调试；宿主也会执行 Manifest Schema 校验。
    for event in events:
        if not isinstance(event, dict) or set(event) != EVENT_FIELDS:
            raise PluginError("invalid_configuration")
        if event["kind"] not in ALLOWED_KINDS:
            raise PluginError("invalid_configuration")
        if not _is_non_negative_integer(event["occurred_epoch_seconds"]):
            raise PluginError("invalid_configuration")
        latency = event["latency_ms"]
        if latency is not None and not _is_non_negative_integer(latency):
            raise PluginError("invalid_configuration")

    # 数据库按时间倒序返回事件。内部按时间升序分析，避免依赖输入顺序。
    ordered = sorted(events, key=lambda event: event["occurred_epoch_seconds"])
    down_count = sum(event["kind"] == "down" for event in ordered)
    recovered_count = sum(event["kind"] == "recovered" for event in ordered)
    recent_kinds = [event["kind"] for event in ordered[-6:]]
    recovered_latencies = [
        event["latency_ms"]
        for event in ordered
        if event["kind"] == "recovered" and event["latency_ms"] is not None
    ]

    checks: list[dict[str, Any]] = []

    # 检查规则 1：空历史只表示当前没有可分析事件，不推断探测器状态。
    if not ordered:
        summary = "当前目标没有已记录的监控事件。"
        checks.append(
            _check(
                "events.empty",
                "暂无监控历史",
                "info",
                "尚无 down 或 recovered 事件可供分析。",
                "启用目标并等待下一次已配置的监控周期。",
            )
        )
    else:
        # 检查规则 2：仅按事件类型计数，不访问渠道或补做探测。
        summary = (
            f"最近 {len(ordered)} 条记录中有 {down_count} 次 down、"
            f"{recovered_count} 次 recovered。"
        )
        if down_count == 0:
            checks.append(
                _check(
                    "events.no-down",
                    "记录中没有故障事件",
                    "pass",
                    "本次提供的事件快照中没有 down 记录。",
                )
            )
        else:
            checks.append(
                _check(
                    "events.down-count",
                    "发现故障事件",
                    "warning",
                    f"最近 {len(ordered)} 条记录中包含 {down_count} 次 down。",
                    "结合目标的监控周期和告警阈值检查上游状态。",
                )
            )

        # 检查规则 3：快照最后一条仍是 down 时，标记为“尚未观察到恢复”。
        # 这只描述所给历史窗口，不声称渠道目前仍然故障。
        if recent_kinds[-1] == "down":
            checks.append(
                _check(
                    "events.no-later-recovery",
                    "快照末尾尚未记录恢复",
                    "warning",
                    "最新事件是 down；这份快照之后是否恢复尚不可知。",
                    "刷新监控事件后再判断当前状态。",
                )
            )
        elif recovered_count:
            checks.append(
                _check(
                    "events.recovered",
                    "检测到恢复记录",
                    "pass",
                    "事件快照的最新一条记录是 recovered。",
                )
            )

        # 检查规则 4：有采样延迟时给出均值；没有时只说明数据不足。
        if recovered_latencies:
            average_latency = sum(recovered_latencies) // len(recovered_latencies)
            if average_latency >= 5_000:
                checks.append(
                    _check(
                        "events.recovery-latency-high",
                        "恢复探测延迟偏高",
                        "warning",
                        f"{len(recovered_latencies)} 条恢复记录的平均延迟约为 {average_latency} ms。",
                        "检查上游响应时间和当前监控超时设置。",
                    )
                )
            else:
                checks.append(
                    _check(
                        "events.recovery-latency",
                        "恢复探测延迟在示例阈值内",
                        "pass",
                        f"{len(recovered_latencies)} 条恢复记录的平均延迟约为 {average_latency} ms。",
                    )
                )
        else:
            checks.append(
                _check(
                    "events.latency-unavailable",
                    "缺少恢复延迟样本",
                    "info",
                    "事件快照中没有带延迟值的 recovered 记录。",
                )
            )

    # 结果契约最多允许 64 项；这个示例固定返回少量规则，仍显式守住上限。
    result = {"summary": summary, "checks": checks[:64]}
    if ordered and ordered[-1]["kind"] == "down" and value["targetEnabled"]:
        result["actionProposal"] = {
            "actionId": "active-channel-monitor.target.set-enabled",
            "actionVersion": 1,
            "input": {"enabled": False},
        }
    elif ordered and ordered[-1]["kind"] == "recovered" and not value["targetEnabled"]:
        result["actionProposal"] = {
            "actionId": "active-channel-monitor.target.set-enabled",
            "actionVersion": 1,
            "input": {"enabled": True},
        }
    encoded = json.dumps(result, ensure_ascii=False, allow_nan=False).encode("utf-8")
    if len(encoded) > 16 * 1024:
        # 这是业务数据无效的统一错误，不把本地路径或异常内容发回宿主。
        raise PluginError("invalid_response")
    return result


def main() -> int:
    # Plugin SDK 负责 JSON Lines 握手、健康响应、请求关联和错误帧。
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    return Plugin(manifest, review_events).serve()


if __name__ == "__main__":
    # 只有协议帧写入标准输出；调试日志应写标准错误。
    sys.exit(main())
