#!/usr/bin/env python3
"""Compare stored balance history with one explicitly approved live snapshot."""

from __future__ import annotations

from decimal import Decimal, InvalidOperation
import json
from pathlib import Path
from typing import Any

from anyflows_plugin import Plugin, PluginError

ROOT = Path(__file__).resolve().parent
CAPABILITY = "account.balance.snapshot.live-review.read"
MAX_SNAPSHOTS = 20


def parse_amount(value: object) -> Decimal | None:
    """Parse a finite decimal string without converting provider units."""
    if not isinstance(value, str) or not value or len(value) > 128:
        return None
    try:
        result = Decimal(value)
    except InvalidOperation:
        return None
    return result if result.is_finite() else None


def review_live_balance(
    capability_id: str,
    public_config: dict[str, Any],
    value: object,
    context,
) -> dict[str, Any]:
    """Compare a host-provided history with a fresh balance projection.

    The only callback used here is the fixed `account.balance` endpoint. The
    host binds it to the current user's selected account, reads its credential
    internally, records a balance snapshot, and returns an allowlisted JSON
    projection. This handler cannot select another account or provider URL.
    """
    if capability_id != CAPABILITY:
        raise PluginError("unsupported")
    if public_config or not isinstance(value, dict) or set(value) != {"snapshots"}:
        raise PluginError("invalid_configuration")
    snapshots = value["snapshots"]
    if not isinstance(snapshots, list) or not 1 <= len(snapshots) <= MAX_SNAPSHOTS:
        raise PluginError("invalid_configuration")

    latest = context.proxy_read("account.balance")
    if not isinstance(latest, dict) or set(latest) != {"amount", "unit", "status", "observedAt"}:
        raise PluginError("invalid_response")
    if latest["status"] != "available":
        raise PluginError("temporarily_unavailable")
    current = parse_amount(latest["amount"])
    unit = latest["unit"]
    if current is None or not isinstance(unit, str) or not unit:
        raise PluginError("invalid_response")

    prior: list[tuple[int, Decimal]] = []
    for snapshot in snapshots:
        if not isinstance(snapshot, dict) or set(snapshot) != {"amount", "unit", "status", "observedAt"}:
            raise PluginError("invalid_configuration")
        if snapshot["status"] != "available" or snapshot["unit"] != unit:
            continue
        observed_at = snapshot["observedAt"]
        amount = parse_amount(snapshot["amount"])
        if type(observed_at) is not int or observed_at < 1 or amount is None:
            raise PluginError("invalid_configuration")
        prior.append((observed_at, amount))

    latest_observed_at = latest["observedAt"]
    if type(latest_observed_at) is not int or latest_observed_at < 1:
        raise PluginError("invalid_response")
    if not prior:
        return {
            "summary": f"A current {unit} balance was recorded, but the saved history has no comparable amount.",
            "checks": [{
                "code": "no-comparable-history",
                "title": "No comparable saved balance",
                "status": "info",
                "message": f"The current provider reading is {current} {unit}.",
                "recommendation": "Compare future snapshots using the same provider unit.",
            }],
        }

    observed_at, previous = max(prior, key=lambda item: item[0])
    delta = current - previous
    direction = "increased" if delta > 0 else "decreased" if delta < 0 else "did not change"
    return {
        "summary": f"The newly recorded {unit} balance {direction} by {delta} {unit} since the latest comparable saved snapshot.",
        "checks": [{
            "code": "latest-balance-change",
            "title": "Latest balance comparison",
            "status": "info",
            "message": f"The provider returned {current} {unit}; the prior comparable amount was {previous} {unit}.",
            "recommendation": "Use the provider's own records to verify unexpected changes.",
        }],
    }


def main() -> None:
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    Plugin(manifest, review_live_balance).serve()


if __name__ == "__main__":
    main()
