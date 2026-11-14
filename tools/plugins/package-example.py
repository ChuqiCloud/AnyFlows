#!/usr/bin/env python3
"""Create the unsigned Linux/Python catalogue example package."""

from __future__ import annotations

import argparse
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new .afplugin path")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    with tempfile.TemporaryDirectory(prefix="anyflows-catalog-example-") as temporary:
        distribution = Path(temporary) / "catalog"
        shutil.copytree(root / "examples" / "catalog", distribution, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        shutil.copyfile(root / "sdk" / "python" / "anyflows_plugin.py", distribution / "anyflows_plugin.py")
        return subprocess.run([
            sys.executable, str(root / "package-plugin.py"), str(distribution),
            str(distribution / "manifest.json"), "plugin.py", str(args.output.resolve()),
        ], check=False).returncode


if __name__ == "__main__":
    sys.exit(main())
