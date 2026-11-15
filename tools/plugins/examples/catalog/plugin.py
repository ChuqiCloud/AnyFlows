#!/usr/bin/python3
"""Local, read-only catalogue example; no network or credentials are required.

This file contains example data lookup rules. The AnyFlows Python SDK handles the
JSON Lines protocol; the handler below only validates its input and returns a
regular Python dictionary that matches manifest.json.
"""

from __future__ import annotations

import json
from pathlib import Path
import sys

from anyflows_plugin import Plugin, PluginError


# Resolve data paths relative to this file so the example works no matter what
# working directory the operating system uses to start the plugin process.
ROOT = Path(__file__).resolve().parent


def read_catalog(capability_id: str, public_config: dict, value: object) -> dict:
    """Filter the bundled public catalogue and return one page of results."""
    # The host chooses one of these two fixed capabilities. The same handler can
    # serve both because they share the same input and output Schema.
    if capability_id not in {"catalog.models.read", "catalog.models.search"}:
        raise PluginError("unsupported")

    # This example has no administrator settings. Reject unexpected fields rather
    # than accidentally accepting credentials or data that this sample ignores.
    if public_config or not isinstance(value, dict) or set(value) - {"query", "cursor", "limit"}:
        raise PluginError("invalid_configuration")

    # Keep parsing defensive even though the host validates its own input Schema.
    # This makes the handler easier to run and understand as an ordinary function.
    query = value.get("query")
    cursor = value.get("cursor")
    limit = value.get("limit", 20)
    if query is not None and (not isinstance(query, str) or len(query) > 200):
        raise PluginError("invalid_configuration")
    if cursor is not None and (not isinstance(cursor, str) or not cursor.isascii() or not cursor.isdecimal() or len(cursor) > 8):
        raise PluginError("invalid_configuration")
    if type(limit) is not int or not 1 <= limit <= 50:
        raise PluginError("invalid_configuration")

    # models.json contains sample display information only. It cannot add models,
    # routes, credentials, price cards or billing behavior to the platform.
    models = json.loads((ROOT / "models.json").read_text(encoding="utf-8"))
    search = (query or "").strip().casefold()
    if search:
        # Search the model ID, display name and provider without changing the rows.
        models = [model for model in models if search in " ".join((model["id"], model["displayName"], model["provider"])).casefold()]

    # The cursor is an offset for this small example. A production plugin could
    # instead use an opaque cursor from its own read-only data source.
    offset = int(cursor) if cursor is not None else 0
    end = offset + limit
    return {"models": models[offset:end], "nextCursor": str(end) if end < len(models) else None}


def main() -> int:
    # Load the exact manifest shipped with the plugin and let the SDK keep reading
    # host requests. Do not print debugging information to stdout: it carries frames.
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    return Plugin(manifest, read_catalog).serve()


if __name__ == "__main__":
    sys.exit(main())
