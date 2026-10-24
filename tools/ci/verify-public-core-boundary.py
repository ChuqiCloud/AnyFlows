#!/usr/bin/env python3
"""Check that a public checkout does not contain excluded source paths."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def expand(root: Path, pattern: str) -> list[Path]:
    """Expand a file or directory pattern into the files it owns."""
    expanded: set[Path] = set()
    for path in root.glob(pattern):
        if path.is_file():
            expanded.add(path)
        elif path.is_dir():
            expanded.update(candidate for candidate in path.rglob("*") if candidate.is_file())
    return sorted(expanded)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("tools/repository/public-core-export.json"),
    )
    parser.add_argument(
        "--mode",
        choices=("inventory", "public"),
        default="inventory",
        help="inventory reports matched paths; public fails when any are present",
    )
    args = parser.parse_args()
    root = args.root.resolve()
    manifest_path = args.manifest if args.manifest.is_absolute() else root / args.manifest
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"boundary manifest cannot be read: {error}", file=sys.stderr)
        return 2
    if not isinstance(manifest, dict):
        print("boundary manifest must be an object", file=sys.stderr)
        return 2
    patterns = manifest.get("excludedPaths")
    if not isinstance(patterns, list) or not all(
        isinstance(pattern, str) and pattern for pattern in patterns
    ):
        print("boundary manifest excludedPaths must be an array of non-empty strings", file=sys.stderr)
        return 2
    matches = sorted({
        path.relative_to(root).as_posix()
        for pattern in patterns
        for path in expand(root, pattern)
    })

    if args.mode == "inventory":
        for path in matches:
            print(path)
        print(f"source boundary inventory: {len(matches)} matched paths")
        return 0

    if matches:
        print("public checkout contains excluded paths:", file=sys.stderr)
        for path in matches:
            print(f"  {path}", file=sys.stderr)
        return 1
    print("public core boundary: no excluded paths found")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
