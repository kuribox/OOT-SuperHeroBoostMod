"""Developer step: bundle the companion app into a single executable (PyInstaller).

Works on Windows, macOS and Linux; PyInstaller must run on the target OS.
Needs app/payload (run tools/make_release.py first).

Output:
    Windows  release/SHB-OcarinaTweaks.exe
    macOS    release/SHB-OcarinaTweaks.app (and a Unix executable inside it)
    Linux    release/SHB-OcarinaTweaks
    all      release/SuperHeroBoost-<version>-presets.zip (every app profile, no executables)
"""
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NAME = "SHB-OcarinaTweaks"
DIST = ROOT / "release"
WORK = ROOT / "build" / "pyi"
if not (ROOT / "app" / "payload").exists():
    raise SystemExit("app/payload is missing: run tools/make_release.py on a machine with the game dump first")
WORK.mkdir(parents=True, exist_ok=True)

icon = ROOT / "app" / "assets" / "shb.ico"
args = [
    sys.executable, "-m", "PyInstaller", "--noconfirm", "--clean", "--onefile", "--windowed",
    "--name", NAME,
    "--distpath", str(DIST),
    "--workpath", str(WORK),
    "--specpath", str(WORK),
    "--add-data", f"{ROOT / 'app' / 'payload'}{os.pathsep}payload",
    "--add-data", f"{ROOT / 'app' / 'assets'}{os.pathsep}assets",
    *(["--icon", str(icon)] if sys.platform == "win32" and icon.is_file() else []),
    str(ROOT / "app" / "main.py"),
]
subprocess.run(args, check=True, cwd=ROOT)

if sys.platform == "win32":
    out = DIST / f"{NAME}.exe"
elif sys.platform == "darwin":
    out = DIST / f"{NAME}.app"
else:
    out = DIST / NAME
print("built", out)

# A single zip with every profile of the app, ready for a GameBanana Mods upload (no executables).
subprocess.run([sys.executable, str(ROOT / "tools" / "make_presets.py")], check=True, cwd=ROOT)
