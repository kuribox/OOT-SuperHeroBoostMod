"""Developer step: build the companion app.

Two apps live in this repository, and this script builds either or both:

    app/         the current app, in Rust: one executable (the window, and the command line when it gets arguments)
    app-legacy/  the first app, in Python (Tkinter), bundled into one executable with PyInstaller

    python tools/build_app.py            # the current app (Rust)
    python tools/build_app.py --legacy   # the legacy app (Python)
    python tools/build_app.py --all      # both

Both carry `app/payload` (the prebuilt module, the button art, the romfs options): run tools/make_release.py first on
a machine with the game dump, or tools/ci_payload.py in CI. The Rust build needs `cargo` (rustup); the legacy one
needs PyInstaller (`pip install pyinstaller zstandard`) and, on Linux, python3-tk.

Output (release/ is not committed):
    Rust     release/SHB-OcarinaTweaks.exe            (Linux/macOS: release/SHB-OcarinaTweaks)
    Python   release/legacy/SHB-OcarinaTweaks.exe     (macOS: .app, Linux: no extension)
    --legacy and --all also write release/SuperHeroBoost-<version>-presets.zip (every profile, no executables)
"""
from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NAME = "SHB-OcarinaTweaks"
APP = ROOT / "app"                 # the Rust workspace (and the payload, shared by both apps)
LEGACY = ROOT / "app-legacy"       # the Python app
PAYLOAD = APP / "payload"
DIST = ROOT / "release"
EXE = ".exe" if sys.platform == "win32" else ""


def need_payload(strict: bool) -> None:
    if PAYLOAD.exists():
        return
    message = "app/payload is missing: run tools/make_release.py on a machine with the game dump first"
    if strict:
        raise SystemExit(message)
    print(f"warning: {message}; the executable will carry no payload", file=sys.stderr)


def cargo() -> str:
    found = shutil.which("cargo")
    if found:
        return found
    candidate = Path.home() / ".cargo" / "bin" / f"cargo{EXE}"
    if candidate.is_file():
        return str(candidate)
    raise SystemExit("cargo not found: install Rust (https://rustup.rs) to build the current app")


def build_rust() -> Path:
    """The current app: `cargo build --release -p shb-cli`; the window is part of that one executable."""
    need_payload(strict=False)
    subprocess.run([cargo(), "build", "--release", "-p", "shb-cli"], check=True, cwd=APP)
    built = APP / "target" / "release" / f"{NAME}{EXE}"
    if not built.is_file():
        raise SystemExit(f"the build finished but {built} is not there")
    DIST.mkdir(parents=True, exist_ok=True)
    out = DIST / built.name
    shutil.copy2(built, out)
    print(f"built {out}  ({out.stat().st_size / 1_048_576:.1f} MB)")
    return out


def build_legacy() -> Path:
    """The legacy app: the Python sources bundled with PyInstaller."""
    need_payload(strict=True)
    dist = DIST / "legacy"
    work = ROOT / "build" / "pyi"
    work.mkdir(parents=True, exist_ok=True)
    icon = LEGACY / "assets" / "shb.ico"
    args = [
        sys.executable, "-m", "PyInstaller", "--noconfirm", "--clean", "--onefile", "--windowed",
        "--name", NAME,
        "--distpath", str(dist),
        "--workpath", str(work),
        "--specpath", str(work),
        "--add-data", f"{PAYLOAD}{os.pathsep}payload",
        "--add-data", f"{LEGACY / 'assets'}{os.pathsep}assets",
        *(["--icon", str(icon)] if sys.platform == "win32" and icon.is_file() else []),
        str(LEGACY / "main.py"),
    ]
    subprocess.run(args, check=True, cwd=ROOT)
    if sys.platform == "win32":
        out = dist / f"{NAME}.exe"
    elif sys.platform == "darwin":
        out = dist / f"{NAME}.app"
    else:
        out = dist / NAME
    print("built", out)
    # A single zip with every profile of the app, ready for a GameBanana Mods upload (no executables).
    subprocess.run([sys.executable, str(ROOT / "tools" / "make_presets.py")], check=True, cwd=ROOT)
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    which = parser.add_mutually_exclusive_group()
    which.add_argument("--rust", action="store_true", help="the current app (the default)")
    which.add_argument("--legacy", action="store_true", help="the legacy Python app")
    which.add_argument("--all", action="store_true", help="both")
    args = parser.parse_args()
    if args.all or not args.legacy:
        build_rust()
    if args.all or args.legacy:
        build_legacy()
    return 0


if __name__ == "__main__":
    sys.exit(main())
