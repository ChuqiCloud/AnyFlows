#!/usr/bin/python3
"""Create a small report from a public merchant service state transition."""

from __future__ import annotations

import json
from pathlib import Path
import sys
from typing import Any

from anyflows_plugin import Plugin, PluginError

ROOT = Path(__file__).resolve().parent
CAPABILITY = "events.merchant-service.changed.consume"
INPUT_FIELDS = {"eventKey", "serviceId", "revision", "status", "observedAt"}


def is_positive_integer(value: object) -> bool:
    """Reject bool, which is a subclass of int in Python."""
    return type(value) is int and value > 0


def review_event(
    capability_id: str,
    public_config: dict[str, Any],
    value: object,
) -> dict[str, str]:
    """Summarize the state and use eventKey to deduplicate retries in future side effects."""
    if capability_id != CAPABILITY or public_config:
        raise PluginError("unsupported")
    if not isinstance(value, dict) or set(value) != INPUT_FIELDS:
        raise PluginError("invalid_event")

    event_key = value["eventKey"]
    service_id = value["serviceId"]
    revision = value["revision"]
    observed_at = value["observedAt"]
    status = value["status"]
    if not all(is_positive_integer(item) for item in (service_id, revision, observed_at)):
        raise PluginError("invalid_event")
    if (
        not isinstance(event_key, str)
        or not event_key
        or len(event_key) > 128
        or any(ord(character) < 0x20 for character in event_key)
    ):
        raise PluginError("invalid_event")
    if status not in {"active", "paused"}:
        raise PluginError("invalid_event")

    if status == "active":
        result = {
            "summary": f"Merchant service {service_id} revision {revision} is active.",
            "status": "info",
            "recommendation": "Confirm the service remains available to its intended audience.",
        }
    else:
        result = {
            "summary": f"Merchant service {service_id} revision {revision} is paused.",
            "status": "warning",
            "recommendation": "Review the service state before directing new traffic to it.",
        }

    if len(json.dumps(result, ensure_ascii=False).encode("utf-8")) > 2 * 1024:
        raise PluginError("invalid_response")
    return result


def main() -> int:
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    return Plugin(manifest, review_event).serve()


if __name__ == "__main__":
    sys.exit(main())
