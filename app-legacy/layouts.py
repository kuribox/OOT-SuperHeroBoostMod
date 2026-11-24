"""Game-wide controller layouts (PlayStation / Xbox) for Super Hero Boost.

Our own HUD draws its buttons from `UI/Tex/Icon/OotBtn*.bntx`, but the game's own
prompts are not separate files: the button glyphs are glyphs of the External font
and textures inside `Common.Product.100...bntx` (~69 MB). Those archives are mostly
game data, so they are not bundled. The app downloads the community
"Dualsense/XBOX/Steamdeck UI" mod (GameBanana 737074) on demand and caches only the
PlayStation or Xbox set; the Switch layout is the vanilla game and needs nothing.

That mod also ships a ResourceSizeTable that raises the External font entry
(0x3A08 -> 0x5000). A game has a single table, and the mod's copy would drop our
entries (the bigger Common layout and the enemy packs), so its values are merged
into ours entry by entry (max) instead of copied over.
"""
from __future__ import annotations

import json
import os
import shutil
import struct
import sys
import urllib.request
import zipfile
from pathlib import Path

GB_MOD = 737074
GB_PROFILE = f"https://gamebanana.com/apiv11/Mod/{GB_MOD}/ProfilePage"
GB_PAGE = f"https://gamebanana.com/mods/{GB_MOD}"
UA = "SHB-OcarinaTweaks"

# style -> file name in the GameBanana submission ("standard" = Nintendo button
# positions; the "western" variants move the confirm button, so they are skipped)
FILES = {"playstation": "dualsense_standard.zip", "xbox": "xbox_standard.zip"}
PREFIX = {"playstation": "dualsense", "xbox": "xbox"}
REL = {
    "font": "Font2/Scalable/External.Nin_Ounce_NVN2.bfotf.zs",
    "table": "System/Resource/ResourceSizeTable.Product.111.Nin_Ounce_NVN2.rsizetable.zs",
    "atlas": "UI/Layout2Archive/Common.Product.100.Nin_Ounce_NVN2.bntx.zs",
}
MIN_ATLAS = 60_000_000


class LayoutError(Exception):
    """A layout could not be downloaded or read; `key` is a short reason."""

    def __init__(self, key: str, arg: str = ""):
        super().__init__(f"{key}: {arg}")
        self.key, self.arg = key, arg


# ------------------------------------------------------------------ cache
def resource_dir() -> Path:
    """The payload: `payload` inside the built app, `app/payload` of the checkout when run from the sources (the
    payload is shared with the current app, which lives in `app/`)."""
    frozen = getattr(sys, "_MEIPASS", None)
    if frozen:
        return Path(frozen) / "payload"
    return Path(__file__).resolve().parent.parent / "app" / "payload"


def cache_dir() -> Path:
    if sys.platform == "win32":
        base = Path(os.environ.get("LOCALAPPDATA") or Path.home() / "AppData" / "Local")
    elif sys.platform == "darwin":
        base = Path.home() / "Library" / "Caches"
    else:
        base = Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache")
    return base / "SHB-OcarinaTweaks" / "layouts"


def available(style: str) -> bool:
    if style not in FILES:
        return False
    root = cache_dir() / style
    if not (root / "ok").is_file():
        return False
    try:
        return ((root / REL["atlas"]).stat().st_size >= MIN_ATLAS
                and (root / REL["font"]).stat().st_size >= 4096
                and (root / REL["table"]).is_file())
    except OSError:
        return False


def cached_size(style: str) -> int:
    total = 0
    root = cache_dir() / style
    for rel in REL.values():
        try:
            total += (root / rel).stat().st_size
        except OSError:
            pass
    return total


