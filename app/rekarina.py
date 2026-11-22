"""Rekarina install logic for the companion app.

Rekarina is a static recompiler for the Switch 2 build of the game: `Rekarina.exe` (built from the game's
ExeFS) loads the ExeFS modules and runs code translated to C, and it takes *romfs* mods at run time. Mods that
carry code (`exefs/subsdk*`) are rejected by its launcher ("the game has no loader for it"), and the lift it runs
only knows `main,subsdk0,sdk`. This module installs the mod without editing Rekarina:

  * the module (built for Rekarina's layout, with the synthetic function-seed table its lifter reads) is added to
    the lift as `subsdk9`, and the generated C is compiled with Rekarina's own tools (`gen.py`, `build_game.py`);
  * the `.ips` of the mod is converted by Rekarina's own `tools/exefs_patches.py`, and the official patch table
    plus every other mod's code patches are kept, so their code still works;
  * only *game data* is written: `Rekarina.exe` (backed up), `exefs/subsdk9`, `mods/zz-Super-Hero-Boost/romfs`,
    `build-cache/*.status`, `rekarina.ini` (mod order) and `.shb-rekarina/` (lift, objects, backups);
  * the settings live in the module's config block. Applying them patches `exefs/subsdk9` and the expected SHA-256
    that `Rekarina.exe` stores for it, so no rebuild is needed for a settings change;
  * uninstall restores everything from `.shb-rekarina/backup`.

All errors are `installer.InstallError(key, arg)` so the window can translate them.
"""
from __future__ import annotations

import hashlib
import json
import mmap
import os
import re
import shutil
import struct
import subprocess
import sys
import threading
from dataclasses import dataclass, field
from pathlib import Path

import installer as I

MOD_NAME = I.MOD_NAME
WORK = ".shb-rekarina"
BACKUP = WORK + "/backup"
QUARANTINE = WORK + "/quarantine"
EXE_NAME = "Rekarina.exe"
INI = "rekarina.ini"
MODULE = "subsdk9"
TITLE = "0400486024DEC000"          # the game's title id (Atmosphere layouts)
GAME = "1.1.1"                      # the game version this module is built for
MODULE_BUILD = "e0f3ccfa"           # main's NSO module id (first 8 hex characters)
BUILD_ID = "E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348"   # the full id: an IPS file is for main when its name says so
# The module is linked at main+0x5200000 (Rekarina's layout: main, subsdk0, subsdk9, sdk). The base is checked
# against the real module sizes before building, so a different game version fails instead of booting into nothing.
EXPECTED_BASE = 0x5200000
# The five input `bl` sites (and the four equip sites) that identify our IPS. IPS offsets count main's 0x100 header.
INPUT_SITES = (0x34695F8, 0x3469624, 0x3469650, 0x346976C, 0x3469798,
               0x33A937C, 0x33AAD34, 0x33BDA28, 0x33C9BB4)
EXEFS_PLACES = ("exefs", "romfs", "cheats")   # (romfs is not a code place; kept for completeness)
EXEFS_ROOTS = ("exefs", f"atmosphere/contents/{TITLE}/exefs", f"contents/{TITLE}/exefs", f"{TITLE}/exefs")
CHECKAT_ROOTS = ("cheats", f"atmosphere/contents/{TITLE}/cheats", f"contents/{TITLE}/cheats", f"{TITLE}/cheats")
STAGE_RANGE = {"read": (0.00, 0.03), "lift": (0.03, 0.16), "compile": (0.16, 0.98)}
NINJA_LINE = re.compile(r"^\[(\d+)/(\d+)\]")

_proc: subprocess.Popen | None = None
_cancel = threading.Event()


@dataclass
class Result:
    target: Path
    rebuilt: bool
    notes: list = field(default_factory=list)


# --------------------------------------------------------------------------- paths / detection

def find_game(path) -> Path | None:
    """The game folder (`Rekarina.exe`, `rekarina.ini`, `exefs/main`). Accepts the install root or `.../game`."""
    if not path:
        return None
    p = Path(path).expanduser()
    if not p.exists():
        return None
    if p.is_file():
        p = p.parent
    cands = [p]
    if p.name.lower() == "game":
        cands.append(p.parent)
    else:
        cands.append(p / "game")
    for c in cands:
        if (c / INI).is_file() and (c / "exefs" / "main").is_file() and (c / EXE_NAME).is_file():
            return c
    for c in cands:
        if (c / INI).is_file() and (c / "exefs" / "main").is_file():
            return c
    return None


def find_setup(game: Path | None) -> Path | None:
    """The Rekarina release folder that holds the tools we drive (`setup/<version>`)."""
    if game is None:
        return None
    roots = [game.parent, game.parent.parent]
    found = []
    for root in roots:
        setup = root / "setup"
        if not setup.is_dir():
            continue
        try:
            for d in setup.iterdir():
                if (d / "installer" / "oot.py").is_file() and (d / "recomp" / "gen.py").is_file() \
                        and (d / "tools" / "build_game.py").is_file():
                    found.append(d)
        except OSError:
            continue
    if not found:
        return None

    def key(p: Path):
        return tuple(int(n) for n in re.findall(r"\d+", p.name)) or (0,)

    return sorted(found, key=key)[-1]


def launcher_of(game: Path) -> Path | None:
    for name in (EXE_NAME, "rekarina"):
        p = game / name
        if p.is_file():
            return p
    return None


def game_running(game: Path | None = None) -> bool:
    """A copy of Rekarina (or any exe in the game folder) is running."""
    try:
        import recomp as R
        if game is not None and R._game_processes(game):
            return True
    except Exception:
        pass
    if os.name != "nt":
        try:
            out = subprocess.run(["pgrep", "-fl", "rekarina"], capture_output=True, text=True, timeout=5).stdout
            return bool(out.strip())
        except (OSError, subprocess.SubprocessError):
            return False
    return False


def _kill_running(game: Path, log=None) -> None:
    try:
        import recomp as R
        R._kill_game_copies(game, log)
    except Exception:
        pass


# --------------------------------------------------------------------------- backup / quarantine

def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def _read_json(path: Path) -> dict:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def _manifest(game: Path) -> tuple[Path, dict]:
    bak = game / BACKUP
    bak.mkdir(parents=True, exist_ok=True)
    man = _read_json(bak / "manifest.json")
    if man.get("version") != 4 or man.get("kind") != "rekarina":
        man = {"version": 4, "kind": "rekarina", "files": []}
    return bak, man


