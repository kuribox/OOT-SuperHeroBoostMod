"""Install the mod into a Longshot or Hookshot build (static recompilation).

Hookshot and Longshot are **not emulators**: they translate the game's code to C
and compile it into a native program (`bin/longshot.exe` or `bin/hookshot`).
The `.longshot_exefs` folder is only the source of that build, and an `exefs`
folder inside a mod is logged and left out ("the code is compiled ahead of
time"). So a code mod cannot be installed by dropping files: the game has to be
built again with the mod's code inside.

This tool does the whole thing:

  1. backs up the game's executable, image and `main` (once),
  2. writes the chosen configuration block into `subsdk9` (the factory block has
     the HUD and button mapping off) and patches the 26 hook sites in
     `.longshot_exefs/main`,
  3. copies the romfs files to `<game>/mods/Super-Hero-Boost/romfs`
     (Longshot/Hookshot's own mod layout),
  4. runs the builder's `nx2/make_game.py --update <game>` with its bundled
     toolchain, which lifts the patched code and compiles the new executable.

The build takes about as long as the first one (25-45 minutes). The result is
tied to the user's own dump and cannot be shared.

Usage:
    # full install with the payload defaults (Switch buttons, no optional romfs)
    python tools/install_longshot.py --game "F:\\Switch2\\Longshot"

    # reuse an existing emulator install: its profile and its romfs files
    python tools/install_longshot.py --game "F:\\Switch2\\Longshot" ^
        --config "E:\\...\\00-Super-Hero-Boost\\exefs\\subsdk9" ^
        --romfs "E:\\...\\00-Super-Hero-Boost\\romfs"

    # the game was already rebuilt: only change the configuration (no build)
    python tools/install_longshot.py --game "F:\\Switch2\\Longshot" ^
        --config "...\\subsdk9" --config-only

    python tools/install_longshot.py --game "F:\\Switch2\\Longshot" --uninstall

Hookshot is the same; point --game at its folder and --builder at its builder.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PAYLOAD = ROOT / "app" / "payload"
STANDALONE = PAYLOAD / "standalone"
IPS = ROOT / "build" / "native" / "exefs" / "E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348.ips"
MODULE = ROOT / "build" / "native" / "exefs" / "subsdk9"
MOD_NAME = "Super-Hero-Boost"
BACKUP = ".shb-backup"           # beside .longshot_exefs, never inside it
CONFIG_MAGIC = b"SHBCFG01"
CONFIG_SIZE = 72                 # ShbConfig, version 1
BUTTON_STYLES = ("switch", "xbox", "playstation")
ROMFS_OPTS = ("enemy_double", "farore_outdoor", "great_fairy", "no_heart_drop", "no_magic_drop")
sys.path.insert(0, str(ROOT / "app-legacy"))
import installer as I  # noqa: E402
import recomp as R  # noqa: E402


# --------------------------------------------------------------------------- IPS

def parse_ips32(raw: bytes):
    if raw[:5] != b"IPS32":
        raise SystemExit("not an IPS32 patch")
    pos, out = 5, []
    while pos < len(raw):
        if raw[pos:pos + 4] == b"EEOF" or raw[pos:pos + 3] == b"EOF":
            break
        off = struct.unpack_from(">I", raw, pos)[0]
        size = struct.unpack_from(">H", raw, pos + 4)[0]
        pos += 6
        if size:
            out.append((off, raw[pos:pos + size]))
            pos += size
        else:
            count = struct.unpack_from(">H", raw, pos)[0]
            value = raw[pos + 2:pos + 3]
            pos += 3
            out.append((off, value * count))
    return out


def exefs_of(game: Path) -> Path:
    for n in (".longshot_exefs", ".hookshot_exefs"):
        d = game / n
        if (d / "main").exists():
            return d
    raise SystemExit("no .longshot_exefs/.hookshot_exefs in %s: is this a Longshot/Hookshot game folder?" % game)


def apply_ips(main: Path, ips: Path) -> tuple[int, int, bool]:
    """Returns (records, file-offset delta, changed)."""
    data = bytearray(main.read_bytes())
    text_off = struct.unpack_from("<I", data, 0x10)[0]   # Longshot's main: 0x104
    delta = text_off - 0x100                              # IPS offsets assume text at 0x100
    recs = parse_ips32(ips.read_bytes())
    changed = False
    for off, patch in recs:
        at = off + delta
        if data[at:at + len(patch)] != patch:
            data[at:at + len(patch)] = patch
            changed = True
    if changed:
        main.write_bytes(bytes(data))
    return len(recs), delta, changed


# ------------------------------------------------------------------------ config

def read_config(src: Path) -> bytes:
    d = src.read_bytes()
    i = d.find(CONFIG_MAGIC)
    if i < 0:
        raise SystemExit("%s has no %s config block" % (src, CONFIG_MAGIC.decode()))
    if len(d) < i + CONFIG_SIZE:
        raise SystemExit("%s: config block cut short" % src)
    return d[i:i + CONFIG_SIZE]


def write_config(target: Path, cfg: bytes) -> bool:
    d = bytearray(target.read_bytes())
    i = d.find(CONFIG_MAGIC)
    if i < 0:
        return False
    d[i:i + CONFIG_SIZE] = cfg
    target.write_bytes(bytes(d))
    return True


def config_summary(cfg: bytes) -> str:
    name = lambda k: cfg[k]
    return ("stamina %d (bar %d), HUD rupees %d / clock %d / collectibles %d, button mapping %d (style %d), "
            "songbook %d, enemy HP %d%%, damage %d%%, great fairy/farore are romfs"
            % (name(16), name(52), name(30), name(31), name(43), name(37), name(38), name(36),
               struct.unpack_from("<H", cfg, 26)[0], struct.unpack_from("<H", cfg, 28)[0]))


# ------------------------------------------------------------------------ builder

def find_builder() -> Path | None:
    env = os.environ.get("LONGSHOT_BUILDER") or os.environ.get("HOOKSHOT_BUILDER")
    if env and (Path(env) / "tools" / "nx2" / "make_game.py").exists():
        return Path(env)
    local = Path(os.environ.get("LOCALAPPDATA", ""))
    roots = [local / "LongshotLauncher" / "toolchains", local / "HookshotLauncher" / "toolchains"]
    found = []
    for root in roots:
        if root.is_dir():
            for p in list(root.glob("*/LongshotBuilder")) + list(root.glob("*/HookshotBuilder")):
                if (p / "tools" / "nx2" / "make_game.py").exists():
                    found.append(p)
    return sorted(found)[-1] if found else None


def default_work() -> Path | None:
    cfg = Path(os.environ.get("LOCALAPPDATA", "")) / "LongshotLauncher" / "settings.json"
    try:
        return Path(json.loads(cfg.read_text(encoding="utf-8"))["work"])
    except (OSError, KeyError, ValueError):
        return None


def build_game(game: Path, builder: Path, work: Path | None, jobs: int) -> int:
    tools = builder / "tools"
    py = tools / "python" / "python.exe"
    if not py.exists():
        py = Path(sys.executable)
    cmd = [str(py), "-I", "nx2/make_game.py", "--update", str(game), "--jobs", str(jobs)]
    if work:
        cmd += ["--work", str(work)]
    env, note = R.build_env(game, builder, work)
    if note:
        print(note, flush=True)
    print("building: %s" % " ".join(cmd), flush=True)
    print("        (the game must be closed; this takes 25-45 minutes)", flush=True)
    return subprocess.call(cmd, cwd=str(tools), env=env)


# -------------------------------------------------------------------------- files

def backup_once(game: Path, exefs: Path) -> Path:
    """Snapshot, once, every file the install and the rebuild can change (app-legacy/recomp.py does the list)."""
    bak = R.backup_once(game)
    print("backup: %s" % bak, flush=True)
    return bak


def patch_exefs(game: Path, cfg: bytes | None) -> Path:
    exefs = exefs_of(game)
    src_ips = IPS if IPS.exists() else STANDALONE / "ours.ips"
    src_mod = MODULE if MODULE.exists() else STANDALONE / "subsdk9"
    if not src_ips.exists() or not src_mod.exists():
        raise SystemExit("build the payload first (tools/make_release.py)")

    backup_once(game, exefs)
    n, delta, changed = apply_ips(exefs / "main", src_ips)
    print("patched %d sites in main%s (file offset delta %+#x)"
          % (n, "" if changed else " (already patched)", delta))
    shutil.copy2(src_mod, exefs / "subsdk9")
    print("copied the module to %s/subsdk9" % exefs.name)
    if cfg:
        if not write_config(exefs / "subsdk9", cfg):
            raise SystemExit("the module has no config block")
        print("wrote the configuration: %s" % config_summary(cfg))
    else:
        print("WARNING: factory config (HUD and button mapping are off). Pass --config from an emulator install.")
    return exefs


def install_romfs(game: Path, src: Path | None, style: str, opts: set[str], enemy=(0, 0, 0)) -> int:
    romfs = game / "mods" / MOD_NAME / "romfs"
    if romfs.exists():
        shutil.rmtree(romfs)
    copied = 0
    if src:
        src = src / "romfs" if (src / "romfs").is_dir() else src
        for f in src.rglob("*"):
            if f.is_file():
                dst = romfs / f.relative_to(src)
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(f, dst)
                copied += 1
    else:
        bases = [PAYLOAD / "button_romfs" / "common", PAYLOAD / "button_romfs" / style]
        chosen = [o for o in ROMFS_OPTS if o in opts]
        if "no_heart_drop" in chosen and "no_magic_drop" in chosen:   # one file carries both changes
            chosen = [o for o in chosen if o not in ("no_heart_drop", "no_magic_drop")] + ["no_heart_magic_drop"]
        bases += [PAYLOAD / "romfs_opts" / o for o in chosen]
        for base in bases:
            if not base.is_dir():
                continue
            for f in base.rglob("*"):
                if f.is_file():
                    dst = romfs / f.relative_to(base)
                    dst.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(f, dst)
                    copied += 1
        if any(enemy):
            s = I.Settings(enemy_move=enemy[0], enemy_anim=enemy[1], enemy_attack=enemy[2])
            for key, _ in I.install_enemy_mods(romfs, s, PAYLOAD):
                print("WARNING: %s" % key)
    if copied:
        print("copied %d romfs files to %s" % (copied, romfs))
    return copied


def uninstall(game: Path) -> None:
    bak = game / BACKUP
    restored = R.uninstall(game)
    if restored:
        print("restored the original files from %s and removed the mod" % bak)
    else:
        print("nothing to uninstall (no backup and no mod files)")
    if not (bak / "files").exists() and not (bak / "longshot.exe").exists() \
            and not (bak / "hookshot").exists() and not (bak / "hookshot.exe").exists():
        print("no executable backup: the game still may have the mod's code; rebuild the game to remove it.")


# --------------------------------------------------------------------------- main

if __name__ == "__main__":
    ap = argparse.ArgumentParser(description="Install Super Hero Boost into a Longshot/Hookshot build")
    ap.add_argument("--game", required=True, help='the game folder (the one with ".longshot_exefs" and "romfs")')
    ap.add_argument("--builder", help="the LongshotBuilder/HookshotBuilder folder (autodetected)")
    ap.add_argument("--work", help="the builder's work folder (autodetected)")
    ap.add_argument("--jobs", type=int, default=0, help="compilers at once (0: automatic)")
    ap.add_argument("--config", help="a subsdk9 whose configuration block to use (an emulator install)")
    ap.add_argument("--romfs", help="an installed mod's romfs folder to copy (an emulator install)")
    ap.add_argument("--button-style", choices=BUTTON_STYLES, default="switch")
    ap.add_argument("--option", action="append", choices=ROMFS_OPTS, default=[], help="optional romfs feature")
    ap.add_argument("--enemy-move", type=int, choices=I.ENEMY_LEVELS, default=0,
                    help="enemy movement: 0 off, 1 x1.5, 2 x2, 3 x3")
    ap.add_argument("--enemy-anim", type=int, choices=I.ENEMY_LEVELS, default=0,
                    help="enemy animation: 0 off, 1 x1.5, 2 x2, 3 x3")
    ap.add_argument("--enemy-attack", type=int, choices=I.ENEMY_LEVELS, default=0,
                    help="enemy attack cadence: 0 off, 1 x1.5, 2 x2, 3 x3")
    ap.add_argument("--config-only", action="store_true", help="patch the module and image.bin; no rebuild")
    ap.add_argument("--no-build", action="store_true", help="only patch the exefs and copy the romfs")
    ap.add_argument("--no-romfs", action="store_true", help="do not touch the romfs mod folder")
    ap.add_argument("--uninstall", action="store_true")
    a = ap.parse_args()
    game = Path(a.game).expanduser()
    if a.uninstall:
        uninstall(game)
        sys.exit(0)

    cfg = read_config(Path(a.config)) if a.config else None
    if a.config and not a.config_only and not a.romfs and not a.no_romfs:
        print("note: --config without --romfs copies the payload's romfs (%s buttons)" % a.button_style)

    if a.config_only:
        exefs = exefs_of(game)
        if not cfg:
            print("--config-only needs --config", file=sys.stderr)
            sys.exit(2)
        if not write_config(exefs / "subsdk9", cfg):
            raise SystemExit("the module has no config block")
        img = game / "image.bin"
        ok = write_config(img, cfg) if img.exists() else False
        print("wrote the configuration: %s" % config_summary(cfg))
        print("image.bin patched: %s (no rebuild needed)" % ("yes" if ok else "no image.bin"))
        sys.exit(0)

    patch_exefs(game, cfg)
    if not a.no_romfs:
        install_romfs(game, Path(a.romfs) if a.romfs else None, a.button_style, set(a.option),
                      (a.enemy_move, a.enemy_anim, a.enemy_attack))
    if a.no_build:
        print("prepared. Run the builder's update to rebuild the game.")
        sys.exit(0)

    builder = Path(a.builder) if a.builder else find_builder()
    if not builder or not (builder / "tools" / "nx2" / "make_game.py").exists():
        print("builder not found: pass --builder (the folder with tools/nx2/make_game.py), "
              "or set LONGSHOT_BUILDER.", file=sys.stderr)
        sys.exit(2)
    work = Path(a.work) if a.work else default_work()
    code = build_game(game, builder, work, a.jobs)
    if code:
        print("the build failed (exit %d). The backup is in %s" % (code, game / BACKUP), file=sys.stderr)
    sys.exit(code)
