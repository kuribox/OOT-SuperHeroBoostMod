"""Hookshot / Longshot install logic for the companion app.

Hookshot and Longshot are static recompilations: the game's code is translated to C and compiled into
`bin/hookshot` / `bin/longshot.exe`, and `image.bin` holds the guest image. An `exefs` folder inside a mod is ignored
("the code is compiled ahead of time"), so the module has to be built into the game. This module:

  * patches `.longshot_exefs/main` and writes `subsdk9` (with the chosen configuration) before a rebuild,
  * copies the selected romfs files to `<game>/mods/zz-Super-Hero-Boost/romfs` (the recompilers' own mod layout),
  * can update the configuration and romfs without rebuilding (the config block lives in `image.bin` too),
  * runs the builder's `nx2/make_game.py --update` with its bundled toolchain, streaming progress,
  * restores the original executable and image from `.shb-backup` on uninstall.

All errors are `InstallError(key, arg)` so the window can translate them.
"""
from __future__ import annotations

import json
import hashlib
import os
import re
import shutil
import struct
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path

import installer as I

MOD_NAME = I.MOD_NAME   # must sort last: the last mod wins a file (see installer.MOD_NAME)
BACKUP = ".shb-backup"
CONFIG_MAGIC = b"SHBCFG01"
CONFIG_SIZE = 76
MODULE_BUILD = "e0f3ccfa"   # the game build the IPS and the module are made for (main's NSO module id)
# Longshot, Hookshot and ReLink are the same kind of static recompilation (an exefs, image.bin, a romfs and a
# launcher beside a game exe in bin/). The folder name and the exe say which one it is.
EXEFS_NAMES = (".longshot_exefs", ".hookshot_exefs", ".relink_exefs")
# The builders produce the same layout on every OS: the native program is `bin/longshot`,
# `bin/hookshot` or `bin/relink-game` (with .exe only on Windows).
EXE_NAMES = (Path("bin") / "longshot.exe", Path("bin") / "longshot",
             Path("bin") / "hookshot.exe", Path("bin") / "hookshot",
             Path("bin") / "relink-game.exe", Path("bin") / "relink-game")
LAUNCHER_NAMES = ("longshot.exe", "longshot", "hookshot.exe", "hookshot",
                  "ReLink.exe", "ReLink", "relink.exe", "relink", "relink-game")
BUILDER_NAMES = ("LongshotBuilder", "HookshotBuilder", "ReLink")
# Each flavor has its own launcher folder and builder name: never mix them (a ReLink builder cannot build a
# Longshot game, and the settings that decide DLSS/FSR live in the matching launcher).
BUILDER_ROOTS = {"longshot": "LongshotLauncher", "hookshot": "HookshotLauncher", "relink": "ReLink"}
BUILDER_ENV = {"longshot": "LONGSHOT_BUILDER", "hookshot": "HOOKSHOT_BUILDER", "relink": "RELINK_BUILDER"}
# The launchers keep their data in the OS's per-user data folder. Windows: %LOCALAPPDATA%\<name>; Linux:
# $XDG_DATA_HOME/<name> or ~/.local/share/<name>; macOS: ~/Library/Application Support/<name>. The known
# spellings are tried first and a folder whose name contains the flavor's name is accepted as well.
LAUNCHER_KEYS = {"longshot": "longshot", "hookshot": "hookshot", "relink": "relink"}
LAUNCHER_DIR_NAMES = {
    "longshot": ("LongshotLauncher", "Longshot Launcher", "longshot-launcher"),
    "hookshot": ("HookshotLauncher", "Hookshot Launcher", "hookshot-launcher"),
    "relink": ("ReLink", "Relink", "relink", "ReLinkLauncher", "ReLink Launcher", "relink-launcher"),
}


def _data_dirs(platform: str | None = None, home: Path | None = None) -> list[Path]:
    """The per-user data folders a launcher can use on this OS (the ones that exist)."""
    if platform is None:
        platform = "windows" if os.name == "nt" else ("mac" if sys.platform == "darwin" else "linux")
    home = Path(home) if home else Path.home()
    env = os.environ
    out: list[Path] = []
    if platform == "windows":
        for var in ("LOCALAPPDATA", "APPDATA"):
            if env.get(var):
                out.append(Path(env[var]))
    elif platform == "mac":
        out += [home / "Library" / "Application Support", home / "Library" / "Preferences"]
    # XDG (Linux; also a fallback for cross-built launchers on Windows/macOS)
    out.append(Path(env.get("XDG_DATA_HOME") or home / ".local" / "share"))
    out.append(Path(env.get("XDG_CONFIG_HOME") or home / ".config"))
    if platform == "linux":
        out.append(home / ".local" / "state")
    found: list[Path] = []
    for p in out:
        try:
            if p.is_dir() and p not in found:
                found.append(p)
        except OSError:
            pass
    return found


def launcher_dirs(fl: str) -> list[Path]:
    """Every data folder of this flavor's launcher, on Windows, Linux and macOS."""
    names = LAUNCHER_DIR_NAMES[fl]
    key = LAUNCHER_KEYS[fl]
    found: list[Path] = []
    for base in _data_dirs():
        for name in names:
            p = base / name
            if p.is_dir() and p not in found:
                found.append(p)
    if found:
        return found
    # A launcher packaged under another name (or an XDG folder with a different spelling).
    for base in _data_dirs():
        try:
            for p in base.iterdir():
                if not p.is_dir():
                    continue
                low = p.name.lower().replace(" ", "").replace("-", "").replace("_", "")
                if key in low and p not in found:
                    found.append(p)
        except OSError:
            pass
    return found
# Files the builders regenerate on every build: never copied (some are gigabytes) and deleted on uninstall, so a
# stale cache of the modded build cannot meet the restored game.
VOLATILE = (
    "cache/longshot_pipelines.bin", "longshot_pipelines.bin",
    "longshot_shaders.bin", "longshot_pipeline_states_shipped.bin",
    "relink_programs.bin", "relink_programs.idx", "relink_pipelines.bin",
    "relink_pipeline_states.bin", "relink_pipeline_states_shipped.bin",
    "relink_shaders.bin", "relink_computes.bin", "relink_computes_shipped.bin",
    "relink_pipelines_warm.txt",
)
# Folders the builders write (their own dump repair mod).
VOLATILE_DIRS = ("mods/00-dump-repair",)


def flavor(game: Path) -> str:
    """'relink', 'hookshot' or 'longshot', from the exefs folder the game keeps."""
    name = exefs_of(game).name
    if name == ".relink_exefs":
        return "relink"
    if name == ".hookshot_exefs":
        return "hookshot"
    return "longshot"


