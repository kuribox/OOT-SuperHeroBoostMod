"""Fail if a game-dump or generated payload file is tracked by git.

Usage: python tools/check_no_game_data.py

The repository must contain sources and documentation only. Game data
(romfs, BYML, BNTX, SARC, NSO/NRO, etc.), the prebuilt payload and release
binaries belong in a release artifact, never in the git index.

A few files are committed on purpose although their kind is not source: the documentation's screenshots, the apps'
logo and icon, the GameCube artwork (generated from `app-legacy/gamecube_textures.py` by `app/tools/gen_gamecube.py`,
committed so the Rust build and its tests need no Python), and the settings files the legacy app wrote (the fixtures
`app/crates/shb-core/tests/settings_compat.rs` holds the Rust side to). They are listed in ALLOW.
"""
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

BAD_SUFFIXES = {
    ".byml", ".bgyml", ".bntx", ".sarc", ".blarc", ".bfres", ".bfstm", ".bfsar",
    ".bars", ".genvb", ".pack", ".zs", ".nso", ".nro", ".nsp", ".xci", ".nca",
    ".ips", ".bin", ".png",
}
BAD_DIRS = ("romfs/", "exefs/", "stock-exefs/", "app/payload/", "release/", "build/", "data/")
BAD_MAGIC = {
    b"NSO0": "NSO",
    b"BNTX": "BNTX",
    b"SARC": "SARC",
    b"Yaz0": "Yaz0",
    b"FRES": "FRES",
    b"BFSI": "BFSI",
    b"BYML": "BYML",
    b"YBML": "YBML",
}

# Committed on purpose, with what makes each one safe to keep (see the module docstring).
ALLOW = (
    "docs/",                                                   # screenshots and the cover art of the documentation
    "app/assets/",                                             # the current app's logo (the window's header and icon)
    "app-legacy/assets/",                                      # the legacy app's logo and icon
    "app/crates/shb-cli/assets/",                              # the executable's icon
    "app/crates/shb-core/src/gamecube_textures.bin",           # generated artwork, checked by gen_gamecube.py --check
    "app/crates/shb-core/tests/data/",                         # the legacy app's settings files (test fixtures)
)


def git_exe() -> str:
    found = shutil.which("git")
    if found:
        return found
    for candidate in (r"C:\Program Files\Git\cmd\git.exe", r"C:\Program Files (x86)\Git\cmd\git.exe"):
        if Path(candidate).exists():
            return candidate
    raise SystemExit("git not found in PATH")


def allowed(rel: str) -> bool:
    return any(rel == entry or rel.startswith(entry) for entry in ALLOW)


def main() -> int:
    tracked = subprocess.run([git_exe(), "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    bad = []
    for rel in tracked.splitlines():
        rel_norm = rel.replace("\\", "/")
        if allowed(rel_norm):
            continue
        p = ROOT / rel
        why = None
        if p.suffix.lower() in BAD_SUFFIXES:
            why = f"extension {p.suffix}"
        elif any(part in rel_norm for part in BAD_DIRS):
            why = "generated/dump folder"
        elif p.is_file() and p.stat().st_size:
            head = p.read_bytes()[:4]
            if head in BAD_MAGIC:
                why = f"magic {BAD_MAGIC[head]}"
        if why:
            bad.append((rel, why))

    if bad:
        print("Tracked game data / generated files found:")
        for rel, why in bad:
            print(f"  {rel}  ({why})")
        print("\nRemove them from git (git rm --cached <file>) and keep them in .gitignore.")
        return 1
    print(f"OK: {len(tracked.splitlines())} tracked files; no game dump or payload files.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
