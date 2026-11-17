#!/usr/bin/python3
"""Create a short report from one host-projected channel health event."""

from __future__ import annotations

import json
from pathlib import Path
import sys
from typing import Any

from anyflows_plugin import Plugin, PluginError

ROOT = Path(__file__).resolve().parent
CAPABILITY = "events.channel-health.changed.consume"
INPUT_FIELDS = {"eventKey", "targetId", "channelId", "status", "observedAt"}


def is_positive_integer(value: object) -> bool:
    """Reject bool, which is a subclass of int in Python."""
    return type(value) is int and value > 0


def review_event(
    capability_id: str,
    public_config: dict[str, Any],
    value: object,
) -> dict[str, str]:
    """Summarize a single state change; this handler never probes or edits a channel."""
    if capability_id != CAPABILITY:
        raise PluginError("unsupported")
    if public_config:
        raise PluginError("invalid_configuration")
    if not isinstance(value, dict) or set(value) != INPUT_FIELDS:
        raise PluginError("invalid_configuration")

    event_key = value["eventKey"]
    target_id = value["targetId"]
    channel_id = value["channelId"]
    observed_at = value["observedAt"]
    status = value["status"]
    if not all(is_positive_integer(item) for item in (target_id, channel_id, observed_at)):
        raise PluginError("invalid_configuration")
    if (
        not isinstance(event_key, str)
        or not event_key
        or len(event_key) > 128
        or any(ord(character) < 0x20 for character in event_key)
    ):
        raise PluginError("invalid_configuration")
    if status not in {"down", "recovered"}:
        raise PluginError("invalid_configuration")

    if status == "down":
        result = {
            "summary": f"Channel {channel_id} monitor target {target_id} changed to down.",
            "status": "warning",
            "recommendation": "Review the upstream status and recent monitoring history.",
        }
    else:
        result = {
            "summary": f"Channel {channel_id} monitor target {target_id} recovered.",
            "status": "info",
            "recommendation": "Confirm subsequent monitoring events remain stable.",
        }

    if len(json.dumps(result, ensure_ascii=False).encode("utf-8")) > 2 * 1024:
        raise PluginError("invalid_response")
    return result


def main() -> int:
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    return Plugin(manifest, review_event).serve()


if __name__ == "__main__":
    sys.exit(main())