def _env_prefix(game: Path) -> str:
    return "RELINK" if flavor(game) == "relink" else "LONGSHOT"


def launcher_options(game: Path) -> dict:
    """How the game was built (DLSS, FSR, XeSS), as its launcher remembers it.

    A rebuild that ignores these silently changes the renderer: the DLSS path is compiled out, the ngx runtime is
    removed and the game can stop on a setting it no longer has. ReLink keeps them in launcher.ini; the Longshot and
    Hookshot launchers in settings.json, with the per-folder entry of builds.json. The launcher folder is found on
    Windows, Linux and macOS; if it is not installed, the game's own options file decides DLSS."""
    opt = {"dlss": False, "dlssLicense": False, "fsr": False, "fsrLicense": False,
           "xess": False, "xessLicense": False, "ngx": None, "launcher": None}
    fl = flavor(game)
    roots = launcher_dirs(fl)
    if fl == "relink":
        ini = next((b / "launcher.ini" for b in roots if (b / "launcher.ini").is_file()), None)
        opt["launcher"] = str(ini) if ini else None
        try:
            for line in ini.read_text(encoding="utf-8", errors="replace").splitlines():
                if line.strip().startswith("nvidia_dlss="):
                    opt["dlss"] = line.split("=", 1)[1].strip() == "1"
        except (OSError, AttributeError):
            pass
        base = ini.parent if ini else (roots[0] if roots else None)
        if base is not None:
            opt["ngx"] = base / "ngx-sdk"
    else:
        # The settings that decide DLSS/FSR belong to the launcher that built this flavor (a Hookshot game reads
        # HookshotLauncher; a Longshot game LongshotLauncher).
        base = next((b for b in roots if (b / "settings.json").is_file()), None)
        if base is None and roots:
            base = roots[0]
        if base is not None:
            opt["launcher"] = str(base / "settings.json") if (base / "settings.json").is_file() else None
            try:
                settings = json.loads((base / "settings.json").read_text(encoding="utf-8"))
            except (OSError, ValueError):
                settings = {}
            for key in ("dlss", "dlssLicense", "fsr", "fsrLicense", "xess", "xessLicense"):
                opt[key] = bool(settings.get(key))
            try:
                builds = json.loads((base / "builds.json").read_text(encoding="utf-8")).get("builds", [])
            except (OSError, ValueError):
                builds = []
            for b in builds:
                try:
                    same = Path(str(b.get("folder", ""))).resolve() == game.resolve()
                except OSError:
                    same = False
                if same:
                    for key in ("dlss", "fsr", "xess"):
                        opt[key] = bool(b.get(key, opt[key]))
            opt["ngx"] = base / "ngx-sdk"
    if not opt["ngx"] or not Path(opt["ngx"]).is_dir():
        opt["ngx"] = None
    # Last resort: the game's own options (the launcher may not be installed on this machine).
    if not (opt["dlss"] or opt["fsr"] or opt["xess"]):
        names = ("relink_options.ini",) if fl == "relink" else ("longshot_options.ini", "hookshot_options.ini")
        for name in names:
            ini = game / name
            if not ini.is_file():
                continue
            try:
                for line in ini.read_text(encoding="utf-8", errors="replace").splitlines():
                    k, _, v = line.partition("=")
                    if k.strip().lower() in ("dlss", "nvidia_dlss") and v.strip() == "1":
                        opt["dlss"] = True
            except OSError:
                pass
            break
    return opt


_proc: subprocess.Popen | None = None
_cancel = threading.Event()


@dataclass
class Result:
    target: Path
    rebuilt: bool
    notes: list = field(default_factory=list)


# --------------------------------------------------------------------------- paths / detection

def find_game(path) -> Path | None:
    """The game folder (the one with `.longshot_exefs`/`.hookshot_exefs` and `bin/longshot.exe`)."""
    if not path:
        return None
    p = Path(path).expanduser()
    if not p.exists():
        return None
    if p.is_file():
        p = p.parent
    candidates = [p]
    if p.name.lower() == "bin":
        candidates.append(p.parent)
    for c in candidates:
        if any((c / n / "main").exists() for n in EXEFS_NAMES) and any((c / e).exists() for e in EXE_NAMES):
            return c
    # a folder prepared but not run yet: accept it if the exefs is there
    for c in candidates:
        if any((c / n / "main").exists() for n in EXEFS_NAMES):
            return c
    return None


def exefs_of(game: Path) -> Path:
    for n in EXEFS_NAMES:
        if (game / n / "main").exists():
            return game / n
    raise I.InstallError("recomp_no_game", str(game))


def launcher_of(game: Path) -> Path | None:
    """What opens the game: the launcher beside it (ReLink.exe, longshot.exe...), else the game in bin/."""
    for n in LAUNCHER_NAMES:
        p = game / n
        if p.is_file():
            return p
    return exe_of(game)


def exe_of(game: Path) -> Path | None:
    for e in EXE_NAMES:
        if (game / e).exists():
            return game / e
    return None


def expected_exefs(builder: Path | None) -> str | None:
    """The folder name the builder's --update reads (Longshot keeps `.longshot_exefs`)."""
    if not builder:
        return None
    try:
        txt = (builder / "tools" / "nx2" / "make_game.py").read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    for line in txt.splitlines():
        if "kept" in line and "os.path.join(out" in line:
            for n in EXEFS_NAMES:
                if n in line:
                    return n
    for n in EXEFS_NAMES:
        if '"%s"' % n in txt or "'%s'" % n in txt:
            return n
    return None


def is_builder(path: Path) -> bool:
    return (path / "tools" / "nx2" / "make_game.py").is_file()


def _builder_names(fl: str) -> tuple[str, ...]:
    return ("ReLink",) if fl == "relink" else (("HookshotBuilder",) if fl == "hookshot" else ("LongshotBuilder",))


def _flavors(game: Path | None) -> list[str]:
    """The flavors to look for: the game's own, or every one when there is no game yet (Longshot first)."""
    if game is None:
        return ["longshot", "hookshot", "relink"]
    try:
        return [flavor(game)]
    except I.InstallError:
        return []


