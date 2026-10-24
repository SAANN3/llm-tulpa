#!/usr/bin/env python3
"""Copies the `tulpa_plugin` package into a plugin's folder, next to its `main.py`, so that the plugin carries its SDK:

    python3 vendor.py ~/my-plugin

A plugin is shared as a `.tulpa` file or a git repository and runs wherever it is installed, where this SDK is not, so what it
imports has to be in its own folder."""

import shutil
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: python3 vendor.py <the plugin's folder>", file=sys.stderr)
        return 2
    target = Path(sys.argv[1]) / "tulpa_plugin"
    source = Path(__file__).resolve().parent / "tulpa_plugin"
    shutil.rmtree(target, ignore_errors=True)
    shutil.copytree(source, target, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    print(f"copied {source} to {target}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
