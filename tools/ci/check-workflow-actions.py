#!/usr/bin/env python3
"""Reject workflow action references that GitHub cannot resolve during job setup."""

from pathlib import Path
import re
import sys


def main() -> int:
    root = Path(__file__).resolve().parents[2]
    references = re.compile(r"^\s*(?:-\s+)?uses:\s*(.*?)\s*(?:#.*)?$")
    remote_action = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_./-]+)?@[^\s@:#]+")
    errors = []
    for workflow in sorted((root / ".github/workflows").glob("*.y*ml")):
        for number, line in enumerate(workflow.read_text(encoding="utf-8").splitlines(), 1):
            match = references.fullmatch(line)
            if not match:
                continue
            action = match.group(1).strip().strip("'\"")
            if action.startswith(("./", "docker://")) or remote_action.fullmatch(action):
                continue
            errors.append(f"{workflow.relative_to(root)}:{number}: unsupported action reference: {action}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("Workflow action references are compatible with GitHub and Gitea.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