def find_builder(game: Path | None = None) -> Path | None:
    """The builder folder (the one with tools/nx2/make_game.py) **for this game**.

    Longshot keeps it in `<launcher>\\toolchains\\<version>\\LongshotBuilder`, Hookshot in the Hookshot launcher and
    ReLink in `<launcher>\\versions\\<version>\\ReLink`. `<launcher>` is the launcher's data folder on this OS:
    `%LOCALAPPDATA%\\<Launcher>` on Windows, `$XDG_DATA_HOME/<Launcher>` or `~/.local/share/<Launcher>` on Linux,
    `~/Library/Application Support/<Launcher>` on macOS. The game's flavor decides which of them is searched, so a
    ReLink or Hookshot builder is never picked for a Longshot game. The matching environment variable wins, then the
    newest folder found, then one beside the game."""
    flavors = _flavors(game)
    for fl in flavors:
        env = os.environ.get(BUILDER_ENV[fl])
        if env and is_builder(Path(env)):
            return Path(env)
    local = Path(os.environ["LOCALAPPDATA"]) if os.environ.get("LOCALAPPDATA") else None
    found: list[Path] = []
    for fl in flavors:
        names = _builder_names(fl)
        roots = launcher_dirs(fl)
        if not roots and local is not None and local.is_dir():   # (kept for old layouts: %LOCALAPPDATA%\<Launcher>)
            roots = [local / BUILDER_ROOTS[fl]]
        for base in roots:
            if not base.is_dir():
                continue
            # toolchains/<version>/<Builder>, versions/<version>/<ReLink>, plus a level for launchers that group
            # by channel or platform.
            patterns = ("toolchains/*", "toolchains/*/*", "versions/*", "versions/*/*", "versions/*/*/*")
            for pat in patterns:
                try:
                    for p in base.glob(pat):
                        if p.name in names and is_builder(p):
                            found.append(p)
                        else:
                            for name in names:
                                q = p / name
                                if q.is_dir() and is_builder(q):
                                    found.append(q)
                except OSError:
                    continue
            for name in names:      # a portable builder directly in the launcher folder
                p = base / name
                if p.is_dir() and is_builder(p):
                    found.append(p)
    if game and flavors:
        fl = flavors[0]
        for rel in ((*_builder_names(fl),), ("..", *_builder_names(fl))):
            p = game.joinpath(*rel)
            if is_builder(p):
                found.append(p)
    if not found:
        return None
    # With a game there is a single flavor: the newest installed version wins. Without one, keep the Longshot,
    # then Hookshot, then ReLink order instead of comparing unrelated paths.
    return sorted(set(found))[-1] if game else found[0]


def find_work(game: Path | None = None) -> Path | None:
    """The build cache folder the launcher of this game uses, so a rebuild is incremental instead of a full
    recompile."""
    fl = None
    if game:
        try:
            fl = flavor(game)
        except I.InstallError:
            fl = None
    for f in ([fl] if fl else ["longshot", "hookshot", "relink"]):
        for base in launcher_dirs(f):
            try:
                w = json.loads((base / "settings.json").read_text(encoding="utf-8")).get("work")
                if w and Path(w).is_dir():
                    return Path(w)
            except (OSError, ValueError):
                pass
            if (base / "work").is_dir():
                return base / "work"
            if f == "relink" and ((base / "build_times.json").is_file() or (base / "objcache").is_dir()):
                return base   # ReLink keeps objcache/, image/, build_times.json in its own folder
    if game:
        # (a `temp` beside the game may hold another copy's cache: reuse it only when it was lifted from this game's
        # exefs, or a rebuild links code translated from other game files)
        for cand in (game.parent / "temp", game / ".shb-work"):
            if cand.is_dir() and _work_matches(cand, game):
                return cand
    return None


def _work_matches(work: Path, game: Path) -> bool:
    """The lift cache in <work>/sa/gen was made from this game's exefs (same main build id), not another copy."""
    try:
        bid = main_build_id(exefs_of(game) / "main")
    except (I.InstallError, OSError):
        return False
    gen = Path(work) / "sa" / "gen" / "hos_image.c"
    try:
        text = gen.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return True      # nothing cached there yet: safe as a fresh work folder
    m = re.search(r'char hos_main_build_id\[\] = "([0-9a-f]+)"', text)
    if not (m and bid):
        return True
    # (main's module id is zero-padded past the build id: the builders call the first 8 characters the build)
    return m.group(1)[:16] == bid[:16]


def payload_dir() -> Path:
    return I.resource_dir()


def payload_variant(game: Path) -> str:
    """The module build for this loader: ReLink needs the input wrappers without the lazy
    PLT check (the static translator turns that check into a tail call and drops the dispatch)."""
    if flavor(game) == "relink" and (payload_dir() / "relink" / "subsdk9").exists():
        return "relink"
    return "standalone"


def game_running() -> bool:
    """The recompiled game (or a builder) is running: the update cannot replace it."""
    names = {"longshot.exe", "longshot", "hookshot.exe", "hookshot",
             "relink-game.exe", "relink-game", "relink", "relink.exe"}
    if os.name == "nt":
        try:
            out = subprocess.run(["tasklist", "/fo", "csv", "/nh"], capture_output=True, text=True,
                                 creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0), timeout=10).stdout
            for line in out.splitlines():
                n = line.split(",")[0].strip('"').lower() if line else ""
                if n in names:
                    return True
        except (OSError, subprocess.SubprocessError):
            return False
    else:
        try:
            out = subprocess.run(["pgrep", "-fl", "longshot|hookshot|relink"], capture_output=True, text=True,
                                 timeout=5).stdout
            return bool(out.strip())
        except (OSError, subprocess.SubprocessError):
            return False
    return False


GAME_EXE_PREFIXES = ("longshot", "hookshot", "relink")


