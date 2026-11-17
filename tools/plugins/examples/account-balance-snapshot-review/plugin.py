#!/usr/bin/env python3
"""Example analysis of user-selected, already stored balance snapshots."""

from __future__ import annotations

from decimal import Decimal, InvalidOperation
import json
from pathlib import Path
from typing import Any

from anyflows_plugin import Plugin, PluginError

ROOT = Path(__file__).resolve().parent
CAPABILITY = "account.balance.snapshot.review.read"
MAX_SNAPSHOTS = 20


def review_snapshots(
    capability_id: str,
    public_config: dict[str, Any],
    value: object,
) -> dict[str, Any]:
    """Summarize changes within the user-selected snapshot history.

    The host sends only time, native amount/unit and status. There is no account
    identifier, provider identity, endpoint, credential, or AnyFlows user ID.
    This example is analysis only: it does not contact a provider or change data.
    """
    if capability_id != CAPABILITY:
        raise PluginError("unsupported")
    if public_config or not isinstance(value, dict) or set(value) != {"snapshots"}:
        raise PluginError("invalid_configuration")

    snapshots = value["snapshots"]
    if not isinstance(snapshots, list) or not 1 <= len(snapshots) <= MAX_SNAPSHOTS:
        raise PluginError("invalid_configuration")

    by_unit: dict[str, list[tuple[int, Decimal]]] = {}
    unavailable = 0
    for item in snapshots:
        if not isinstance(item, dict) or set(item) != {"amount", "unit", "status", "observedAt"}:
            raise PluginError("invalid_configuration")
        status = item["status"]
        observed_at = item["observedAt"]
        if status not in {"available", "unavailable", "authentication_required"}:
            raise PluginError("invalid_configuration")
        if type(observed_at) is not int or observed_at < 1:
            raise PluginError("invalid_configuration")
        if status != "available":
            if item["amount"] is not None or item["unit"] is not None:
                raise PluginError("invalid_configuration")
            unavailable += 1
            continue

        amount = item["amount"]
        unit = item["unit"]
        if not isinstance(amount, str) or not isinstance(unit, str) or not unit.strip():
            raise PluginError("invalid_configuration")
        try:
            numeric = Decimal(amount)
        except InvalidOperation as error:
            raise PluginError("invalid_configuration") from error
        if not numeric.is_finite():
            raise PluginError("invalid_configuration")
        by_unit.setdefault(unit, []).append((observed_at, numeric))

    checks: list[dict[str, Any]] = []
    for unit, values in sorted(by_unit.items()):
        values.sort(key=lambda pair: pair[0])
        first = values[0][1]
        latest = values[-1][1]
        delta = latest - first
        direction = "increased" if delta > 0 else "decreased" if delta < 0 else "did not change"
        checks.append({
            "code": "balance-history-change",
            "title": f"Recorded {unit} balance",
            "status": "info",
            "message": f"Across {len(values)} available snapshots, the recorded amount {direction} by {delta} {unit}.",
            "recommendation": "Compare with the provider's own records if this change is unexpected.",
        })

    if unavailable:
        checks.append({
            "code": "incomplete-history",
            "title": "Some snapshots have no amount",
            "status": "warning",
            "message": f"{unavailable} of {len(snapshots)} snapshots were unavailable or required updated authentication.",
            "recommendation": "Refresh the account snapshot from its account page if you want a current observation.",
        })
    if not checks:
        checks.append({
            "code": "no-comparable-amounts",
            "title": "No comparable amounts",
            "status": "info",
            "message": "The selected history contains no available amount snapshots to compare.",
            "recommendation": None,
        })

    return {
        "summary": f"Reviewed {len(snapshots)} stored snapshots across {len(by_unit)} unit(s). This is a descriptive comparison, not a provider balance or financial recommendation.",
        "checks": checks[:64],
    }


def main() -> None:
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    Plugin(manifest, review_snapshots).serve()


if __name__ == "__main__":
    main()