def _remove_mod_order(game: Path) -> None:
    """Take our mod out of `mods_order` / `mods_disabled` without touching the player's other settings."""
    ini = game / INI
    try:
        text = ini.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return
    lines = text.splitlines()
    changed = False
    for i, line in enumerate(lines):
        low = line.strip().lower()
        if (low.startswith("mods_order") or low.startswith("mods_disabled")) and "=" in line:
            cur = line.split("=", 1)[1]
            names = [n.strip() for n in cur.split("|") if n.strip() and n.strip() != MOD_NAME]
            new = line.split("=", 1)[0] + "= " + "|".join(names)
            if new != line:
                lines[i] = new
                changed = True
    if changed:
        try:
            ini.write_bytes(("\n".join(lines) + "\n").encode("utf-8"))
        except OSError:
            pass


def _backup_entry(game: Path, man: dict, rel: str) -> str:
    """Copy one game-relative path aside (once); return its kind ('file', 'dir' or 'created')."""
    rel = rel.replace("\\", "/").strip("/")
    for e in man.get("files", []):
        if isinstance(e, dict) and e.get("path") == rel:
            return str(e.get("kind") or "file")
    bak = game / BACKUP
    src = game / rel
    kind = "created"
    if src.is_file():
        dst = bak / "files" / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
        kind = "file"
    elif src.is_dir():
        dst = bak / "files" / rel
        if dst.exists():
            shutil.rmtree(dst, ignore_errors=True)
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(src, dst)
        kind = "dir"
    man.setdefault("files", []).append({"path": rel, "kind": kind})
    (bak / "manifest.json").write_text(json.dumps(man, indent=1), encoding="utf-8")
    return kind


def backup_once(game: Path) -> None:
    """Snapshot, once, every game file the install can change or remove."""
    _bak, man = _manifest(game)
    for rel in (EXE_NAME, f"exefs/{MODULE}", INI,
                "build-cache/exefs.status", "build-cache/cheats.status", "build-cache/mods-built.txt",
                f"mods/{MOD_NAME}"):
        _backup_entry(game, man, rel)
    for old in I.our_mod_folders(game / "mods"):   # names used by older app versions
        _backup_entry(game, man, old.relative_to(game).as_posix())
    # Every exefs folder of the mods folder may lose a stale module or our old IPS.
    for mod in _mod_dirs(game):
        for rel in _code_files(mod):
            _backup_entry(game, man, rel.relative_to(game).as_posix())


def _restore_backup(game: Path) -> bool:
    bak = game / BACKUP
    man = _read_json(bak / "manifest.json")
    restored = False
    for e in man.get("files", []):
        if not isinstance(e, dict):
            continue
        rel = str(e.get("path") or "").replace("\\", "/")
        if not rel or rel.startswith("/") or ".." in rel.split("/") or ":" in rel:
            continue
        if rel == INI:
            continue      # the settings are the player's: only our mod's order line is removed (uninstall)
        dst = game / rel
        kind = e.get("kind")
        src = bak / "files" / rel
        try:
            if kind == "file" and src.is_file():
                dst.parent.mkdir(parents=True, exist_ok=True)
                if rel == EXE_NAME:
                    _copy_atomic(src, dst)
                else:
                    shutil.copy2(src, dst)
                restored = True
            elif kind == "dir" and src.is_dir():
                if dst.exists():
                    shutil.rmtree(dst, ignore_errors=True)
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copytree(src, dst)
                restored = True
            elif kind == "created":
                if dst.is_dir():
                    shutil.rmtree(dst, ignore_errors=True)
                    restored = True
                elif dst.is_file():
                    dst.unlink()
                    restored = True
        except OSError:
            pass
    return restored


def _mod_dirs(game: Path) -> list[Path]:
    mods = game / "mods"
    try:
        return [d for d in mods.iterdir() if d.is_dir() and d.name != MOD_NAME] if mods.is_dir() else []
    except OSError:
        return []


def _first_exefs(mod: Path) -> Path | None:
    for rel in EXEFS_ROOTS:
        p = mod / rel
        if p.is_dir():
            return p
    return None


def _code_files(mod: Path) -> list[Path]:
    """The mod's ExeFS patch files and code modules (the ones Rekarina takes as PLUGIN or patch sets)."""
    out: list[Path] = []
    exefs = _first_exefs(mod)
    if exefs:
        for f in sorted(exefs.iterdir()):
            if f.is_file() and (f.suffix.lower() in (".ips", ".pchtxt") or f.name.startswith("subsdk")):
                out.append(f)
    extra = mod / "atmosphere" / "exefs_patches"
    if extra.is_dir():
        for d in sorted(extra.iterdir()):
            if d.is_dir():
                for f in sorted(d.iterdir()):
                    if f.is_file() and f.suffix.lower() in (".ips", ".pchtxt"):
                        out.append(f)
    return out


def _ips_offsets(data: bytes) -> set[int]:
    try:
        if data[:5] == b"PATCH":
            width, end, at, step = 3, b"EOF", 5, 3
        elif data[:5] == b"IPS32":
            width, end, at, step = 4, b"EEOF", 5, 4
        else:
            return set()
        out: set[int] = set()
        while at + width <= len(data) and data[at:at + len(end)] != end:
            off = int.from_bytes(data[at:at + width], "big")
            at += width
            n = int.from_bytes(data[at:at + 2], "big")
            at += 2
            out.add(off)
            if n == 0:
                at += 3
            else:
                at += n
            if at > len(data):
                break
        return out
    except Exception:
        return set()


def _ips_records(data: bytes) -> list[tuple[int, bytes]] | None:
    """(offset, rest of the record) for every record of an IPS / IPS32 file; None when it is not one."""
    if data[:5] == b"PATCH":
        width, end = 3, b"EOF"
    elif data[:5] == b"IPS32":
        width, end = 4, b"EEOF"
    else:
        return None
    out: list[tuple[int, bytes]] = []
    at = 5
    while at + len(end) <= len(data) and data[at:at + len(end)] != end:
        if at + width + 2 > len(data):
            return None
        off = int.from_bytes(data[at:at + width], "big")
        n = int.from_bytes(data[at + width:at + width + 2], "big")
        size = 2 + (3 if n == 0 else n)              # length + data, or 0000 + RLE count + byte
        rest = data[at + width:at + width + size]
        if len(rest) != size:
            return None
        out.append((off, rest))
        at += width + size
    return out


