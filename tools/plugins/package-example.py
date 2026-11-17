#!/usr/bin/env python3
"""Create an unsigned Linux/Python example package with the bundled SDK."""

from __future__ import annotations

import argparse
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


EXAMPLES = {
    "catalog": "catalog",
    "channel-health-event": "channel-health-event",
    "channel-model-mapping-review": "channel-model-mapping-review",
    "channel-monitor-review": "channel-monitor-review",
    "account-balance-snapshot-review": "account-balance-snapshot-review",
    "account-balance-live-review": "account-balance-live-review",
    "merchant-service-review": "merchant-service-review",
    "merchant-service-event": "merchant-service-event",
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new .afplugin path")
    parser.add_argument(
        "--example",
        choices=EXAMPLES,
        default="catalog",
        help="example to package (default: catalog)",
    )
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    with tempfile.TemporaryDirectory(prefix=f"anyflows-{args.example}-example-") as temporary:
        distribution = Path(temporary) / args.example
        shutil.copytree(
            root / "examples" / EXAMPLES[args.example],
            distribution,
            ignore=shutil.ignore_patterns("__pycache__", "*.pyc"),
        )
        shutil.copyfile(root / "sdk" / "python" / "anyflows_plugin.py", distribution / "anyflows_plugin.py")
        return subprocess.run([
            sys.executable, str(root / "package-plugin.py"), str(distribution),
            str(distribution / "manifest.json"), "plugin.py", str(args.output.resolve()),
        ], check=False).returncode


if __name__ == "__main__":
    sys.exit(main())