# ------------------------------------------------------------------ download
def _api(timeout: int = 30) -> list:
    req = urllib.request.Request(GB_PROFILE, headers={"User-Agent": UA, "Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            data = json.load(r)
    except Exception as ex:
        raise LayoutError("net", GB_PAGE) from ex
    files = data.get("_aFiles") if isinstance(data, dict) else None
    if not isinstance(files, list):
        raise LayoutError("net", GB_PAGE)
    return files


def _pick(files: list, style: str) -> dict | None:
    want = FILES[style].lower()
    by_name = {(f.get("_sFile") or "").lower(): f for f in files if isinstance(f, dict)}
    if want in by_name:
        return by_name[want]
    for name, f in by_name.items():
        if name.startswith(PREFIX[style]) and "standard" in name and "western" not in name:
            return f
    return None


def _download(url: str, dest: Path, progress=None, timeout: int = 60) -> None:
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Referer": GB_PAGE})
    done = 0
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r, open(dest, "wb") as f:
            total = int(r.headers.get("Content-Length") or 0)
            while True:
                chunk = r.read(262144)
                if not chunk:
                    break
                f.write(chunk)
                done += len(chunk)
                if progress:
                    progress(done, total)
    except Exception as ex:
        try:
            dest.unlink()
        except OSError:
            pass
        raise LayoutError("net", GB_PAGE) from ex


def _extract(zip_path: Path, out: Path) -> None:
    """Extract only the three romfs files, at their canonical paths."""
    found = set()
    try:
        with zipfile.ZipFile(zip_path) as z:
            for member in z.infolist():
                name = member.filename.replace("\\", "/")
                for key, rel in REL.items():
                    if name == rel or name.endswith("/" + rel):
                        dst = out / rel
                        dst.parent.mkdir(parents=True, exist_ok=True)
                        with z.open(member) as src, open(dst, "wb") as f:
                            shutil.copyfileobj(src, f)
                        found.add(key)
    except (zipfile.BadZipFile, OSError) as ex:
        raise LayoutError("zip", "") from ex
    if found != set(REL):
        raise LayoutError("zip", "")
    if (out / REL["atlas"]).stat().st_size < MIN_ATLAS:
        raise LayoutError("zip", "")
    if (out / REL["font"]).stat().st_size < 4096:
        raise LayoutError("zip", "")
    _restbl(_decompress((out / REL["table"]).read_bytes()))   # must be a valid table


def ensure(style: str, progress=None, timeout: int = 60) -> Path:
    """Return the cached folder for `style`, downloading it from GameBanana once."""
    if style not in FILES:
        raise LayoutError("style", style)
    out = cache_dir() / style
    if available(style):
        return out
    entry = _pick(_api(timeout), style)
    if entry is None:
        raise LayoutError("notfound", FILES[style])
    url = entry.get("_sDownloadUrl") or f"https://gamebanana.com/dl/{entry.get('_idRow')}"
    out.mkdir(parents=True, exist_ok=True)
    part = out / (FILES[style] + ".part")
    _download(url, part, progress, timeout)
    try:
        _extract(part, out)
    finally:
        try:
            part.unlink()
        except OSError:
            pass
    (out / "ok").write_text(json.dumps({
        "style": style,
        "file": entry.get("_sFile"),
        "id": entry.get("_idRow"),
        "date": entry.get("_tsDateAdded"),
        "bytes": entry.get("_nFilesize"),
        "page": GB_PAGE,
    }, indent=1), encoding="utf-8")
    return out


# ------------------------------------------------------------------ restbl
def _decompress(data: bytes) -> bytes:
    import zstandard
    try:
        return zstandard.ZstdDecompressor().decompressobj().decompress(data)
    except Exception as ex:
        raise LayoutError("zip", "") from ex


def _restbl(data: bytes):
    if data[:6] != b"RESTBL":
        raise LayoutError("zip", "")
    ver, sbs, ncrc, nname = struct.unpack_from("<IIII", data, 6)
    p = 0x16
    crc = {}
    for _ in range(ncrc):
        key, size = struct.unpack_from("<II", data, p)
        crc[key] = size
        p += 8
    names = {}
    for _ in range(nname):
        name = data[p:p + sbs].split(b"\0")[0].decode("latin1")
        names[name] = struct.unpack_from("<I", data, p + sbs)[0]
        p += sbs + 4
    return ver, sbs, crc, names


def _dump_restbl(ver: int, sbs: int, crc: dict, names: dict) -> bytes:
    out = bytearray(b"RESTBL")
    out += struct.pack("<IIII", ver, sbs, len(crc), len(names))
    for key in sorted(crc):
        out += struct.pack("<II", key, crc[key])
    for name in sorted(names):
        out += name.encode("latin1").ljust(sbs, b"\0") + struct.pack("<I", names[name])
    return bytes(out)


def merge_table(src: Path, dst: Path) -> None:
    """Merge `src` into `dst` (both .zs RESTBL), keeping the biggest size per entry."""
    if not dst.exists():
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
        return
    import zstandard
    _, _, crc, names = _restbl(_decompress(dst.read_bytes()))
    v2, s2, crc2, names2 = _restbl(_decompress(src.read_bytes()))
    for key, size in crc2.items():
        crc[key] = max(crc.get(key, 0), size)
    for name, size in names2.items():
        names[name] = max(names.get(name, 0), size)
    dst.write_bytes(zstandard.ZstdCompressor(level=19).compress(_dump_restbl(v2, s2, crc, names)))


def apply(romfs: Path, style: str) -> None:
    """Copy the chosen layout's atlas into a mod's romfs, with the patched font, and merge its table.

    The layout's own External font is never installed: this build's font loader aborts when the
    unpacked OTF is larger than ~14640 bytes (the layout ships 15632). `tools/make_button_font.py`
    builds the safe one (the vanilla font plus the eight button glyphs), and it is installed from
    the payload when it exists; without it the game keeps its own prompts.
    """
    src = ensure(style)
    rel = REL["atlas"]
    dst = romfs / rel
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src / rel, dst)
    font = resource_dir() / "button_romfs" / style / REL["font"]
    if font.is_file():
        dst = romfs / REL["font"]
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(font, dst)
    merge_table(src / REL["table"], romfs / REL["table"])


def font_ready(style: str) -> bool:
    """The payload has the patched font for `style` (only PlayStation needs one)."""
    return style == "playstation" and (resource_dir() / "button_romfs" / style / REL["font"]).is_file()
