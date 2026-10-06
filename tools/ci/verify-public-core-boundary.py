#!/usr/bin/env python3
"""Check that a public checkout does not contain enterprise source paths."""

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
        default=Path("docs/public-core-extraction.json"),
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
    groups = manifest.get("enterpriseGroups")
    if not isinstance(groups, list):
        print("boundary manifest enterpriseGroups must be an array", file=sys.stderr)
        return 2

    matches: dict[str, list[str]] = {}
    for group in groups:
        group_id = group.get("id")
        paths = group.get("paths")
        if not isinstance(group_id, str) or not isinstance(paths, list):
            print("every enterprise group needs an id and paths", file=sys.stderr)
            return 2
        matched = {
            path.relative_to(root).as_posix()
            for pattern in paths
            for path in expand(root, pattern)
        }
        matches[group_id] = sorted(matched)

    total = sum(len(paths) for paths in matches.values())
    if args.mode == "inventory":
        for group_id, paths in matches.items():
            print(f"{group_id}: {len(paths)} matched paths")
        print(f"enterprise boundary inventory: {total} matched paths")
        return 0

    if total:
        print("public checkout contains enterprise paths:", file=sys.stderr)
        for group_id, paths in matches.items():
            for path in paths:
                print(f"  {group_id}: {path}", file=sys.stderr)
        return 1
    print("public core boundary: no enterprise paths found")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