def _game_processes(game: Path) -> list[int]:
    """PIDs running an executable from this game folder (Windows).

    The builder refuses to replace a game that is running from the folder: its root launcher (`longshot.exe`) and
    the old `longshot-game.exe` live directly in it, the game itself is `bin/longshot.exe`. This matches both cases
    (any .exe directly in the folder, plus the game's own names under bin/) and leaves the app and the builder's
    Python alone."""
    if os.name != "nt":
        return []
    try:
        import ctypes
        from ctypes import wintypes
    except ImportError:
        return []
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    root = os.path.normcase(os.path.abspath(game)) + os.sep
    skip = {os.getpid(), os.getppid()}
    if _proc is not None:
        skip.add(_proc.pid)

    class Entry(ctypes.Structure):
        _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD),
                    ("th32ProcessID", wintypes.DWORD), ("th32DefaultHeapID", ctypes.c_size_t),
                    ("th32ModuleID", wintypes.DWORD), ("cntThreads", wintypes.DWORD),
                    ("th32ParentProcessID", wintypes.DWORD), ("pcPriClassBase", ctypes.c_long),
                    ("dwFlags", wintypes.DWORD), ("szExeFile", ctypes.c_wchar * 260)]

    k32.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
    k32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    k32.Process32FirstW.argtypes = [wintypes.HANDLE, ctypes.POINTER(Entry)]
    k32.Process32NextW.argtypes = [wintypes.HANDLE, ctypes.POINTER(Entry)]
    k32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    k32.OpenProcess.restype = wintypes.HANDLE
    k32.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR,
                                               ctypes.POINTER(wintypes.DWORD)]
    k32.CloseHandle.argtypes = [wintypes.HANDLE]
    snap = k32.CreateToolhelp32Snapshot(0x2, 0)
    if not snap or snap == wintypes.HANDLE(-1).value:
        return []
    found: list[int] = []
    try:
        e = Entry()
        e.dwSize = ctypes.sizeof(Entry)
        ok = k32.Process32FirstW(snap, ctypes.byref(e))
        while ok:
            pid = e.th32ProcessID
            if pid not in skip and e.szExeFile.lower().endswith(".exe"):
                h = k32.OpenProcess(0x1000, False, pid)   # PROCESS_QUERY_LIMITED_INFORMATION
                if h:
                    buf, n = ctypes.create_unicode_buffer(1024), wintypes.DWORD(1024)
                    if k32.QueryFullProcessImageNameW(h, 0, buf, ctypes.byref(n)):
                        path = os.path.normcase(buf.value)
                        if path.startswith(root):
                            name = os.path.basename(path)
                            stem = name[:-4] if name.lower().endswith(".exe") else name
                            low = stem.lower()
                            if not low.startswith(("superheroboostconfig", "python")):
                                direct = os.path.dirname(path) + os.sep == root
                                if direct or low.startswith(GAME_EXE_PREFIXES):
                                    found.append(pid)
                    k32.CloseHandle(h)
            ok = k32.Process32NextW(snap, ctypes.byref(e))
    finally:
        k32.CloseHandle(snap)
    return found


def _kill_game_copies(game: Path, log=None) -> list[int]:
    """Close every copy of the game running from this folder, so the builder can replace it (Windows)."""
    if os.name != "nt":
        return []
    pids = _game_processes(game)
    if not pids:
        return []
    try:
        import ctypes
        from ctypes import wintypes
    except ImportError:
        return []
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    k32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    k32.OpenProcess.restype = wintypes.HANDLE
    k32.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
    k32.TerminateProcess.restype = wintypes.BOOL
    k32.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    k32.CloseHandle.argtypes = [wintypes.HANDLE]
    killed: list[int] = []
    for pid in pids:
        h = k32.OpenProcess(0x0001 | 0x00100000, False, pid)   # PROCESS_TERMINATE | SYNCHRONIZE
        if not h:
            continue
        if k32.TerminateProcess(h, 1):
            k32.WaitForSingleObject(h, 5000)
            killed.append(pid)
            if log:
                log("Closed the running game (process %d) so the build can finish" % pid)
        k32.CloseHandle(h)
    if killed:
        time.sleep(0.3)      # let Windows release the exe/dll handles
    return killed


def _watch_games(game: Path, stop: threading.Event, log=None) -> None:
    """Keep closing copies of the game opened while the build runs (the builder would stop at the final swap)."""
    while not stop.wait(1.0):
        try:
            _kill_game_copies(game, log)
        except Exception:
            pass


_built_cache: dict = {}


def is_built(game: Path) -> bool:
    """The executable was rebuilt with the module (it differs from the pre-install backup)."""
    try:
        exefs = exefs_of(game)
    except I.InstallError:
        return False
    mod = exefs / "subsdk9"
    img = game / "image.bin"
    exe = exe_of(game)
    if not mod.exists() or not img.exists() or not exe:
        return False
    bak = game / BACKUP / "files" / exe.relative_to(game).as_posix()
    if not bak.exists():
        bak = game / BACKUP / exe.name
    try:
        key = (str(game), img.stat().st_mtime_ns, exe.stat().st_mtime_ns, mod.stat().st_mtime_ns,
               bak.stat().st_mtime_ns if bak.exists() else 0,
               bak.stat().st_size if bak.exists() else 0)
    except OSError:
        return False
    if key in _built_cache:
        return _built_cache[key]
    try:
        # The builder writes a new executable; a plain `--no-build` leaves the old one (the backup).
        ok = not bak.exists() or exe.stat().st_size != bak.stat().st_size or \
            exe.stat().st_mtime_ns != bak.stat().st_mtime_ns
        if ok:
            with open(img, "rb") as f:
                ok = f.read().find(CONFIG_MAGIC) >= 0
    except OSError:
        ok = False
    _built_cache.clear()
    _built_cache[key] = ok
    return ok


# --------------------------------------------------------------------------- main / config / romfs

def _read(data: bytes) -> bytes:
    i = data.find(CONFIG_MAGIC)
    if i < 0:
        raise I.InstallError("bad_module", "config block not found")
    return data[i:i + CONFIG_SIZE]


def _patch_bytes(data: bytes, s: I.Settings) -> bytes:
    i = data.find(CONFIG_MAGIC)
    if i < 0:
        raise I.InstallError("bad_module", "config block not found")
    # Older images carry the 72-byte config; the current module has 76 bytes (target_r3/sprint_l3).
    import struct
    size = struct.unpack_from("<I", data, i + 12)[0]
    if size < 72 or size > 256 or i + size > len(data):
        size = CONFIG_SIZE
    cfg = I.patch_config(data[i:i + size], s)
    return data[:i] + cfg + data[i + size:]


def main_build_id(main: Path) -> str:
    """The NSO module id (header +0x40, 32 bytes) as hex: the builders call the first 8 characters the build."""
    try:
        with open(main, "rb") as f:
            head = f.read(0x60)
    except OSError:
        return ""
    return head[0x40:0x60].hex()


def patch_main(main: Path, ips: Path) -> int:
    data = bytearray(main.read_bytes())
    text_off = struct.unpack_from("<I", data, 0x10)[0]
    delta = text_off - 0x100
    n = 0
    for off, patch in I.read_ips(ips.read_bytes()):
        at = off + delta
        if data[at:at + len(patch)] != patch:
            data[at:at + len(patch)] = patch
            n += 1
    main.write_bytes(bytes(data))
    return n


