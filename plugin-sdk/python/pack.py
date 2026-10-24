#!/usr/bin/env python3
"""Packs a plugin's folder into a `.tulpa` file, the form a plugin is shared in:

    python3 pack.py <the plugin's folder> [--out <folder for the file>]

The file is a zip with `plugin.toml` at its top, named `<name>-<subname>-<version>.tulpa` from the manifest. What a build or a run
leaves behind (`target/`, virtual environments, caches, `.git`) is left out. A plugin that says `sdk = "python"` and does not carry
the SDK gets a copy of it in the file (the folder itself is not changed), since the plugin runs where the SDK is not."""

import re
import sys
import zipfile
from pathlib import Path

LEFT_OUT = {"target", ".venv", "venv", "__pycache__", "node_modules", ".git", ".staging"}
SDK = Path(__file__).resolve().parent / "tulpa_plugin"


def manifest_of(folder: Path) -> dict:
    text = (folder / "plugin.toml").read_text()
    try:
        import tomllib
    except ModuleNotFoundError:
        # Python 3.10 has no TOML reader: the three keys the file name needs are found in the text, and the backend does the rest
        return {key: m.group(1) for key in ("name", "subname", "version", "sdk") if (m := re.search(rf'^{key}\s*=\s*"([^"]*)"', text, re.M))}
    return tomllib.loads(text)


def files_of(folder: Path):
    for path in sorted(folder.rglob("*")):
        relative = path.relative_to(folder)
        if path.is_file() and not path.is_symlink() and not LEFT_OUT.intersection(relative.parts) and path.suffix not in (".pyc", ".tulpa") and path.name != ".DS_Store":
            yield path, relative.as_posix()


def main() -> int:
    arguments = sys.argv[1:]
    out = Path.cwd()
    if "--out" in arguments:
        at = arguments.index("--out")
        out = Path(arguments[at + 1])
        del arguments[at : at + 2]
    if len(arguments) != 1 or not (Path(arguments[0]) / "plugin.toml").is_file():
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: python3 pack.py <the plugin's folder, with its plugin.toml> [--out <folder>]", file=sys.stderr)
        return 2
    folder = Path(arguments[0])
    manifest = manifest_of(folder)
    missing = [key for key in ("name", "subname", "version") if not manifest.get(key)]
    if missing:
        print(f"plugin.toml has no {', '.join(missing)}", file=sys.stderr)
        return 1
    target = out / f"{manifest['name']}-{manifest['subname']}-{manifest['version']}.tulpa"
    out.mkdir(parents=True, exist_ok=True)
    count = 0
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as archive:
        for path, name in files_of(folder):
            info = zipfile.ZipInfo.from_file(path, name)
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, path.read_bytes())
            count += 1
        if manifest.get("sdk") == "python" and not (folder / "tulpa_plugin").is_dir():
            for path, name in files_of(SDK):
                archive.writestr(f"tulpa_plugin/{name}", path.read_bytes())
                count += 1
            print("the Python SDK was copied into the file")
    print(f"{target} ({count} files, {target.stat().st_size // 1024} kB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
