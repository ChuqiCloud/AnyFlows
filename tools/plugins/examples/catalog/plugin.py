#!/usr/bin/python3
"""Local, read-only catalogue example; no network or credentials are required."""

from __future__ import annotations

import json
from pathlib import Path
import sys

from anyflows_plugin import Plugin, PluginError


ROOT = Path(__file__).resolve().parent


def read_catalog(capability_id: str, public_config: dict, value: object) -> dict:
    if capability_id not in {"catalog.models.read", "catalog.models.search"}:
        raise PluginError("unsupported")
    if public_config or not isinstance(value, dict) or set(value) - {"query", "cursor", "limit"}:
        raise PluginError("invalid_configuration")
    query = value.get("query")
    cursor = value.get("cursor")
    limit = value.get("limit", 20)
    if query is not None and (not isinstance(query, str) or len(query) > 200):
        raise PluginError("invalid_configuration")
    if cursor is not None and (not isinstance(cursor, str) or not cursor.isascii() or not cursor.isdecimal() or len(cursor) > 8):
        raise PluginError("invalid_configuration")
    if type(limit) is not int or not 1 <= limit <= 50:
        raise PluginError("invalid_configuration")
    models = json.loads((ROOT / "models.json").read_text(encoding="utf-8"))
    search = (query or "").strip().casefold()
    if search:
        models = [model for model in models if search in " ".join((model["id"], model["displayName"], model["provider"])).casefold()]
    offset = int(cursor) if cursor is not None else 0
    end = offset + limit
    return {"models": models[offset:end], "nextCursor": str(end) if end < len(models) else None}


def main() -> int:
    manifest = json.loads((ROOT / "manifest.json").read_text(encoding="utf-8"))
    return Plugin(manifest, read_catalog).serve()


if __name__ == "__main__":
    sys.exit(main())