def _backup_candidates(game: Path, exefs_name: str) -> list[str]:
    """Every file the install and the builder's finish step can write, replace or remove, as game-relative paths.

    The builders stage their BESIDE list (the game exe, the renderer, SDL, ffmpeg, the VC runtime for FSR) into bin/
    when a launcher exists, write image.bin and their caches, replace the root launcher and remove the old loose
    libraries. Our install patches exefs/main and writes exefs/subsdk9. ReLink uses its own names."""
    if exefs_name == ".relink_exefs":
        rels = [
            "image.bin", "relink_prepared.bin", "relink_version.txt",
            "ReLink.exe", "relink.exe", "ReLink", "relink", "relink-game.exe", "relink-game",
            "relink-game.map",
            "bin/relink-game.exe", "bin/relink-game",
            "bin/relink_vk.dll", "bin/relink_vk.so", "bin/relink_vk.dylib",
            "bin/SDL2.dll", "bin/SDL3.dll", "bin/libSDL2-2.0.so.0", "bin/libSDL3.so.0", "bin/libSDL3.dylib",
            "bin/wgpu_native.dll", "bin/libwgpu_native.so",
            "bin/amd_fidelityfx_vk.dll", "bin/amd_fidelityfx_vk.so", "bin/libamd_fidelityfx_vk.so",
            "bin/LICENSE-FidelityFX.txt",
            "bin/vcrt/msvcp140.dll", "bin/vcrt/vcruntime140.dll", "bin/vcrt/vcruntime140_1.dll",
            "wheel/icons.bin", "wheel/names.bin", "wheel/sounds.bin",
            "language.txt", "relink_options.ini",
            f"{exefs_name}/main", f"{exefs_name}/subsdk9",
        ]
    else:
        rels = [
            "image.bin", "longshot_prepared.bin",
            "longshot.exe", "hookshot.exe", "hookshot", "longshot",
            "longshot.map", "longshot-game.exe", "wgpu_native.dll",
            "bin/longshot.exe", "bin/hookshot.exe", "bin/hookshot", "bin/longshot",
            "bin/longshot_vk.dll", "bin/longshot_vk.so", "bin/longshot_vk.dylib",
            "bin/SDL2.dll", "bin/SDL3.dll", "bin/libSDL2-2.0.so.0", "bin/libSDL3.so.0", "bin/libSDL3.dylib",
            "bin/wgpu_native.dll", "bin/libwgpu_native.so",
            "bin/longshot.map", "bin/longshot-game.exe",
            f"{exefs_name}/main", f"{exefs_name}/subsdk9",
        ]
    for d in (game / "bin", game):
        if d.is_dir():
            try:
                for f in d.iterdir():
                    if not f.is_file():
                        continue
                    # ffmpeg ships as av*/sw*.dll on Windows and libav*/libsw*.so* (or .dylib) elsewhere.
                    ffmpeg = (f.suffix.lower() == ".dll" and f.name.startswith(("av", "sw"))) or \
                             (f.name.startswith(("libav", "libsw")) and
                              (".so" in f.name or f.suffix.lower() == ".dylib"))
                    if ffmpeg:
                        rel = f.relative_to(game).as_posix()
                        if rel not in rels:
                            rels.append(rel)
            except OSError:
                pass
    return rels


MANIFEST_VERSION = 3


def backup_once(game: Path, exefs_original: str | None = None) -> Path:
    """Snapshot, once, everything the install and the rebuild can change.

    Files that exist are copied under `.shb-backup/files/` (folders too); files that do not exist are recorded as
    created, so the uninstall deletes them instead of leaving a half-built game. The builders' caches are only
    recorded: they are regenerated on every build (some are gigabytes) and deleted on uninstall, so a stale cache
    of the modded build cannot meet the restored game. The manifest keeps the first snapshot: installing again never
    overwrites the originals. Older backups (flat files in `.shb-backup/`, or a version 2 manifest) keep working."""
    bak = game / BACKUP
    files = bak / "files"
    files.mkdir(parents=True, exist_ok=True)
    man_path = bak / "manifest.json"
    man = None
    try:
        data = json.loads(man_path.read_text(encoding="utf-8"))
        if isinstance(data, dict) and data.get("version") in (2, MANIFEST_VERSION):
            man = data
    except (OSError, ValueError):
        man = None
    if man is None:
        man = {"version": MANIFEST_VERSION, "exefs": exefs_original or exefs_of(game).name, "files": []}
    else:
        man["version"] = MANIFEST_VERSION
    known = {e.get("path") for e in man.get("files", []) if isinstance(e, dict)}
    current = exefs_of(game).name          # (the folder may have been renamed to match the builder)
    for rel in _backup_candidates(game, current):
        if rel in known:
            continue
        src = game / rel
        entry = {"path": rel}
        if src.is_file():
            dst = files / rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)
            entry["kind"] = "file"
        elif src.is_dir():
            dst = files / rel
            if dst.exists():
                shutil.rmtree(dst, ignore_errors=True)
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(src, dst)
            entry["kind"] = "dir"
        else:
            entry["kind"] = "created"
        man["files"].append(entry)
        known.add(rel)
    for rel in VOLATILE:
        if rel not in known:
            man["files"].append({"path": rel, "kind": "volatile"})
            known.add(rel)
    for rel in VOLATILE_DIRS:
        if rel not in known:
            man["files"].append({"path": rel, "kind": "volatile-dir"})
            known.add(rel)
    man_path.write_text(json.dumps(man, indent=1), encoding="utf-8")
    return bak


