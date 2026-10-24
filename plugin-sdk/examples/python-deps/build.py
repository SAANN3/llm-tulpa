"""Runs once when the plugin is installed: a virtual environment inside the plugin's folder, with its packages in it. `build` in
plugin.toml has no shell, so a command that is two steps is a script."""

import subprocess
import sys
from pathlib import Path

folder = Path(__file__).resolve().parent
subprocess.run([sys.executable, "-m", "venv", str(folder / ".venv")], check=True)
python = folder / ".venv" / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
subprocess.run([str(python), "-m", "pip", "install", "--disable-pip-version-check", "-r", str(folder / "requirements.txt")], check=True)
