"""Copy the built Rust executable into the project's `release/` folder (gitignored, beside the Python release files).

    py -3.12 tools/release_rust.py

It builds nothing: run `cargo build --release -p shb-cli` in `app-rs/` first (or let CI do it). The window is
part of that executable, so there is one file, and it lands in `release/rust/`, which is where the user looks for them and what `.gitignore` keeps out of the
repository. `--out` sends them to another checkout's `release/` (a worktree does not share that folder with the main
checkout, and the one the user opens is the main one).
"""

from __future__ import annotations

import argparse
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "app-rs" / "target" / "release"
OUT = ROOT / "release" / "rust"
BINS = ("SHB-OcarinaTweaks.exe",)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=OUT, help=f"where the binaries go (default {OUT})")
    args = parser.parse_args()
    out = args.out
    missing = [name for name in BINS if not (TARGET / name).is_file()]
    if missing:
        print(f"not built: {', '.join(missing)} (cargo build --release -p shb-cli)")
        return 1
    out.mkdir(parents=True, exist_ok=True)
    for name in BINS:
        shutil.copy2(TARGET / name, out / name)
        size = (out / name).stat().st_size
        print(f"{out / name}  {size / 1_048_576:.1f} MB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