def _write_ips32(records: list[tuple[int, bytes]]) -> bytes:
    return b"IPS32" + b"".join(off.to_bytes(4, "big") + rest for off, rest in records) + b"EEOF"


def split_foreign_ips(game: Path, mods_root: Path | None = None, log=None) -> int:
    """Move the patch records that are not ours out of our mod's IPS, into a sibling file.

    A mod merger puts the ExeFS patches of every merged mod into one IPS. Rekarina's converter takes a patch file as a
    whole: one record it cannot lift (a code change that alters the CPU flags differently from the game, for example)
    makes it reject the file, and our hooks go with it. The foreign records move to `<id>-other-mods.ips`, which the
    converter classifies and skips on its own. Returns the number of records moved."""
    root = Path(mods_root) if mods_root else game / "mods"
    path = root / MOD_NAME / "exefs" / (BUILD_ID + ".ips")
    try:
        recs = _ips_records(path.read_bytes())
        ours = {off for off, _ in (_ips_records(_payload_ips().read_bytes()) or [])}
    except OSError:
        return 0
    if not recs or not ours or not any(off in ours for off, _ in recs):
        return 0
    mine = [r for r in recs if r[0] in ours]
    foreign = [r for r in recs if r[0] not in ours]
    if not foreign:
        return 0
    other = path.with_name(BUILD_ID + "-other-mods.ips")
    keep = []
    try:
        keep = _ips_records(other.read_bytes()) or []
    except OSError:
        pass
    seen = {off for off, _ in keep}
    keep += [r for r in foreign if r[0] not in seen]
    other.write_bytes(_write_ips32(keep))
    path.write_bytes(_write_ips32(mine))
    if log:
        log("Rekarina: %d patch record(s) of other mods moved out of %s into %s (the converter takes a whole file or "
            "none)" % (len(foreign), path.name, other.name))
    return len(foreign)


def _is_our_ips(path: Path) -> bool:
    try:
        return bool(_ips_offsets(path.read_bytes()[:4096]) & set(INPUT_SITES))
    except OSError:
        return False


def _quarantine(game: Path, rel: str, log=None) -> None:
    """Move one stale code file out of a mod (its content is already in the backup)."""
    src = game / rel
    if not src.exists():
        return
    _bak, man = _manifest(game)
    _backup_entry(game, man, rel)
    dst = game / QUARANTINE / rel
    dst.parent.mkdir(parents=True, exist_ok=True)
    try:
        if dst.exists():
            dst.unlink() if dst.is_file() else shutil.rmtree(dst, ignore_errors=True)
        shutil.move(str(src), str(dst))
        if log:
            log("Rekarina: moved %s out of the mods folder (old Super Hero Boost code)" % rel)
    except OSError as ex:
        if log:
            log("Rekarina: could not move %s: %s" % (rel, ex))


def sanitize_mods(game: Path, log=None) -> None:
    """Disable our old code in other mods (the merge report's `0-M` carries an old subsdk9 and IPS).

    Rekarina marks a mod with `exefs/subsdk*` as PLUGIN and switches the whole mod off; and the old IPS branches
    into the old module base, which no longer exists. Moving both out leaves the mod's romfs usable.
    """
    for mod in _mod_dirs(game):
        for f in _code_files(mod):
            rel = f.relative_to(game).as_posix()
            name = f.name.lower()
            if name.startswith("subsdk"):
                try:
                    if b"SHBCFG01" in f.read_bytes()[:0x20000]:
                        _quarantine(game, rel, log)
                except OSError:
                    pass
            elif f.suffix.lower() == ".ips" and _is_our_ips(f):
                _quarantine(game, rel, log)


# --------------------------------------------------------------------------- settings / order

def _patch_bytes(data: bytes, s: I.Settings) -> bytes:
    i = data.find(b"SHBCFG01")
    if i < 0:
        raise I.InstallError("bad_module", "config block not found")
    size = struct.unpack_from("<I", data, i + 12)[0]
    if size < 72 or size > 256 or i + size > len(data):
        size = 76
    return data[:i] + I.patch_config(data[i:i + size], s) + data[i + size:]


def _set_mod_order(game: Path, log=None) -> None:
    """Put our mod first in `mods_order` (Rekarina's top mod wins a file) and clear it from `mods_disabled`."""
    ini = game / INI
    try:
        text = ini.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return
    lines = text.splitlines()
    have = False
    for i, line in enumerate(lines):
        low = line.strip().lower()
        if low.startswith("mods_order") and "=" in line:
            cur = line.split("=", 1)[1]
            names = [n.strip() for n in cur.split("|") if n.strip() and n.strip() != MOD_NAME]
            lines[i] = "mods_order = " + "|".join([MOD_NAME, *names])
            have = True
        elif low.startswith("mods_disabled") and "=" in line:
            cur = line.split("=", 1)[1]
            names = [n.strip() for n in cur.split("|") if n.strip() and n.strip() != MOD_NAME]
            lines[i] = "mods_disabled = " + "|".join(names)
    if not have:
        lines += ["", "[mods]", "mods_order = " + MOD_NAME]
    try:
        ini.write_bytes(("\n".join(lines) + "\n").encode("utf-8"))
        if log:
            log("Rekarina: %s is first in mods_order" % MOD_NAME)
    except OSError:
        pass


# --------------------------------------------------------------------------- romfs

