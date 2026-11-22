"""Build a test variant of the game's External font and (optionally) boot the emulator with it.

The game loads `Font2/Scalable/External.Nin_Ounce_NVN2.bfotf.zs` as zstd (game dictionary)
over an 8-byte header and an XOR-obfuscated OTF. The community PlayStation layout replaces
18 charstrings; this tool mixes a chosen subset of them into the vanilla font so the failing
glyph can be found by bisection.

    py312 tools/font_variant.py --set vanilla          # control: the game's own font
    py312 tools/font_variant.py --set all              # the community font unchanged
    py312 tools/font_variant.py --set buttons          # only U+E040..U+E047
    py312 tools/font_variant.py --set list:uniE049,uniE060
    py312 tools/font_variant.py --set buttons --boot    # install and boot, report crash/ok

`--boot` copies the result into the installed mod's romfs and starts fritchii.exe for
`--seconds`, then reports whether the guest aborted (prefetch abort / unmapped) in the log.
"""
from __future__ import annotations

import argparse
import io
import os
import struct
import subprocess
import sys
import time
from pathlib import Path

import zstandard
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
MAGIC = 0x18029A7F
KEY = 0x28501A6           # the layout's header key; the game does not verify it per file
EMU = Path(os.environ.get("SHB_EMU", r"E:\Switch2\Emu\Fritchii-v4.1-Windows-x64\Fritchii-v4.1"))
GAME = Path(os.environ.get("SHB_GAME", "game.nx"))
DICT = ROOT / "build" / "enemy_opts_dicts" / "zs.zsdic"
CACHE = Path(os.environ.get("LOCALAPPDATA", "")) / "SHB-OcarinaTweaks" / "layouts" / "playstation"
REL = "Font2/Scalable/External.Nin_Ounce_NVN2.bfotf.zs"
BUTTONS = [f"uniE{n:03X}" for n in range(0x040, 0x048)]


def decompress(path: Path, d) -> bytes:
    raw = zstandard.ZstdDecompressor(dict_data=d).decompress(path.read_bytes())
    key = struct.unpack_from("<I", raw, 0)[0] ^ MAGIC
    body = bytearray(raw[8:])
    for i in range(0, len(body) - 3, 4):
        struct.pack_into("<I", body, i, struct.unpack_from("<I", body, i)[0] ^ key)
    if body[:4] != b"OTTO":
        raise SystemExit(f"{path}: not an OTF after decode")
    return bytes(body)


def compress(otf: bytes, d, key: int = KEY, use_dict: bool = True) -> bytes:
    size = len(otf)
    w1 = ((size & 0xFF) << 24) | (((size >> 8) & 0xFF) << 16)
    header = struct.pack("<II", (MAGIC ^ key) & 0xFFFFFFFF, (w1 ^ key) & 0xFFFFFFFF)
    body = bytearray(otf)
    for i in range(0, len(body) - 3, 4):
        struct.pack_into("<I", body, i, struct.unpack_from("<I", body, i)[0] ^ key)
    z = zstandard.ZstdCompressor(level=19, dict_data=d) if use_dict else zstandard.ZstdCompressor(level=19)
    return z.compress(header + bytes(body))


def charstrings(font: TTFont):
    cff = font["CFF "].cff
    return cff[cff.fontNames[0]].CharStrings


def build(vset: list[str], vanilla: bytes, ps: bytes, drop: list[str] | None = None) -> tuple[bytes, list[str]]:
    fv = TTFont(io.BytesIO(vanilla))
    fp = TTFont(io.BytesIO(ps))
    cv, cp = charstrings(fv), charstrings(fp)
    changed = []
    for n in vset:
        if n not in cp:
            raise SystemExit(f"{n} is not in the PlayStation font")
        b = cp[n].bytecode
        if cv[n].bytecode != b:
            cv[n].setBytecode(b)
            changed.append(n)
    for t in (drop or []):
        if t in fv:
            del fv[t]
    out = io.BytesIO()
    fv.save(out)
    return out.getvalue(), changed


def install(zs: bytes) -> Path:
    mods = EMU / "user" / "load" / "0400486024DEC000" / "zz-Super-Hero-Boost"
    dst = mods / "romfs" / REL
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(zs)
    return dst


def boot(seconds: float) -> str:
    log = EMU / "user" / "log" / "eden_log.txt"
    if log.exists():
        log.unlink()
    exe = EMU / "fritchii.exe"
    env = dict(os.environ, EDEN_NX2_DEBUG_SERVER="127.0.0.1:6545")
    proc = subprocess.Popen([str(exe), "-g", str(GAME)], cwd=str(EMU), env=env)
    time.sleep(seconds)
    text = log.read_text(errors="replace") if log.exists() else ""
    crashed = ("prefetch abort" in text) or ("ExceptionRaised" in text)
    lines = [l for l in text.splitlines() if "Critical" in l or "ExceptionRaised" in l]
    proc.terminate()
    try:
        proc.wait(10)
    except subprocess.TimeoutExpired:
        proc.kill()
    return ("CRASH" if crashed else "OK") + "\n" + "\n".join(lines[-4:])


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--set", default="all",
                    help="vanilla | all | buttons | list:NAMES (comma separated glyph names)")
    ap.add_argument("--vanilla", default=r"F:\Switch2\Longshot\rc\romfs\Font2\Scalable"
                                        r"\External.Nin_Ounce_NVN2.bfotf.zs")
    ap.add_argument("--ps", default=str(CACHE / REL))
    ap.add_argument("--out", default=None, help="write the .zs here (default: scratch)")
    ap.add_argument("--pad", type=int, default=0, help="append N zero bytes to the OTF (size probe)")
    ap.add_argument("--drop-tables", default="", help="comma list of tables to drop (name,DSIG,GDEF,post,OS/2)")
    ap.add_argument("--no-dict", action="store_true", help="compress without the game dictionary")
    ap.add_argument("--install", action="store_true", help="copy it into the installed mod")
    ap.add_argument("--boot", action="store_true", help="boot the emulator and report crash/ok")
    ap.add_argument("--seconds", type=float, default=25.0)
    args = ap.parse_args()

    d = zstandard.ZstdCompressionDict(DICT.read_bytes())
    vanilla = decompress(Path(args.vanilla), d)
    ps = decompress(Path(args.ps), d)
    names = [n for n in TTFont(io.BytesIO(ps)).getGlyphOrder()]
    if args.set == "vanilla":
        vset = []
    elif args.set == "all":
        vset = names
    elif args.set == "buttons":
        vset = BUTTONS
    elif args.set.startswith("list:"):
        vset = args.set.split(":", 1)[1].split(",")
    else:
        raise SystemExit("bad --set")

    otf, changed = build(vset, vanilla, ps, [t for t in args.drop_tables.split(",") if t])
    if args.pad:
        otf += b"\0" * args.pad
    zs = compress(otf, d, use_dict=not args.no_dict)
    out = Path(args.out) if args.out else Path(os.environ.get("TMPDIR", ".")) / "font_variant.zs"
    out.write_bytes(zs)
    print(f"font {len(otf)} bytes, {len(zs)} zs, changed {len(changed)}: {', '.join(changed)}")
    if args.install:
        print("installed", install(zs))
    if args.boot:
        print(boot(args.seconds))
    return 0


if __name__ == "__main__":
    sys.exit(main())
