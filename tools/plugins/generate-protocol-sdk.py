#!/usr/bin/env python3
"""Export the checked Plugin Protocol schema and SDK metadata.

The Rust contract remains authoritative. CI runs the Rust exporter first, then
this script copies the generated schema into a language-neutral SDK bundle.
It intentionally never builds or executes plugin code.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--schema", type=Path, default=Path("contracts/plugins/v1/schema.json"))
    parser.add_argument("--output", type=Path, default=Path("tools/plugins/sdk/generated"))
    args = parser.parse_args()
    document = json.loads(args.schema.read_text(encoding="utf-8"))
    if document.get("$schema") is None and document.get("title") is None:
        raise SystemExit("protocol schema is missing metadata")
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "plugin-protocol-v1.schema.json").write_text(
        json.dumps(document, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    metadata = {
        "protocolVersion": 1,
        "operations": ["manifest", "health", "invoke_capability", "invoke_task", "invoke_provider", "invoke_credential", "plugin_route"],
        "resultKinds": {"invoke_capability": "capability", "plugin_route": "capability", "invoke_task": "task", "invoke_provider": "provider", "invoke_credential": "credential"},
        "frameLimitBytes": 1_048_576,
        "schemaSha256": hashlib.sha256(json.dumps(document, ensure_ascii=False, separators=(",", ":")).encode("utf-8")).hexdigest(),
        "sdkSources": {
            "typescript": "typescript/anyflows-plugin.ts",
            "go": "go/anyflows_plugin.go",
            "rust": "rust/anyflows_plugin.rs",
            "python": "python/anyflows_plugin.py",
        },
    }
    (args.output / "metadata.json").write_text(
        json.dumps(metadata, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
