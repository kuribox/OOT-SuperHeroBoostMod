"""Developer step: build the PlayStation button font the game can actually load.

A full replacement of the button glyphs in
`Font2/Scalable/External.Nin_Ounce_NVN2.bfotf.zs` (the community layouts ship one). This build's font loader (Owl, inside the
system NSS) aborts with a null call when the unpacked OTF is larger than ~14640 bytes:
the vanilla font is 14556 and 14676 already crashes (measured by bisection with
`tools/font_variant.py --pad`, see docs/RE-NOTES.md). A community layout's font is 15632, so it can
never be shipped as it comes.

This tool keeps the **vanilla** font, swaps only the eight button glyphs U+E040..U+E047
(A B X Y L R ZL ZR) for the layout's art, drops the OpenType `name` table (524 bytes the game
never reads) and checks the result against the budget. Output: 14232 bytes, which boots.

The font is game data (the vanilla half comes from the user's own dump), so the result is
written into `app/payload/` and never committed. `app/layouts.py` installs it instead of the
layout's own font; without it the game keeps its own prompts.

    py312 tools/make_button_font.py                 # playstation, from SHB_ROMFS + the cache
    py312 tools/make_button_font.py --check         # only report sizes and the budget
"""
from __future__ import annotations

import argparse
import io
import os
import struct
import sys
from pathlib import Path

import zstandard
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
MAGIC = 0x18029A7F
KEY = 0x28501A6
BUTTONS = [f"uniE{n:03X}" for n in range(0x040, 0x048)]
REL = "Font2/Scalable/External.Nin_Ounce_NVN2.bfotf.zs"
LIMIT = 14640                     # hard limit of this build's loader (14636 boots, 14676 aborts)
TARGET = 14624                    # leave a little margin
ROMFS = Path(os.environ.get("SHB_ROMFS", r"F:\Switch2\Longshot\rc\romfs"))
DICT = Path(os.environ.get("SHB_ZSDIC", ROOT / "build" / "enemy_opts_dicts" / "zs.zsdic"))
CACHE = Path(os.environ.get("LOCALAPPDATA", "")) / "SHB-OcarinaTweaks" / "layouts"
OUT = ROOT / "app" / "payload" / "button_romfs"


def decode(zs: bytes, dic) -> tuple[bytes, int]:
    raw = zstandard.ZstdDecompressor(dict_data=dic).decompress(zs)
    if len(raw) < 16:
        raise SystemExit("font: short file")
    key = struct.unpack_from("<I", raw, 0)[0] ^ MAGIC
    body = bytearray(raw[8:])
    for i in range(0, len(body) - 3, 4):
        struct.pack_into("<I", body, i, struct.unpack_from("<I", body, i)[0] ^ key)
    if body[:4] != b"OTTO":
        raise SystemExit("font: not an OTF after decode")
    return bytes(body), key


def encode(otf: bytes, key: int) -> bytes:
    size = len(otf)
    w1 = ((size & 0xFF) << 24) | (((size >> 8) & 0xFF) << 16)
    header = struct.pack("<II", (MAGIC ^ key) & 0xFFFFFFFF, (w1 ^ key) & 0xFFFFFFFF)
    body = bytearray(otf)
    for i in range(0, len(body) - 3, 4):
        struct.pack_into("<I", body, i, struct.unpack_from("<I", body, i)[0] ^ key)
    # The game's own decompressor is given its dictionary; a frame that does not reference it
    # decodes fine, and this keeps the app free of the 128 KB dictionary.
    return zstandard.ZstdCompressor(level=19).compress(header + bytes(body))


def charstrings(font: TTFont):
    cff = font["CFF "].cff
    return cff[cff.fontNames[0]].CharStrings


def build(vanilla: bytes, layout: bytes) -> tuple[bytes, list[str]]:
    fv = TTFont(io.BytesIO(vanilla))
    fl = TTFont(io.BytesIO(layout))
    cv, cl = charstrings(fv), charstrings(fl)
    changed = []
    for n in BUTTONS:
        b = cl[n].bytecode
        if b is None:
            cl[n].decompile()
            b = cl[n].bytecode
        if cv[n].bytecode != b:
            cv[n].setBytecode(b)
            changed.append(n)
    if "name" in fv:
        del fv["name"]
    out = io.BytesIO()
    fv.save(out)
    return out.getvalue(), changed


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--style", default="playstation")
    ap.add_argument("--romfs", default=str(ROMFS), help="the game's own romfs (SHB_ROMFS)")
    ap.add_argument("--dict", default=str(DICT))
    ap.add_argument("--layout", default=None, help="the downloaded layout (default: the app cache)")
    ap.add_argument("--out", default=None)
    ap.add_argument("--check", action="store_true", help="report sizes only")
    args = ap.parse_args()

    if args.style != "playstation":
        raise SystemExit("only the playstation style needs a patched font (xbox keeps the game's)")
    vanilla_path = Path(args.romfs) / REL
    layout_path = Path(args.layout) if args.layout else CACHE / args.style / REL
    for p, what in ((vanilla_path, "vanilla font (set SHB_ROMFS to the game's romfs)"),
                    (layout_path, "layout font (download it in the app once)"),
                    (Path(args.dict), "game zstd dictionary (set SHB_ZSDIC)")):
        if not p.is_file():
            raise SystemExit(f"missing {what}: {p}")
    dic = zstandard.ZstdCompressionDict(Path(args.dict).read_bytes())
    vanilla, key = decode(vanilla_path.read_bytes(), dic)
    layout, _ = decode(layout_path.read_bytes(), dic)
    print(f"vanilla {len(vanilla)} | layout {len(layout)} | budget {LIMIT}")
    otf, changed = build(vanilla, layout)
    print(f"patched {len(otf)} bytes, glyphs {len(changed)}: {', '.join(changed)}")
    if len(otf) > TARGET:
        raise SystemExit(f"patched font is {len(otf)} bytes, over the {TARGET} target "
                         f"(the loader aborts past {LIMIT})")
    if args.check:
        return 0
    zs = encode(otf, key)
    out = Path(args.out) if args.out else OUT / args.style / REL
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(zs)
    print(f"wrote {out} ({len(zs)} bytes compressed)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