def install_romfs(game: Path, s: I.Settings) -> int:
    """Copy the selected romfs files to `mods/zz-Super-Hero-Boost/romfs` (Rekarina merges the resource size tables
    of every enabled mod at run time, so no table surgery is needed)."""
    pay = I.resource_dir()
    target = game / "mods" / MOD_NAME / "romfs"
    style = s.button_style if s.button_style in I.BUTTON_STYLES else "switch"
    atlas, gamecube_art, _gc_note = I.prepare_gamecube_layout(s, game, pay)
    for old in I.our_mod_folders(game / "mods"):
        if old.name != MOD_NAME:
            shutil.rmtree(old, ignore_errors=True)
    if target.parent.exists():
        shutil.rmtree(target.parent)
    copied = 0

    def copy(src: Path) -> bool:
        nonlocal copied
        if not src.exists():
            return False
        for f in src.rglob("*"):
            if f.is_file():
                dst = target / f.relative_to(src)
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(f, dst)
                copied += 1
        return True

    if s.button_mapping:
        copy(pay / "button_romfs" / style)
        copy(pay / "button_romfs" / "common")
    if gamecube_art is not None:
        copy(gamecube_art)
    if atlas is not None:
        I.gamecube.write_atlas(target, atlas)
        copied += 1
    if s.bm_layout == "gamecube":
        I.gamecube.disable_legacy_atlas(game / "mods")
    drops = (["no_heart_drop"] if s.no_heart_drop else []) + (["no_magic_drop"] if s.no_magic_drop else [])
    if len(drops) == 2:
        drops = ["no_heart_magic_drop"]
    for opt in drops:
        copy(pay / "romfs_opts" / opt)
    for opt, enabled in (("enemy_double", s.enemy_double), ("farore_outdoor", s.farore_outdoor),
                         ("great_fairy", s.great_fairy)):
        if enabled:
            copy(pay / "romfs_opts" / opt)
    I.install_enemy_mods(target, s, pay)
    # Rekarina ships its own PlayStation / Xbox prompt overlays (romfs-patches-*), so the game-wide prompts are
    # not installed as a mod here: a mod file would shadow Rekarina's live prompt switching.
    target.parent.mkdir(parents=True, exist_ok=True)
    # Its runtime turns a mod's code patch set on only when the mod folder holds the ExeFS patch file that the
    # status line names (runtime/mods.c mods_exefs_switch). Keep it beside the romfs: without it the game loads
    # subsdk9 but never branches into it.
    exefs = target.parent / "exefs"
    exefs.mkdir(parents=True, exist_ok=True)
    shutil.copy2(_payload_ips(), exefs / (BUILD_ID + ".ips"))
    (target.parent / ".super-hero-boost").write_text(json.dumps(
        {"variant": "rekarina", "module": MODULE, **s.to_dict()}, indent=1), encoding="utf-8")
    return copied


# --------------------------------------------------------------------------- module / base

def payload_dir() -> Path:
    return I.resource_dir() / "rekarina"


def _payload_module() -> bytes:
    src = payload_dir() / MODULE
    if not src.is_file():
        raise I.InstallError("no_payload", "rekarina")
    return src.read_bytes()


def _payload_ips() -> Path:
    src = payload_dir() / "ours.ips"
    if not src.is_file():
        raise I.InstallError("no_payload", "rekarina")
    return src


def _nso_size(data: bytes) -> int:
    if len(data) < 0x100 or data[:4] != b"NSO0":
        raise I.InstallError("rekarina_no_exefs", "NSO0")
    bss = struct.unpack_from("<I", data, 0x3C)[0]
    _off, mem, dec = struct.unpack_from("<III", data, 0x10 + 2 * 0x10)
    end = mem + dec + bss
    return (end + 0xFFF) & ~0xFFF


def _align(v: int, a: int = 0x200000) -> int:
    return (v + a - 1) & ~(a - 1)


def module_base(game: Path, module: bytes) -> int:
    """Rekarina's base for subsdk9 in this ExeFS (module sizes decide it), relative to main."""
    base = 0x7000000000
    subsdk9 = None
    for name in ("main", "subsdk0", MODULE, "sdk"):
        path = game / "exefs" / name
        if name == MODULE:
            size = _nso_size(module)
        else:
            try:
                with open(path, "rb") as f:
                    size = _nso_size(f.read(0x100))
            except OSError:
                raise I.InstallError("rekarina_no_exefs", str(path)) from None
        if name == MODULE:
            subsdk9 = base
        base = _align(base + size)
    return (subsdk9 or 0) - 0x7000000000


def _check_exefs(game: Path, setup: Path, module: bytes) -> None:
    exefs = game / "exefs"
    for name in ("main", "subsdk0", "sdk"):
        if not (exefs / name).is_file():
            raise I.InstallError("rekarina_no_exefs", str(exefs / name))
    with open(exefs / "main", "rb") as f:
        bid = _module_id(f.read(0x60))
    if bid and not bid.startswith(MODULE_BUILD):
        raise I.InstallError("rekarina_bad_version", bid[:8])
    got = module_base(game, module)
    if got != EXPECTED_BASE:
        raise I.InstallError("rekarina_bad_base",
                             "main+%#x, this game needs main+%#x" % (EXPECTED_BASE, got))
    expected = setup / "tools" / "data" / "expected-oot-v1.1.1.json"
    if not expected.is_file():
        raise I.InstallError("rekarina_no_setup", str(expected))


def _module_id(head: bytes) -> str:
    return head[0x40:0x60].hex() if len(head) >= 0x60 else ""


# --------------------------------------------------------------------------- build

def _python(setup: Path) -> Path:
    for p in (setup / "python" / "python.exe", setup / "python" / "bin" / "python3",
              setup / "python" / "bin" / "python"):
        if p.is_file():
            return p
    return Path(sys.executable)


def _env(setup: Path, pgo: Path | None) -> dict:
    env = dict(os.environ)
    env.update({"PYTHONUNBUFFERED": "1", "PYTHONIOENCODING": "utf-8", "PYTHONUTF8": "1",
                "PYTHONDONTWRITEBYTECODE": "1", "REKARINA_RT_LTO": "1",
                "SHB_REKARINA_SETUP": str(setup)})
    env.pop("REKARINA_PGO_RUNTIME_ONLY", None)
    if pgo is not None:
        env.pop("REKARINA_HOT_OBJS", None)
    else:
        hot = setup / "tools" / "data" / "hot-oot-v111.txt"
        if hot.is_file():
            env["REKARINA_HOT_OBJS"] = str(hot)
    return env


def _cpu() -> str:
    forced = os.environ.get("REKARINA_CPU")
    if forced in ("v2", "v3"):
        return forced
    try:
        if os.name == "nt":
            import ctypes
            return "v3" if ctypes.windll.kernel32.IsProcessorFeaturePresent(40) else "v2"
        with open("/proc/cpuinfo", encoding="ascii", errors="replace") as f:
            return "v3" if " avx2" in f.read() else "v2"
    except (OSError, AttributeError):
        return "v3"