def _restore_backup(game: Path, bak: Path) -> bool:
    """Undo everything the snapshot recorded: copy the saved files back and delete the ones that were not there."""
    restored = False
    man = None
    try:
        data = json.loads((bak / "manifest.json").read_text(encoding="utf-8"))
        if isinstance(data, dict) and data.get("version") in (2, MANIFEST_VERSION):
            man = data
    except (OSError, ValueError):
        man = None
    if man is not None:
        files = bak / "files"
        for e in man.get("files", []):
            if not isinstance(e, dict):
                continue
            rel = str(e.get("path") or "").replace("\\", "/")
            if not rel or rel.startswith("/") or ".." in rel.split("/") or ":" in rel:
                continue
            dst = game / rel
            kind = e.get("kind")
            try:
                if kind == "file":
                    src = files / rel
                    if src.is_file():
                        dst.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copy2(src, dst)
                        restored = True
                elif kind == "dir":
                    src = files / rel
                    if src.is_dir():
                        if dst.exists():
                            shutil.rmtree(dst, ignore_errors=True)
                        dst.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copytree(src, dst)
                        restored = True
                elif kind in ("volatile", "volatile-dir"):
                    if dst.is_dir():
                        shutil.rmtree(dst, ignore_errors=True)
                        restored = True
                    elif dst.is_file():
                        dst.unlink()
                        restored = True
                else:   # created by the install/build: removed again
                    if dst.is_dir():
                        shutil.rmtree(dst, ignore_errors=True)
                        restored = True
                    elif dst.is_file():
                        dst.unlink()
                        restored = True
            except OSError:
                pass
        name = man.get("exefs")
        if name in EXEFS_NAMES:
            for other in EXEFS_NAMES:
                p = game / other
                if other != name and p.exists() and not (game / name).exists():
                    try:
                        p.rename(game / name)
                        restored = True
                    except OSError:
                        pass
    # older backups: flat copies in .shb-backup/ (main, longshot.exe, hookshot, image.bin, longshot_prepared.bin)
    for src, dst in ((bak / "main", exefs_of(game) / "main"),
                     (bak / "longshot.exe", game / "bin" / "longshot.exe"),
                     (bak / "hookshot", game / "bin" / "hookshot"),
                     (bak / "hookshot.exe", game / "bin" / "hookshot.exe"),
                     (bak / "image.bin", game / "image.bin"),
                     (bak / "longshot_prepared.bin", game / "longshot_prepared.bin")):
        if src.exists():
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)
            restored = True
    return restored



def write_module(game: Path, s: I.Settings) -> None:
    variant = payload_variant(game)
    src = payload_dir() / variant / "subsdk9"
    if not src.exists():
        raise I.InstallError("no_payload", variant)
    (exefs_of(game) / "subsdk9").write_bytes(_patch_bytes(src.read_bytes(), s))


def write_image(game: Path, s: I.Settings) -> bool:
    img = game / "image.bin"
    if not img.exists():
        return False
    data = img.read_bytes()
    if CONFIG_MAGIC not in data:
        return False
    img.write_bytes(_patch_bytes(data, s))
    return True


def install_romfs(game: Path, s: I.Settings) -> tuple[int, list]:
    """Copy the selected romfs files into the recompiler's mod folder."""
    pay = payload_dir()
    target = game / "mods" / MOD_NAME / "romfs"
    notes = []
    style = s.button_style if s.button_style in I.BUTTON_STYLES else "switch"   # the button mapping's pictures
    game_layout = None if s.bm_layout == "gamecube" else I.ensure_layout(s)
    atlas, gamecube_art, gc_note = I.prepare_gamecube_layout(s, game, pay)
    for old in I.our_mod_folders(game / "mods"):   # older versions used other folder names
        if old.name != MOD_NAME:
            shutil.rmtree(old, ignore_errors=True)
    if target.parent.exists():
        shutil.rmtree(target.parent)
    copied = 0

    def copy(src: Path):
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
    if gc_note:
        notes.append(gc_note)
    drops = (["no_heart_drop"] if s.no_heart_drop else []) + (["no_magic_drop"] if s.no_magic_drop else [])
    if len(drops) == 2:        # one file carries both changes (same DropGlobalParam table)
        drops = ["no_heart_magic_drop"]
    for opt in drops:
        if not copy(pay / "romfs_opts" / opt):
            missing = ("no_heart_drop_missing", "no_magic_drop_missing") if opt == "no_heart_magic_drop" \
                else (opt + "_missing",)
            for key in missing:
                notes.append((key, ""))
    for opt, enabled in (("enemy_double", s.enemy_double),
                         ("farore_outdoor", s.farore_outdoor),
                         ("great_fairy", s.great_fairy)):
        if enabled and not copy(pay / "romfs_opts" / opt):
            notes.append((opt + "_missing", ""))
    notes.extend(I.install_enemy_mods(target, s, pay))
    if game_layout:            # last: the game's own prompts, font and controller picture (table merged)
        I.layouts.apply(target, game_layout)
        copied += 3
    fighting = I.other_layout_mods(game / "mods", {MOD_NAME})
    if fighting:
        notes.append(("layout_mods", ", ".join(fighting)))
    merged = I.merge_other_tables(game / "mods", {MOD_NAME}, target / I.layouts.REL["table"])
    if merged:
        notes.append(("tables_merged", ", ".join(merged)))
    later = I.later_table_mods(game / "mods", MOD_NAME)
    if later:
        notes.append(("table_later", ", ".join(later)))
    if copied:
        variant = payload_variant(game)
        sha = hashlib.sha256((payload_dir() / variant / "subsdk9").read_bytes()).hexdigest()
        (target.parent / ".super-hero-boost").write_text(
            json.dumps({"variant": "recomp", "module": variant, "module_sha": sha, **s.to_dict()}, indent=1))
    return copied, notes


# --------------------------------------------------------------------------- build

STAGE_RANGE = {
    "read": (0.00, 0.03),
    "lift": (0.03, 0.15),
    "compile": (0.15, 0.90),
    "shaders": (0.90, 0.99),
}


def cancel() -> None:
    _cancel.set()
    p = _proc
    if p is not None and p.poll() is None:
        try:
            p.terminate()
        except OSError:
            pass


def _python(builder: Path) -> Path:
    """The interpreter that runs make_game.py: the builder's own (Windows `tools/python/python.exe`, Linux/macOS
    `tools/python/bin/python3`), or a system python3 when the app is packaged (`sys.executable` is the app then)."""
    tools = builder / "tools"
    for p in (tools / "python" / "python.exe", tools / "python" / "python",
              tools / "python" / "bin" / "python3", tools / "python" / "bin" / "python",
              tools / "venv" / "bin" / "python", tools / "venv" / "bin" / "python3"):
        if p.exists():
            return p
    if getattr(sys, "frozen", False):
        for name in ("python3", "python", "py"):
            p = shutil.which(name)
            if p:
                return Path(p)
    return Path(sys.executable)


def _vk_headers_ok(builder: Path, work: Path | None) -> bool:
    """Ask the builder itself whether it can find the Vulkan, SPIR-V and VMA headers.

    Some builder packages keep the CPM cache at tools/cpm, which the builder finds by itself; a source
    build may not ship it at all. That is when make_game.py stops with "no headers for longshot_vk.dll".
    """
    tools = builder / "tools"
    code = ("import sys; sys.path.insert(0, 'nx2'); import build; "
            "print(1 if all(build.vulkan_headers(sys.argv[1] if len(sys.argv) > 1 else None)) else 0)")
    cmd = [str(_python(builder)), "-I", "-c", code]
    if work:
        cmd.append(str(work))
    try:
        r = subprocess.run(cmd, cwd=str(tools), capture_output=True, text=True, encoding="utf-8",
                           errors="replace", timeout=90,
                           creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0) if os.name == "nt" else 0)
    except (OSError, subprocess.SubprocessError):
        return True   # cannot tell: let the builder run its normal path
    if r.returncode:
        return True
    lines = [line.strip() for line in (r.stdout or "").splitlines() if line.strip()]
    return (lines[-1] if lines else "") == "1"


