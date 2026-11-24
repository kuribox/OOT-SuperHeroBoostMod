"""Apply the GameCube artwork to a local copy of the game's controller atlas."""
import base64
import hashlib
import os
import zlib
from pathlib import Path

import zstandard

from gamecube_textures import ATLAS_SIZE, LEGACY_ATLAS_HASHES, TEXTURES
from layouts import REL


def make_atlas(install_root: Path) -> bytes:
    romfs = Path(os.environ["SHB_ROMFS"]).expanduser() if os.environ.get("SHB_ROMFS") else install_root / "romfs"
    source = romfs / REL["atlas"]
    if not source.is_file():
        raise ValueError(f"GameCube needs the original UI archive at {source}. Set SHB_ROMFS to your extracted romfs folder if it is elsewhere.")
    try:
        with source.open("rb") as f, zstandard.ZstdDecompressor().stream_reader(f) as reader:
            atlas = bytearray(reader.read(ATLAS_SIZE + 1))
    except (OSError, zstandard.ZstdError) as ex:
        raise ValueError(f"Could not read the original UI archive at {source}.") from ex
    if len(atlas) != ATLAS_SIZE or atlas[:4] != b"BNTX":
        raise ValueError(f"The UI archive at {source} is not supported by the GameCube artwork.")
    for offset, size, digest, encoded in TEXTURES:
        end = offset + size
        if hashlib.sha256(memoryview(atlas)[offset:end]).hexdigest() != digest:
            raise ValueError(f"The UI archive at {source} has different controller textures. Use the original archive.")
        replacement = zlib.decompress(base64.b85decode(encoded))
        if len(replacement) != size:
            raise ValueError("The bundled GameCube controller artwork is invalid.")
        atlas[offset:end] = replacement
    return zstandard.ZstdCompressor(level=6).compress(atlas)


def write_atlas(romfs: Path, atlas: bytes) -> None:
    path = romfs / REL["atlas"]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(atlas)


def disable_legacy_atlas(mods: Path) -> None:
    """Move a recognized standalone diagram out of the game's resource lookup."""
    path = mods / "GameCube-System-Controller" / "romfs" / REL["atlas"]
    if not path.is_file():
        return
    with path.open("rb") as f:
        digest = hashlib.file_digest(f, "sha256").hexdigest()
    if digest not in LEGACY_ATLAS_HASHES:
        return
    backup = path.with_name(path.name + ".disabled")
    if backup.exists():
        with backup.open("rb") as f:
            if hashlib.file_digest(f, "sha256").hexdigest() != digest:
                raise ValueError(f"Move the standalone GameCube-System-Controller mod out of {mods}; its backup already exists.")
        path.unlink()
    else:
        path.rename(backup)