def cancel() -> None:
    _cancel.set()
    p = _proc
    if p is not None and p.poll() is None:
        try:
            if os.name == "nt":
                subprocess.run(["taskkill", "/F", "/T", "/PID", str(p.pid)], capture_output=True,
                               creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            else:
                p.terminate()
        except (OSError, subprocess.SubprocessError):
            pass


def _run(cmd: list, cwd: Path, env: dict, log=None, on_line=None) -> int:
    """Run one of Rekarina's tools, streaming its output. Returns the exit code."""
    global _proc
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0) if os.name == "nt" else 0
    try:
        _proc = subprocess.Popen([str(c) for c in cmd], cwd=str(cwd), stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace",
                                 bufsize=1, env=env, creationflags=flags)
    except OSError as ex:
        raise I.InstallError("recomp_no_builder", str(ex)) from None
    tail: list[str] = []
    with _proc:
        for line in _proc.stdout or ():
            line = line.rstrip("\r\n")
            if not line:
                continue
            tail.append(line)
            tail = tail[-40:]
            if on_line is not None:
                on_line(line)
            if log is not None:
                log(line)
            if _cancel.is_set():
                cancel()
                break
        code = _proc.wait()
    _proc = None
    if _cancel.is_set():
        raise I.InstallError("recomp_cancelled")
    if code:
        raise I.InstallError("recomp_build_failed", "\n".join(tail[-10:]))
    return code


def _has_our_patch(path: Path) -> bool:
    """The converted patch table holds a set for our IPS (the converter names the mod in its comment)."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False
    return ("# %s/exefs/" % MOD_NAME) in text and "set mod_" in text


def _nso_text_size(data: bytes) -> int:
    if len(data) < 0x100 or data[:4] != b"NSO0":
        raise I.InstallError("rekarina_no_exefs", "NSO0")
    _off, _mem, size = struct.unpack_from("<III", data, 0x10)
    return size


def _direct_targets(text: bytes, pc0: int) -> set[int]:
    """Every direct branch target in a .text segment (B, BL, B.cond, CBZ/CBNZ, TBZ/TBNZ)."""
    out: set[int] = set()
    for i in range(0, len(text) - 3, 4):
        w = struct.unpack_from("<I", text, i)[0]
        op = w >> 26
        imm = None
        if op in (0x05, 0x25):                       # B, BL
            imm = w & 0x3FFFFFF
            if imm & 0x2000000:
                imm -= 0x4000000
        elif op in (0x2A, 0x34, 0x35):               # B.cond, CBZ, CBNZ
            imm = w & 0x7FFFF
            if imm & 0x40000:
                imm -= 0x80000
        elif op in (0x36, 0x37):                     # TBZ, TBNZ
            imm = w & 0x3FFF
            if imm & 0x2000:
                imm -= 0x4000
        if imm is not None:
            out.add(pc0 + i + imm * 4)
    return out


def external_main_seeds(game: Path, module: bytes) -> list[int]:
    """Main addresses our module branches to from outside its own text.

    Most are mid-function resume points (a code hook re-runs the original instruction and branches back). Rekarina's
    lifter only dispatches to function starts, so `gen.py` must know them as extra seeds; the wrapper adds them to
    main only, and its own fall-through rule keeps the split invisible to the game's normal flow."""
    try:
        with open(game / "exefs" / "main", "rb") as f:
            main_head = f.read(0x100)
    except OSError:
        return []
    main_lo = 0x7000000000
    main_hi = main_lo + _nso_text_size(main_head)
    text_off, _mem, text_size = struct.unpack_from("<III", module, 0x10)
    text = module[text_off:text_off + text_size]
    mod_lo = main_lo + EXPECTED_BASE
    mod_hi = mod_lo + text_size
    return sorted(t for t in _direct_targets(text, mod_lo) if main_lo <= t < main_hi and not (mod_lo <= t < mod_hi))


def _gen_wrapper(work: Path, setup: Path, seeds: Path) -> Path:
    """A runner for Rekarina's `recomp/gen.py` that adds our extra main seeds (no Rekarina file is edited).

    `gen.py` emits in multiprocessing workers, which re-import the module by name: import it as `gen` with
    `recomp/` on sys.path (the child gets SHB_REKARINA_SETUP from the environment) and keep the work in a
    `__main__` guard so a spawn child does not run it again."""
    path = work / "gen_rekarina.py"
    path.write_text(
        "import os, sys\n"
        "from pathlib import Path\n"
        "_setup = os.environ.get('SHB_REKARINA_SETUP')\n"
        "if _setup:\n"
        "    sys.path.insert(0, str(Path(_setup) / 'recomp'))\n"
        "    sys.path.insert(0, str(Path(_setup) / 'tools'))\n"
        "\n"
        "def _main():\n"
        "    import json\n"
        "    import gen\n"
        "    setup = Path(sys.argv[1]); seeds_file = Path(sys.argv[2]); args = sys.argv[3:]\n"
        "    extra = {int(x) for x in json.loads(seeds_file.read_text())}\n"
        "    orig = gen.Module.seeds\n"
        "    def seeds(self):\n"
        "        out = list(orig(self))\n"
        "        if self.name == 'main':\n"
        "            out += list(extra)\n"
        "        return out\n"
        "    gen.Module.seeds = seeds\n"
        "    sys.argv = ['gen.py', *args]\n"
        "    raise SystemExit(gen.main())\n"
        "\n"
        "if __name__ == '__main__':\n"
        "    _main()\n",
        encoding="utf-8")
    return path


def _build_wrapper(work: Path, setup: Path) -> Path:
    """A small runner for Rekarina's `tools/build_game.py`.

    `build_game.py` adds `-DGAME_NS2 -DGAME_OOT` only when it picks the expected-hash file itself; we pass our own
    table (the release's plus `subsdk9`), so the wrapper adds the same two flags before calling its `main()`.
    Rekarina's file is imported and run unmodified."""
    path = work / "build_game_rekarina.py"
    path.write_text(
        "import importlib.util, sys\n"
        "from pathlib import Path\n"
        "setup = Path(sys.argv[1])\n"
        "args = sys.argv[2:]\n"
        "sys.path.insert(0, str(setup / 'tools'))\n"
        "spec = importlib.util.spec_from_file_location('rek_build_game', setup / 'tools' / 'build_game.py')\n"
        "mod = importlib.util.module_from_spec(spec)\n"
        "sys.modules['rek_build_game'] = mod\n"
        "spec.loader.exec_module(mod)\n"
        "mod.RT_FLAGS.extend(['-DGAME_NS2', '-DGAME_OOT'])\n"
        "mod.GEN_FLAGS.extend(['-DGAME_NS2', '-DGAME_OOT'])\n"
        "sys.argv = ['build_game.py', *args]\n"
        "raise SystemExit(mod.main())\n",
        encoding="utf-8")
    return path