def _vk_prebuilt(game: Path, work: Path | None) -> Path | None:
    """A copy of the game's working renderer, safe to hand to make_game.py as <FLAVOR>_VK_PREBUILT.

    The copy lives in the build work folder: make_game.py copies the prebuilt back beside the new exe, and
    source and destination must not be the same file (it builds in place)."""
    names = (("relink_vk.dll", "relink_vk.so", "relink_vk.dylib") if flavor(game) == "relink"
             else ("longshot_vk.dll", "longshot_vk.so", "longshot_vk.dylib"))
    for sub in ("bin", ""):
        for name in names:
            src = game / sub / name if sub else game / name
            if not src.is_file():
                continue
            out = Path(work) if work else game / ".shb-work"
            try:
                out.mkdir(parents=True, exist_ok=True)
                dst = out / ("shb-" + name)
                if src.resolve() != dst.resolve():
                    shutil.copy2(src, dst)
                return dst
            except OSError:
                return None
    return None


def build_env(game: Path, builder: Path, work: Path | None) -> tuple[dict, str | None]:
    """Environment for make_game.py: the packaged CPM cache, the renderer when the builder cannot compile it
    (missing Vulkan/VMA headers) and the DLSS SDK the game was built with."""
    env = dict(os.environ)
    prefix = _env_prefix(game)
    cpm = builder / "tools" / "cpm"
    if cpm.is_dir() and not env.get("RX_CPM"):
        env["RX_CPM"] = str(cpm)
    opt = launcher_options(game)
    if opt["dlss"] and not env.get(f"{prefix}_NGX_SDK") and opt["ngx"]:
        env[f"{prefix}_NGX_SDK"] = str(opt["ngx"])
    if not env.get(f"{prefix}_VK_PREBUILT") and not _vk_headers_ok(builder, work):
        pre = _vk_prebuilt(game, work)
        if pre is not None:
            env[f"{prefix}_VK_PREBUILT"] = str(pre)
            return env, "Vulkan headers not found: reusing the installed renderer (%s)" % pre.name
    return env, None


def build(game: Path, builder: Path, work: Path | None, progress=None, log=None) -> None:
    """Run the builder's update, reporting (fraction, text) and every output line."""
    global _proc
    _cancel.clear()
    tools = builder / "tools"
    if not (tools / "nx2" / "make_game.py").exists():
        raise I.InstallError("recomp_no_builder", str(builder))
    if work is None:
        work = game / ".shb-work"
    try:
        Path(work).mkdir(parents=True, exist_ok=True)
    except OSError:
        work = None
    cmd = [str(_python(builder)), "-I", "nx2/make_game.py", "--update", str(game), "--jobs", "0"]
    if work:
        cmd += ["--work", str(work)]
    env, note = build_env(game, builder, work)
    prefix = _env_prefix(game)
    opt = launcher_options(game)
    if opt["dlss"] and not env.get(f"{prefix}_NGX_SDK"):
        if opt["dlssLicense"]:
            cmd += ["--fetch-ngx-sdk", "--accept-nvidia-license"]
        else:
            raise I.InstallError("recomp_no_dlss", str(opt["launcher"] or prefix))
    elif opt["dlss"]:
        note = ((note + "; ") if note else "") + "keeping DLSS (%s)" % Path(env[f"{prefix}_NGX_SDK"]).name
    if flavor(game) != "relink" and opt["fsr"]:
        cmd += ["--fsr"]
    env.update({"PYTHONUNBUFFERED": "1", "PYTHONIOENCODING": "utf-8", "PYTHONUTF8": "1"})
    if note and log:
        log(note)
    flags = getattr(subprocess, "CREATE_NO_WINDOW", 0) if os.name == "nt" else 0
    stage = "read"
    fraction = 0.0

    def report(text):
        if progress:
            progress(min(1.0, max(0.0, fraction)), text, stage)

    report("starting")
    _kill_game_copies(game, log)      # (a game left open would stop the builder at its final swap)
    stop_watch = threading.Event()
    threading.Thread(target=_watch_games, args=(game, stop_watch, log), daemon=True,
                     name="shb-game-watch").start()
    try:
        try:
            _proc = subprocess.Popen(cmd, cwd=str(tools), stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                     text=True, encoding="utf-8", errors="replace", bufsize=1, env=env,
                                     creationflags=flags)
        except OSError as ex:
            raise I.InstallError("recomp_no_builder", str(ex))
        last = []
        with _proc:
            for line in _proc.stdout or ():
                line = line.rstrip("\r\n")
                if not line:
                    continue
                last.append(line)
                last = last[-40:]
                if line.startswith("@@stage "):
                    parts = line.split(" ", 2)
                    stage = parts[1] if len(parts) > 1 else stage
                    fraction = STAGE_RANGE.get(stage, (fraction, fraction))[0]
                    report(parts[2] if len(parts) > 2 else stage)
                elif line.startswith("@@progress "):
                    try:
                        p = float(line.split(" ", 1)[1])
                    except ValueError:
                        continue
                    lo, hi = STAGE_RANGE.get(stage, (0.0, 1.0))
                    fraction = lo + max(0.0, min(1.0, p)) * (hi - lo)
                    report("")
                elif line.startswith("@@plan "):
                    report("")
                elif line.startswith("@@eta "):
                    report("")
                elif line.startswith("@@error "):
                    try:
                        _proc.terminate()
                    except OSError:
                        pass
                    raise I.InstallError("recomp_build_failed", line[8:])
                elif line.startswith("@@"):
                    continue
                else:
                    if log:
                        log(line)
                if _cancel.is_set():
                    raise I.InstallError("recomp_cancelled")
            code = _proc.wait()
    finally:
        stop_watch.set()
    _proc = None
    if _cancel.is_set():
        raise I.InstallError("recomp_cancelled")
    if code:
        raise I.InstallError("recomp_build_failed", "\n".join(last[-8:]))
    if progress:
        progress(1.0, "done", "done")


# --------------------------------------------------------------------------- install / remove