def build(game: Path, setup: Path, s: I.Settings, progress=None, log=None, mods_root: Path | None = None,
          work: Path | None = None, status_dir: Path | None = None, module: bytes | None = None) -> Path:
    """Lift and compile the game with our module; returns the new exe. Only writes under `work` and the status
    files (the game's `build-cache`), never into the setup."""
    global _proc
    _cancel.clear()
    work = Path(work) if work else game / WORK
    work.mkdir(parents=True, exist_ok=True)
    mods_root = Path(mods_root) if mods_root else game / "mods"
    python = _python(setup)
    tools = setup / "tools"
    recomp = setup / "recomp"
    llvm = setup / "llvm"
    ninja = setup / "bin" / ("ninja.exe" if os.name == "nt" else "ninja")
    deps = setup / "runtime" / "deps"
    pgo = setup / "data" / "pgo-oot.profdata"
    pgo = pgo if pgo.is_file() else None
    base_patches = recomp / "patches-oot-v111.txt"
    expected_src = tools / "data" / "expected-oot-v1.1.1.json"
    for p in (python, tools / "build_game.py", recomp / "gen.py", base_patches, expected_src, llvm / "bin" / "clang.exe",
              ninja, deps / "include"):
        if not p.exists():
            raise I.InstallError("rekarina_no_setup", str(p))
    stage = ["read"]
    frac = [0.0]

    def report(text="", st=None):
        if st:
            stage[0] = st
        if progress:
            progress(min(1.0, max(0.0, frac[0])), text, stage[0])

    def set_stage(name):
        stage[0] = name
        frac[0] = STAGE_RANGE.get(name, (frac[0], frac[0]))[0]
        report("", name)

    def line_log(msg):
        if log:
            log(msg)

    if module is None:
        module = _payload_module()
    _check_exefs(game, setup, module)
    set_stage("read")
    report("staging the ExeFS")
    exefs = work / "exefs"
    if exefs.exists():
        shutil.rmtree(exefs, ignore_errors=True)
    exefs.mkdir(parents=True, exist_ok=True)
    for name in ("main", "main.npdm", "rtld", "sdk", "subsdk0"):
        src = game / "exefs" / name
        if src.is_file():
            shutil.copy2(src, exefs / name)
    (exefs / MODULE).write_bytes(module)
    gen = work / "gen"
    out = work / "build"
    frac[0] = STAGE_RANGE["read"][1]
    report("", "lift")

    env = _env(setup, pgo)
    status_dir = Path(status_dir) if status_dir else game / "build-cache"
    status_exefs = status_dir / "exefs.status"
    status_cheats = status_dir / "cheats.status"
    status_dir.mkdir(parents=True, exist_ok=True)
    patches = base_patches
    # The mods' cheats and ExeFS patches become patch sets beside the port's own, as Rekarina's Lift does.
    try:
        split_foreign_ips(game, mods_root, log)
    except OSError as ex:
        line_log("Rekarina: could not separate the other mods' patches from ours: %s" % ex)
    inputs = [m for m in mods_root.iterdir() if m.is_dir() and (_first_exefs(m) or (m / "cheats").is_dir())] \
        if mods_root.is_dir() else []
    if inputs:
        try:
            out_cheats = work / "patches-cheats.txt"
            _run([python, "-B", tools / "cheats.py", "--exefs", exefs, "--mods", mods_root,
                  "--patches", patches, "--out", out_cheats, "--status", status_cheats],
                 setup, env, log=line_log)
            patches = out_cheats
        except I.InstallError as ex:
            if ex.key == "recomp_cancelled":
                raise
            status_cheats.unlink(missing_ok=True)
            line_log("Rekarina: the cheats could not be converted; continuing without them")
            patches = base_patches
        try:
            out_exefs = work / "patches-exefs.txt"
            why: list[str] = []
            _run([python, "-B", tools / "exefs_patches.py", "--exefs", exefs, "--mods", mods_root,
                  "--patches", patches, "--out", out_exefs, "--status", status_exefs],
                 setup, env, log=line_log,
                 on_line=lambda ln: why.append(ln) if ln.startswith("exefs: unsupported") else None)
            patches = out_exefs
            if not _has_our_patch(out_exefs):
                raise I.InstallError("rekarina_patch_failed", "\n".join(why[:3]))
        except I.InstallError as ex:
            status_exefs.unlink(missing_ok=True)
            if ex.key == "recomp_cancelled":
                raise
            raise I.InstallError("rekarina_patch_failed", ex.arg) from None
    else:
        status_exefs.unlink(missing_ok=True)
        status_cheats.unlink(missing_ok=True)

    set_stage("lift")
    report("translating the game")
    modules = "main,subsdk0,sdk," + MODULE
    extra = external_main_seeds(game, module)
    seeds_file = work / "main_extra_seeds.json"
    seeds_file.write_text(json.dumps(extra), encoding="utf-8")
    gen_runner = _gen_wrapper(work, setup, seeds_file)
    line_log("Rekarina: %d extra main seeds (mid-function resume points of the hooks)" % len(extra))

    def gen_line(line):
        low = line.lower()
        if "emitting" in low or "emitted" in low:
            frac[0] = STAGE_RANGE["lift"][0] + (STAGE_RANGE["lift"][1] - STAGE_RANGE["lift"][0]) * 0.7
        elif re.match(r"^\S+: \d+ functions", line):
            frac[0] = STAGE_RANGE["lift"][0] + (STAGE_RANGE["lift"][1] - STAGE_RANGE["lift"][0]) * 0.5
        report()

    _run([python, "-B", gen_runner, setup, seeds_file, "--exefs", exefs, "--out", gen, "--modules", modules,
          "--patches", patches], setup, env, log=line_log, on_line=gen_line)

    # The layout the lifter wrote must agree with the base the module was linked for.
    try:
        lay = json.loads((gen / "layout.json").read_text())
        got = int(next(m["base"] for m in lay if m["name"] == MODULE), 16) - 0x7000000000
        if got != EXPECTED_BASE:
            raise I.InstallError("rekarina_bad_base",
                                 "main+%#x, the lifter placed it at main+%#x" % (EXPECTED_BASE, got))
    except (OSError, ValueError, StopIteration, KeyError):
        pass

    # The expected hashes: the release's own table plus our module (the loader checks it at run time).
    expected = json.loads(expected_src.read_text(encoding="utf-8"))
    hashes = expected.setdefault("exefs_sha256", {})
    hashes[MODULE] = _sha256_bytes(module)
    work_expected = work / "expected.json"
    work_expected.write_text(json.dumps(expected, indent=1), encoding="utf-8")

    set_stage("compile")
    report("compiling")
    runner = _build_wrapper(work, setup)
    cmd = [python, "-B", runner, setup, "--version", "oot", "--gen", gen, "--out", out,
           "--toolchain", llvm, "--ninja", ninja, "--deps", deps, "--exefs", game / "exefs",
           "--no-romfs", "--expected", work_expected]
    if pgo is not None:
        cmd += ["--pgo", pgo]
    if _cpu() == "v2":
        cmd += ["--cpu", "v2"]

    def build_line(line):
        m = NINJA_LINE.match(line.strip())
        if m:
            done, total = int(m.group(1)), int(m.group(2))
            lo, hi = STAGE_RANGE["compile"]
            frac[0] = lo + (hi - lo) * (done / max(1, total))
            report()

    report()
    _run(cmd, setup, env, log=line_log, on_line=build_line)
    exe = out / ("rekarina.exe" if os.name == "nt" else "rekarina")
    if not exe.is_file():
        raise I.InstallError("recomp_build_failed", "rekarina.exe was not produced")
    frac[0] = 1.0
    report("done", "done")
    return exe


# --------------------------------------------------------------------------- settings in the built game

def _copy_atomic(src: Path, dst: Path) -> None:
    """Copy over a file through a temporary name, so an interrupted write cannot leave a half exe."""
    tmp = dst.with_name(dst.name + ".shb-tmp")
    try:
        shutil.copy2(src, tmp)
        os.replace(tmp, dst)
    finally:
        try:
            if tmp.exists():
                tmp.unlink()
        except OSError:
            pass


def _patch_exe_hash(src_exe: Path, dst_exe: Path, old: str, new: str) -> int:
    """Patch the expected SHA-256 in a copy of src_exe and replace dst_exe with it (atomic)."""
    tmp = dst_exe.with_name(dst_exe.name + ".shb-tmp")
    try:
        shutil.copy2(src_exe, tmp)
        n = _replace_expected_hash(tmp, old, new)
        if not n:
            return 0
        os.replace(tmp, dst_exe)
        return n
    finally:
        try:
            if tmp.exists():
                tmp.unlink()
        except OSError:
            pass


def _replace_expected_hash(exe: Path, old: str, new: str) -> int:
    """Replace the expected SHA-256 of our module in the exe (the loader compares it with the file)."""
    old_b, new_b = old.encode("ascii"), new.encode("ascii")
    n = 0
    with open(exe, "r+b") as f:
        with mmap.mmap(f.fileno(), 0) as mm:
            at = 0
            while True:
                i = mm.find(old_b, at)
                if i < 0:
                    break
                mm[i:i + 64] = new_b
                at = i + 64
                n += 1
            if n:
                mm.flush()
    return n


def _state_path(game: Path) -> Path:
    return game / WORK / "state.json"


def _read_state(game: Path) -> dict:
    return _read_json(_state_path(game))


def _write_state(game: Path, exe_sha: str, module_sha: str, setup: Path, rebuilt: bool,
                 ips_sha: str | None = None) -> None:
    st = _read_state(game)
    st.update({"built": True, "release": _release(setup), "game": GAME,
               "exe_sha256": exe_sha, "module_sha256": module_sha, "rebuilt": bool(rebuilt)})
    if ips_sha is not None:
        st["ips_sha256"] = ips_sha
    try:
        _state_path(game).write_text(json.dumps(st, indent=1), encoding="utf-8")
    except OSError:
        pass


def _release(setup: Path) -> str:
    try:
        return (setup / "VERSION").read_text(encoding="utf-8").strip()
    except OSError:
        return setup.name


_built_cache: dict = {}


def is_built(game: Path) -> bool:
    """The game's exe was built with our module and its expected hash matches the file."""
    exe = game / EXE_NAME
    mod = game / "exefs" / MODULE
    if not exe.is_file() or not mod.is_file():
        return False
    try:
        key = (str(game), exe.stat().st_mtime_ns, exe.stat().st_size, mod.stat().st_mtime_ns, mod.stat().st_size)
    except OSError:
        return False
    if key in _built_cache:
        return _built_cache[key]
    st = _read_state(game)
    ok = False
    if st.get("built") and st.get("module_sha256"):
        try:
            ok = _sha256(exe) == st.get("exe_sha256") and _sha256(mod) == st.get("module_sha256")
            if ok:
                # the exe must know exactly this module (a Rekarina rebuild would not)
                want = str(st.get("module_sha256")).encode("ascii")
                with open(exe, "rb") as f:
                    with mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as mm:
                        ok = mm.find(want) >= 0
        except (OSError, ValueError):
            ok = False
    _built_cache.clear()
    _built_cache[key] = ok
    return ok


def apply_settings(game_path, s: I.Settings) -> Result:
    """Patch the config block of the module and the exe's expected hash. No rebuild."""
    game = find_game(game_path)
    if game is None:
        raise I.InstallError("rekarina_no_game", str(game_path))
    _kill_running(game)
    if game_running(game):
        raise I.InstallError("recomp_running", str(game))
    if not is_built(game):
        raise I.InstallError("rekarina_needs_build", str(game))
    st = _read_state(game)
    ips_sha = _sha256(_payload_ips())
    if st.get("ips_sha256") and st["ips_sha256"] != ips_sha:
        raise I.InstallError("rekarina_needs_build", str(game))
    backup_once(game)
    mod = game / "exefs" / MODULE
    old_data = mod.read_bytes()
    old_sha = _sha256_bytes(old_data)
    new_data = _patch_bytes(old_data, s)
    new_sha = _sha256_bytes(new_data)
    exe = game / EXE_NAME
    if new_sha != old_sha:
        n = _patch_exe_hash(exe, exe, old_sha, new_sha)
        if not n:
            raise I.InstallError("rekarina_needs_build", str(game))
        mod.write_bytes(new_data)
        _write_state(game, _sha256(exe), new_sha, find_setup(game) or game, False)
    install_romfs(game, s)
    _set_mod_order(game)
    _write_built(game)
    return Result(game, False, [])