def install(game_path, s: I.Settings, rebuild: bool = True, builder_path=None, work=None,
            progress=None, log=None) -> Result:
    game = find_game(game_path)
    if game is None:
        raise I.InstallError("recomp_no_game", str(game_path))
    # A copy running from this folder cannot be replaced (the builder refuses at its final swap). Close it and
    # carry on; only fail if it could not be closed (another user's/elevated process).
    _kill_game_copies(game, log)
    if _game_processes(game):
        raise I.InstallError("recomp_running", str(game))
    builder = None
    exefs_name = exefs_of(game).name          # (the name the folder had before any rename: restored on uninstall)
    if rebuild:
        builder = Path(builder_path).expanduser() if builder_path else find_builder(game)
        exp = expected_exefs(builder)
        ex = exefs_of(game)
        if exp and ex.name != exp:
            dest = game / exp
            if not dest.exists():
                ex.rename(dest)
    exefs = exefs_of(game)
    variant = payload_variant(game)
    ips = payload_dir() / variant / "ours.ips"
    if not ips.exists():
        raise I.InstallError("no_payload", variant)
    bid = main_build_id(exefs / "main")
    if bid and not bid.startswith(MODULE_BUILD):
        raise I.InstallError("recomp_bad_build", bid[:8])
    backup_once(game, exefs_name)
    n = patch_main(exefs / "main", ips)
    write_module(game, s)
    copied, notes = install_romfs(game, s)
    write_image(game, s)

    if rebuild:
        if not builder or not (Path(builder) / "tools" / "nx2" / "make_game.py").exists():
            raise I.InstallError("recomp_no_builder", str(builder_path or ""))
        w = Path(work).expanduser() if work else find_work(game)
        build(game, Path(builder), w, progress=progress, log=log)
        # (the lift writes a new image.bin: the module reads its configuration from there, so patch it again and say
        # so when this build cannot have it, instead of reporting a finished install)
        if not write_image(game, s):
            raise I.InstallError("recomp_needs_build", str(game))
    return Result(game, rebuild, notes)


def is_installed(game_path) -> bool:
    """The game folder holds our module (it was installed, whether or not it was compiled yet)."""
    game = find_game(game_path)
    if game is None:
        return False
    try:
        return (exefs_of(game) / "subsdk9").is_file()
    except I.InstallError:
        return False


def recompile(game_path, builder_path=None, work=None, progress=None, log=None) -> Result:
    """Run only the builder: nothing of the mod is copied, patched or rewritten, so whatever was changed by hand in
    the game folder (the module, the romfs, the exefs) is what gets compiled."""
    game = find_game(game_path)
    if game is None:
        raise I.InstallError("recomp_no_game", str(game_path))
    if not is_installed(game):
        raise I.InstallError("recomp_needs_install", str(game))
    _kill_game_copies(game, log)
    if _game_processes(game):
        raise I.InstallError("recomp_running", str(game))
    builder = Path(builder_path).expanduser() if builder_path else find_builder(game)
    if not builder or not (Path(builder) / "tools" / "nx2" / "make_game.py").exists():
        raise I.InstallError("recomp_no_builder", str(builder_path or ""))
    w = Path(work).expanduser() if work else find_work(game)
    build(game, Path(builder), w, progress=progress, log=log)
    # The lift wrote a new image.bin from the module in the exefs folder; it must carry the configuration block.
    # (It is not rewritten here: it already holds what that module held.)
    img = game / "image.bin"
    try:
        with open(img, "rb") as f:
            ok = f.read().find(CONFIG_MAGIC) >= 0
    except OSError:
        ok = False
    if not ok:
        raise I.InstallError("recomp_needs_build", str(game))
    return Result(game, True, [])


def reinstall(game_path, s: I.Settings, builder_path=None, work=None, progress=None, log=None) -> Result:
    """Uninstall (every original file back), then copy everything again and compile."""
    game = find_game(game_path)
    if game is None:
        raise I.InstallError("recomp_no_game", str(game_path))
    _kill_game_copies(game, log)
    if _game_processes(game):
        raise I.InstallError("recomp_running", str(game))
    if log:
        log("Removing the mod and restoring the original files")
    uninstall(game)
    if log:
        log("Installing the mod again")
    return install(game, s, rebuild=True, builder_path=builder_path, work=work, progress=progress, log=log)


def apply_settings(game_path, s: I.Settings) -> Result:
    """Configuration and romfs only: no rebuild (the code is already in the executable)."""
    game = find_game(game_path)
    if game is None:
        raise I.InstallError("recomp_no_game", str(game_path))
    _kill_game_copies(game)
    if _game_processes(game):
        raise I.InstallError("recomp_running", str(game))
    if not (exefs_of(game) / "subsdk9").exists():
        raise I.InstallError("recomp_needs_build", str(game))
    src = payload_dir() / payload_variant(game) / "subsdk9"
    try:
        cfg = json.loads((game / "mods" / MOD_NAME / ".super-hero-boost").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        cfg = {}
    current = hashlib.sha256(src.read_bytes()).hexdigest() if src.exists() else None
    if src.exists() and (cfg.get("module") != payload_variant(game) or cfg.get("module_sha") != current):
        raise I.InstallError("recomp_needs_build", str(game))
    bid = main_build_id(exefs_of(game) / "main")
    if bid and not bid.startswith(MODULE_BUILD):
        raise I.InstallError("recomp_bad_build", bid[:8])
    backup_once(game)
    write_module(game, s)
    copied, notes = install_romfs(game, s)
    if not write_image(game, s):
        raise I.InstallError("recomp_needs_build", str(game))
    return Result(game, False, notes)


def uninstall(game_path) -> bool:
    game = find_game(game_path)
    if game is None:
        return False
    exefs = exefs_of(game)
    bak = game / BACKUP
    had_snapshot = bak.is_dir()
    restored = _restore_backup(game, bak)
    if had_snapshot:
        # Caches the builders write: never part of the snapshot, always removed. This also covers manifests made
        # before these names were listed (the 540 MB longshot_shaders.bin, the shipped pipeline lists...).
        for rel in VOLATILE:
            p = game / rel
            try:
                if p.is_file():
                    p.unlink()
                    restored = True
            except OSError:
                pass
        for rel in VOLATILE_DIRS:
            p = game / rel
            try:
                if p.is_dir():
                    shutil.rmtree(p, ignore_errors=True)
                    restored = True
            except OSError:
                pass
    for name in ("subsdk9", "subsdk9.bak-longshot"):
        p = exefs / name
        if p.exists():
            p.unlink()
            restored = True
    for d in I.our_mod_folders(game / "mods"):
        try:
            shutil.rmtree(d)
            restored = True
        except OSError:
            pass
    return restored