# --------------------------------------------------------------------------- install / remove

def install(game_path, s: I.Settings, rebuild: bool = True, builder_path=None, work=None,
            progress=None, log=None) -> Result:
    game = find_game(game_path)
    if game is None:
        raise I.InstallError("rekarina_no_game", str(game_path))
    setup = find_setup(game)
    if setup is None:
        raise I.InstallError("rekarina_no_setup", str(game.parent / "setup"))
    _kill_running(game, log)
    if game_running(game):
        raise I.InstallError("recomp_running", str(game))
    module = _payload_module()
    _check_exefs(game, setup, module)
    backup_once(game)
    sanitize_mods(game, log)
    _set_mod_order(game, log)
    install_romfs(game, s)

    if not rebuild:
        if not is_built(game):
            raise I.InstallError("rekarina_needs_build", str(game))
        return apply_settings(game, s)

    # The mod's own IPS stays in mods/zz-Super-Hero-Boost/exefs: Rekarina's runtime reads it to switch the generated
    # code set on (runtime/mods.c mods_exefs_switch), and tools/exefs_patches.py converted it for the lift.
    exe = build(game, setup, s, progress=progress, log=log, mods_root=game / "mods",
                work=Path(work).expanduser() if work else None)

    # The game must be closed when its exe is replaced.
    _kill_running(game, log)
    if game_running(game):
        raise I.InstallError("recomp_running", str(game))
    dest = game / EXE_NAME

    # First settings: the module's config block and the exe's expected hash of that module.
    raw = module
    patched = _patch_bytes(raw, s)
    old_sha, new_sha = _sha256_bytes(raw), _sha256_bytes(patched)
    if new_sha != old_sha:
        if not _patch_exe_hash(exe, dest, old_sha, new_sha):
            raise I.InstallError("rekarina_needs_build", str(game))
    else:
        _copy_atomic(exe, dest)
    (game / "exefs" / MODULE).write_bytes(patched)

    # What the lift was given: the launch screen shows "waiting for a rebuild" for anything else.
    _write_built(game)
    _write_state(game, _sha256(dest), new_sha, setup, True, _sha256(_payload_ips()))
    if progress:
        progress(1.0, "done", "done")
    return Result(game, True, [])


def is_installed(game_path) -> bool:
    """The game folder holds our module (it was installed, whether or not it was compiled yet)."""
    game = find_game(game_path)
    return bool(game) and (game / "exefs" / MODULE).is_file()


def recompile(game_path, s: I.Settings | None = None, progress=None, log=None, work=None) -> Result:
    """Lift and compile again with whatever is in the game folder: the module that is installed there (not the
    payload), the mods' romfs and their code files. Nothing of the mod is copied, patched or rewritten; only the new
    exe replaces the old one and the status files say what it was built from."""
    game = find_game(game_path)
    if game is None:
        raise I.InstallError("rekarina_no_game", str(game_path))
    setup = find_setup(game)
    if setup is None:
        raise I.InstallError("rekarina_no_setup", str(game.parent / "setup"))
    if not is_installed(game):
        raise I.InstallError("rekarina_needs_install", str(game))
    _kill_running(game, log)
    if game_running(game):
        raise I.InstallError("recomp_running", str(game))
    module = (game / "exefs" / MODULE).read_bytes()
    exe = build(game, setup, s or I.Settings(), progress=progress, log=log, mods_root=game / "mods",
                work=Path(work).expanduser() if work else None, module=module)
    _kill_running(game, log)
    if game_running(game):
        raise I.InstallError("recomp_running", str(game))
    dest = game / EXE_NAME
    _copy_atomic(exe, dest)
    _write_built(game)
    _write_state(game, _sha256(dest), _sha256_bytes(module), setup, True)   # (the IPS hash of the install stays)
    if progress:
        progress(1.0, "done", "done")
    return Result(game, True, [])


def reinstall(game_path, s: I.Settings, builder_path=None, work=None, progress=None, log=None) -> Result:
    """Uninstall (every original file back), then copy everything again and compile."""
    game = find_game(game_path)
    if game is None:
        raise I.InstallError("rekarina_no_game", str(game_path))
    _kill_running(game, log)
    if game_running(game):
        raise I.InstallError("recomp_running", str(game))
    if log:
        log("Removing the mod and restoring the original files")
    uninstall(game)
    if log:
        log("Installing the mod again")
    return install(game, s, rebuild=True, builder_path=builder_path, work=work, progress=progress, log=log)


def _write_built(game: Path) -> None:
    """build-cache/mods-built.txt: `<path under mods/>\\t<sha256>` of the code files this build took."""
    mods = game / "mods"
    out: list[str] = []
    if mods.is_dir():
        for mod in sorted((d for d in mods.iterdir() if d.is_dir()), key=lambda d: d.name):
            for f in _code_files(mod):
                try:
                    out.append("%s\t%s" % (f.relative_to(mods).as_posix(), _sha256(f)))
                except OSError:
                    continue
    path = game / "build-cache" / "mods-built.txt"
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        if out:
            path.write_text("\n".join(out) + "\n", encoding="utf-8")
        else:
            path.unlink(missing_ok=True)
    except OSError:
        pass


def uninstall(game_path) -> bool:
    game = find_game(game_path)
    if game is None:
        return False
    _kill_running(game)
    if game_running(game):
        raise I.InstallError("recomp_running", str(game))
    bak = game / BACKUP
    had = bak.is_dir()
    restored = _restore_backup(game) if had else False
    _remove_mod_order(game)
    for rel in (f"mods/{MOD_NAME}", f"exefs/{MODULE}", WORK):
        p = game / rel
        try:
            if p.is_dir():
                shutil.rmtree(p, ignore_errors=True)
                restored = True
            elif p.is_file():
                p.unlink()
                restored = True
        except OSError:
            pass
    return restored or had
