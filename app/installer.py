"""Install logic of the Super Hero Boost companion app. No compiler involved: it only patches the config block
inside the prebuilt module, merges the IPS patch files and copies files into the emulator's mod folder.
User-facing problems are raised as InstallError(key, arg) so the window can show them in the chosen language."""
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import zstandard
from dataclasses import dataclass, field
from pathlib import Path

import gamecube
import layouts

TITLE_ID = "0400486024DEC000"
BUILD_ID = "E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348"
MOD_NAME = "zz-Super-Hero-Boost"   # must sort last: ReLink/emulators let the last mod win each file
MARKER = ".super-hero-boost"
BUTTON_STYLES = ("switch", "playstation", "xbox")   # buttonMapping button art (the config byte: 0 Switch, 1 Xbox, 2 PS)
HUD_MODES = ("wide", "simple", "plain")
HUD_MODE_BYTE = {"wide": 0, "simple": 1, "plain": 2}   # 2 used to be "map" (minimal); old configs read as plain
STYLE_BYTE = {"switch": 0, "xbox": 1, "playstation": 2}
GAME_LAYOUTS = ("none", "playstation", "xbox")   # game-wide prompts download (optional; recompilers have their own)
BM_LAYOUTS = ("nintendo", "ab", "xbox", "gamecube")  # face-button positions in the mod UI
BM_LAYOUT_BYTE = {"nintendo": 0, "ab": 1, "xbox": 2, "gamecube": 3}
L_MODES = ("assign", "selector")           # buttonMapping L: an item slot or the game's item selector
L_MODE_VALUES = {"assign": 0, "selector": 2}
# Button remap (core/remap.h): what each of the eight buttons does. REMAP_BUTTONS is the module's physical order
# (the config bytes bm_role[0..7]); ROLES is the byte value order. Four item roles, each on one button at most, so
# at most four buttons hold an item. "item_main" is the game's own item box (picked with A in the pause menu or the
# item selector); item1 / item2 / item3 are the mod's slots (classically B, L and R).
REMAP_BUTTONS = ("L", "X", "R", "Y", "A", "ZR", "B", "ZL", "L3", "R3", "DR")
# "DR" is the D-pad's right arrow: a short press stays the game's Navi call, the role is its LONG press (the config
# byte bm_role_dr, appended at the end of the block). Roles that need no HUD button only.
ROLES = ("none", "jump", "sword", "action", "target", "item1", "item2", "item3", "item_main",
         "camera", "sprint", "shield", "selector", "armory")   # byte value = index (core/remap.h ROLE_*)
ITEM_ROLES = ("item1", "item2", "item3", "item_main")
# Roles limited to some buttons (everything else goes anywhere)
_NO_STICKS = ("L", "X", "R", "Y", "A", "ZR", "B", "ZL")
_DR_ROLES = ("jump", "sword", "camera", "sprint", "shield", "selector", "armory")
ROLE_BUTTONS = {"selector": (*_NO_STICKS, "DR"),   # any button, not the sticks
                "target": ("ZL", "ZR", "L3", "R3"),
                # action: a face button only (the game's context prompt draws one of A / B / X / Y)
                "action": ("A", "B", "X", "Y"),
                **{r: _NO_STICKS for r in ("item1", "item2", "item3", "item_main")}}   # items: no sticks (no HUD art)
CLASSIC_ROLES = {"L": "item2", "X": "jump", "R": "item3", "Y": "sword", "A": "action", "ZR": "item_main",
                 "B": "item1", "ZL": "target", "L3": "none", "R3": "camera", "DR": "none"}
# Ready-made tables (the app's preset list). Options (toggle sprint, switch target...) are kept apart.
REMAP_PRESETS = {
    "classic": dict(CLASSIC_ROLES),
    "action_down": dict(CLASSIC_ROLES, A="item1", B="action"),
    "shield": dict(CLASSIC_ROLES, L="shield", L3="sprint"),   # item 2 gives L to the shield
    "souls": {"L": "shield", "X": "jump", "R": "item2", "Y": "sword", "A": "action", "ZR": "item_main",
              "B": "item1", "ZL": "item3", "L3": "sprint", "R3": "target"},
}
REMAP_PRESET_KEYS = ("classic", "action_down", "shield", "souls")


def roles_with_armory(roles) -> dict:
    """The table with the armory on the held D-pad right (the profiles' setup): the armory is on no other button."""
    r = dict(normalize_roles(roles))
    for b, role in r.items():
        if role == "armory":
            r[b] = "none"
    r["DR"] = "armory"
    return normalize_roles(r)


def role_allowed(role: str, button: str) -> bool:
    if button == "DR":
        return role == "none" or role in _DR_ROLES
    return role in ROLES and (role not in ROLE_BUTTONS or button in ROLE_BUTTONS[role])


def normalize_roles(roles) -> dict:
    """A role per button, every role but "none" on one button at most (the first one keeps it), only where it is
    allowed (ROLE_BUTTONS). The item selector has its own virtual button (VBTN_SELECTOR): it no longer shares L with item 2.
    Unknown or missing buttons take their classic role when it is still free, else none."""
    src = roles if isinstance(roles, dict) else {}
    out, used = {}, set()
    for b in REMAP_BUTTONS:
        r = src.get(b, CLASSIC_ROLES[b])
        if not role_allowed(r, b):
            r = "none"
        if r != "none" and r in used:
            r = "none"
        out[b] = r
        used.add(r)
    return out


def roles_missing(roles) -> list:
    """The basic roles no button has (jump, sword, action, target): the game cannot do those then."""
    have = set(normalize_roles(roles).values())
    return [r for r in ("jump", "sword", "action", "target") if r not in have]


def roles_flags(roles) -> dict:
    """The module options a table implies: the L3-style sprint (a sprint button) and L as the item selector (a
    selector button). The ZL/R3 swap is never needed: target goes anywhere. The manual shield is a switch of its own
    (an option of the target button); the Shield role only works with it."""
    have = set(normalize_roles(roles).values())
    return dict(sprint_l3="sprint" in have, target_r3=False, bm_l_mode="selector" if "selector" in have else "assign")


def roles_from_flags(roles, manual_shield=None, target_r3=None, sprint_l3=None, l_selector=None) -> dict:
    """Bring a table in line with the old options (profiles, command line). None = leave that part as it is."""
    r = dict(normalize_roles(roles))

    def holder(role):
        return next((b for b, x in r.items() if x == role), None)

    def free_for(role, prefer):
        for b in prefer:
            if r[b] in ("none", "camera") and role_allowed(role, b):
                return b
        return None

    if target_r3 is not None:
        t = holder("target")
        if target_r3 and t != "R3":
            old = r["R3"]
            r["R3"] = "target"
            if t:
                r[t] = old if role_allowed(old, t) else "none"
        elif not target_r3 and t == "R3":
            z = r["ZL"]
            r["ZL"], r["R3"] = "target", (z if role_allowed(z, "R3") else "camera")
    if manual_shield is not None:
        sh = holder("shield")
        if manual_shield and not sh:
            displaced = r["L"]
            r["L"] = "shield"
            if displaced in ITEM_ROLES or displaced == "selector":
                if displaced == "selector":
                    dst = "R" if role_allowed("selector", "R") and r["R"] in ("none",) else None
                else:
                    dst = free_for(displaced, ("ZL", "L3", "R3"))
                if dst:
                    r[dst] = displaced
        elif not manual_shield and sh:
            r[sh] = "item2" if "item2" not in r.values() else "none"
    if sprint_l3 is not None:
        sp = holder("sprint")
        if sprint_l3 and not sp and r["L3"] == "none":
            r["L3"] = "sprint"
        elif not sprint_l3 and sp:
            r[sp] = "none"
    if l_selector is not None:
        sel = holder("selector")
        if l_selector and not sel:
            for b in ("L", "R"):
                if r[b] in ("item2", "none"):
                    r[b] = "selector"
                    break
        elif not l_selector and sel:
            r[sel] = "item2" if "item2" not in r.values() else "none"
    return normalize_roles(r)


LANGS = ("en", "es", "pt", "fr", "de", "it")   # the app's UI languages, also the mod's HUD labels
LANG_VALUES = {"en": 0, "es": 1, "pt": 2, "fr": 3, "de": 4, "it": 5}
# game language, as the mod's own HUD labels follow the language the game is played in
LANG_INDEX = {1: "en", 2: "en", 3: "fr", 4: "de", 5: "es", 6: "it", 7: "pt", 8: "pt"}   # the emulators' system language
LANG_NAMES = (("english", "en"), ("french", "fr"), ("german", "de"),
              ("spanish", "es"), ("italian", "it"), ("portuguese", "pt"))
ENEMY_LEVELS = (0, 10, 11, 4, 1, 5, 2, 6, 3, 7, 8, 9)   # display order: off, x0.5 .. x6
ENEMY_FACTORS = {1: 1.5, 2: 2.0, 3: 3.0, 4: 1.25, 5: 1.75, 6: 2.5, 7: 4.0, 8: 5.0, 9: 6.0,
                 10: 0.5, 11: 0.75}
ENEMY_ANIM_DIR = {1: "enemy_anim_x15", 2: "enemy_anim_x2", 3: "enemy_anim_x3",
                  4: "enemy_anim_x125", 5: "enemy_anim_x175", 6: "enemy_anim_x25",
                  7: "enemy_anim_x4", 8: "enemy_anim_x5", 9: "enemy_anim_x6",
                  10: "enemy_anim_x05", 11: "enemy_anim_x075"}
BOSS_ANIM_DIR = {k: v.replace("enemy_anim_", "boss_anim_") for k, v in ENEMY_ANIM_DIR.items()}
# HUD row: hud.c draws the 1014-unit plates at 0.46 (rupee, clock) and 0.42 (the two collectible plates), so the
# installer can pack the enabled blocks with the same gap between plate edges (layout units, root-relative).
HUD_X0 = -1518
HUD_GAP = 24
HUD_PLATE_W = (466.4, 466.4, 425.9, 425.9)   # rupee, clock, skulltulas, heart pieces


class InstallError(Exception):
    def __init__(self, key: str, arg: str = ""):
        super().__init__(f"{key}: {arg}")
        self.key, self.arg = key, arg


def resource_dir() -> Path:
    base = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent))
    return base / "payload"


@dataclass
class Settings:
    stamina: bool = True
    preset: int = 0                 # 0 hardcore, 1 hardcore combat only, 2 sprint only, 3 advanced
    jump: bool = True
    attack: bool = True
    sprint: bool = True
    block: bool = True
    shoot: bool = True
    roll: bool = True
    combat_only: bool = False
    enemy_hp: int = 100             # enemy health, percent of vanilla
    sprint_speed: int = 125         # sprint (A held while moving) speed, percent of vanilla
    damage: int = 100               # damage the player takes, percent of vanilla
    no_heart_drop: bool = False     # hearts dropped by enemies/grass become green rupees
    hud_rupees: bool = False
    hud_clock: bool = False
    clock_24h: bool = False
    songbook: bool = True            # play a learned song by selecting it in the native songbook
    quick_boots: bool = False        # hold D-pad down to pick the boots
    quick_mask: bool = False         # hold D-pad up to pick a mask
    hud_collectibles: bool = False   # per-area Skulltula / heart-piece counter
    hud_mode: str = "wide"           # one of HUD_MODES: wide / simple (small bar) / map (hearts on the Minus map)
    enemy_double: bool = False       # beta: install the duplicated-enemy romfs mod
    enemy_speed: bool = False        # deprecated: old boolean maps to all three components at x2
    enemy_move: int = 0              # enemy movement speed: 0 off, 1 x1.5, 2 x2, 3 x3
    enemy_anim: int = 0              # enemy animation playback: same levels
    enemy_attack: int = 0            # enemy attack cadence: same levels
    enemy_proj: int = 0              # enemy projectile speed: same levels
    boss_move: int = 0               # bosses and minibosses (left out of the enemy options): movement, same levels
    boss_anim: int = 0               # bosses and minibosses: animation playback, same levels
    enemy_alert: int = 0             # how far a spotting enemy alerts the others: same levels (multiplier)
    enemy_aggro: int = 0             # one of AGGRO_LEVELS: 0 off, 1 aggressive, 2 maximum
    great_fairy: bool = False        # reorder the three Great Fairy magic rewards
    farore_outdoor: bool = False     # allow Farore's Wind points outside dungeons
    button_mapping: bool = True      # buttonMapping: our own item buttons B / L / R / ZR on the HUD
    button_style: str = "switch"     # one of BUTTON_STYLES: the button mapping's own button pictures
    bm_layout: str = "nintendo"      # one of BM_LAYOUTS: face button positions of the button mapping HUD
    game_layout: str = "none"        # one of GAME_LAYOUTS: download the game's own PS/Xbox prompts (optional)
    bm_simple: bool = False          # simple layout: only the assignable buttons (no A / X / Y)
    bm_l_mode: str = "assign"        # one of L_MODES
    stamina_bar: bool = False        # stamina UI: segmented horizontal bar instead of the ring
    manual_shield: bool = False      # ZL holds the guard, L toggles targeting, R3 cancels
    target_r3: bool = False          # swap ZL and R3 (target on R3; Toggle targeting recommended)
    sprint_l3: bool = False          # L3 (left stick click) presses A while held (sprint)
    auto_target: bool = False        # when the target dies, press target again for the next enemy
    switch_target: bool = False      # the other shoulder button switches to the next target
    lang: str = "en"                 # language of the mod's own HUD labels (one of LANGS)
    no_magic_drop: bool = False      # dropped magic jars become green rupees (romfs file)
    hide_minimap: bool = False       # hide the corner minimap while playing
    sprint_l3_toggle: bool = False   # sprint_l3: L3 turns the sprint on / off instead of holding it
    bm_remap: bool = True            # button remap: use bm_roles (the table implies shield/sprint/selector)
    bm_roles: dict = field(default_factory=lambda: dict(CLASSIC_ROLES))   # button (REMAP_BUTTONS) -> role (ROLES)
    hud_ctrl_scale: int = 100        # size of the button cluster HUD, percent (HUD_SCALE_MIN..MAX)
    hud_count_scale: int = 100       # size of the counter row (rupees, clock, collectibles), percent

    def to_dict(self): return dict(self.__dict__)


# what each preset switches on (what the window shows greyed out); "advanced" uses the user's own choices
PRESET_VIEW = {
    0: dict(jump=1, attack=1, sprint=1, block=1, shoot=1, roll=1, combat_only=0),
    1: dict(jump=1, attack=1, sprint=1, block=1, shoot=1, roll=1, combat_only=1),
    2: dict(jump=0, attack=0, sprint=1, block=0, shoot=0, roll=0, combat_only=0),
}


# ---------- emulator / mods folder detection ----------
def _load_directory(emu: Path) -> Path | None:
    """Read `load_directory` from the emulator's qt-config.ini (portable across platforms)."""
    for cfg in (emu / "user" / "config" / "qt-config.ini", emu / "config" / "qt-config.ini"):
        if not cfg.exists():
            continue
        try:
            txt = cfg.read_text(errors="replace")
        except OSError:
            continue
        for m in re.finditer(r"^\s*load_directory\s*=\s*(.+?)\s*$", txt, re.M):
            value = m.group(1).strip().strip('"')
            if not value:
                continue
            p = Path(value).expanduser()
            if not p.is_absolute():
                p = emu / p
            if p.exists():
                return p
    return None


EMU_EXES = ("eden", "citron", "sudachi", "suyu", "torzu", "yuzu", "ryujinx", "kenji-nx", "kenjinx")
EMU_SKIP = ("maintenance", "update", "uninst", "crash", "cmd", "room", "tester")


def emulator_exe(emu: Path) -> Path | None:
    """The emulator program in the chosen folder (or a few folders above it, when the path points at its
    user/load or title folder): a known name first, then any other program there."""
    if not emu:
        return None
    p = Path(emu).expanduser()
    if p.is_file():
        return p
    ext = (".exe",) if os.name == "nt" else ("", ".AppImage")
    for d in [p, *list(p.parents)[:4]]:
        if not d.is_dir():
            continue
        try:
            files = [f for f in d.iterdir() if f.is_file()]
        except OSError:
            continue
        progs = [f for f in files if (f.suffix.lower() in ext if os.name == "nt" else
                                      (f.suffix in ext and os.access(f, os.X_OK)))]
        for name in EMU_EXES:
            for f in progs:
                if f.stem.lower().startswith(name) and not any(s in f.stem.lower() for s in EMU_SKIP):
                    return f
        if os.name == "nt":
            other = [f for f in progs if not any(s in f.stem.lower() for s in EMU_SKIP)]
            if len(other) == 1:
                return other[0]
    return None


def launch(exe: Path, args=()) -> None:
    """Start a program detached from the app, in its own folder."""
    flags = 0
    if os.name == "nt":
        flags = getattr(subprocess, "DETACHED_PROCESS", 0) | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
    subprocess.Popen([str(exe), *args], cwd=str(exe.parent), creationflags=flags, close_fds=True,
                     stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def find_mods_dir(emu: Path) -> Path | None:
    """Find the folder that holds the per-title mod folders.

    Accepts the emulator folder, its `user`/`load` folder or the title folder
    itself, and honours a custom `load_directory` from `qt-config.ini`.
    """
    if not emu or not emu.exists():
        return None
    if emu.name == TITLE_ID:
        return emu
    if (emu / TITLE_ID).is_dir():
        return emu / TITLE_ID
    load = _load_directory(emu)
    if load is not None:
        return load / TITLE_ID
    for cand in (emu / "user" / "load" / TITLE_ID, emu / "load" / TITLE_ID,
                 emu / "mods" / "contents" / TITLE_ID, emu / "mods" / TITLE_ID):
        if cand.exists():
            return cand
    if (emu / "user").exists():
        return emu / "user" / "load" / TITLE_ID
    return None


def lang_code(value: str) -> str:
    """A language code or name (en-US, es-419, pt-BR, EnglishAmerican...) as one of LANG_VALUES, or ""."""
    t = (value or "").strip().lower().replace("_", "-").replace(" ", "")
    if not t or t.startswith("#"):
        return ""
    if t in LANG_VALUES:
        return t
    head = t.split("-")[0]
    if head in LANG_VALUES:
        return head
    for name, code in LANG_NAMES:
        if t.startswith(name):
            return code
    return ""


def _first_line(path: Path) -> str:
    try:
        for line in path.read_text(errors="replace").splitlines():
            line = line.strip()
            if line and not line.startswith("#"):
                return line
    except OSError:
        pass
    return ""


def detect_lang(emu: Path | None = None, game: Path | None = None) -> str:
    """The language the game is played in, so the mod's own HUD labels match the game's text.

    ReLink / Longshot / Hookshot keep it in language.txt next to the game (or in rc/). The emulators keep it in
    their settings: `language_index` (the console language) in the yuzu family, `system_language` in Ryujinx.
    Empty when it cannot be read: the caller then falls back to the app's own language."""
    if game:
        g = Path(game).expanduser()
        for p in (g / "language.txt", g / "rc" / "language.txt"):
            code = lang_code(_first_line(p))
            if code:
                return code
        # Rekarina keeps it in rekarina.ini ([language] lang = es-419), and is read by this app too.
        for name in ("rekarina.ini", "totk.ini"):
            ini = g / name
            try:
                txt = ini.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            m = re.search(r"^\s*lang\s*=\s*([^\s#]+)", txt, re.M)
            code = lang_code(m.group(1)) if m else ""
            if code:
                return code
            break
    seen: set[Path] = set()
    bases = []
    if emu:
        p = Path(emu).expanduser()
        bases = [p, *list(p.parents)[:4]]
    for base in bases:
        for rel in (Path("user") / "config" / "qt-config.ini", Path("config") / "qt-config.ini"):
            cfg = base / rel
            if cfg in seen or not cfg.exists():
                continue
            seen.add(cfg)
            try:
                txt = cfg.read_text(errors="replace")
            except OSError:
                continue
            m = re.search(r"^\s*language_index\s*=\s*(\d+)\s*$", txt, re.M)
            if m:
                code = LANG_INDEX.get(int(m.group(1)), "")
                if code:
                    return code
            m = re.search(r"^\s*Paths\\language\s*=\s*([^\s#]+)", txt, re.M)
            if m:
                code = lang_code(m.group(1))
                if code:
                    return code
        for rel in (Path("portable") / "Config.json", Path("Config.json")):
            cfg = base / rel
            if cfg in seen or not cfg.exists():
                continue
            seen.add(cfg)
            try:
                data = json.loads(cfg.read_text(errors="replace"))
            except (OSError, ValueError):
                continue
            code = lang_code(str(data.get("system_language", "")))
            if code:
                return code
    return ""


def other_exefs_mods(mods: Path, skip: set[str]) -> list[str]:
    out = []
    if mods.exists():
        for d in sorted(mods.iterdir()):
            if d.name not in skip and (d / "exefs").exists() and any((d / "exefs").glob("*.ips")):
                out.append(d.name)
    return out


def other_layout_mods(mods: Path, skip: set[str]) -> list[str]:
    """Mods that also replace the game's button font or UI atlas (they fight our layout)."""
    out = []
    if mods.exists():
        for d in sorted(mods.iterdir()):
            if d.name in skip:
                continue
            rom = d / "romfs"
            if ((rom / layouts.REL["font"]).exists() or (rom / layouts.REL["atlas"]).exists()):
                out.append(d.name)
    return out


def our_mod_folders(mods: Path) -> list[Path]:
    """Every folder this app installed, including names used by older versions."""
    out = []
    if mods.exists():
        for d in sorted(mods.iterdir()):
            if d.is_dir() and (d / MARKER).exists():
                out.append(d)
    return out


def merge_other_tables(mods: Path, skip: set[str], dst: Path) -> list[str]:
    """Merge the resource size table of the other mods into ours (max per entry).

    A game has a single table; ours is raised for the HUD and the button layout, and another
    mod (a costume pack, the controller UI) raises sizes for its own files. Keeping the
    biggest value of each entry makes both work, whichever mod the loader would pick.
    """
    merged = []
    if not dst.is_file() or not mods.exists():
        return merged
    for d in sorted(mods.iterdir()):
        if d.name in skip or not d.is_dir():
            continue
        other = d / "romfs" / layouts.REL["table"]
        if not other.is_file():
            continue
        try:
            layouts.merge_table(other, dst)
            merged.append(d.name)
        except Exception:
            pass
    return merged


def later_table_mods(mods: Path, name: str) -> list[str]:
    """Mods that sort after ours and bring their own table: the loader would use theirs."""
    out = []
    if mods.exists():
        for d in sorted(mods.iterdir()):
            if d.is_dir() and d.name.lower() > name.lower() and (d / "romfs" / layouts.REL["table"]).is_file():
                out.append(d.name)
    return out


# ---------- IPS32 ----------
def read_ips(data: bytes):
    assert data[:5] == b"IPS32"
    i, out = 5, []
    while data[i:i + 4] != b"EEOF":
        off = struct.unpack(">I", data[i:i + 4])[0]
        size = struct.unpack(">H", data[i + 4:i + 6])[0]
        assert size, "RLE records are not supported"
        out.append((off, data[i + 6:i + 6 + size]))
        i += 6 + size
    return out


def merge_ips(*blobs: bytes) -> bytes:
    taken, recs = {}, []
    for n, blob in enumerate(blobs):
        for off, b in read_ips(blob):
            for k in range(off, off + len(b)):
                if k in taken and taken[k] != n:
                    raise InstallError("ips_overlap", f"main+{k - 0x100:#x}")
                taken[k] = n
            recs.append((off, b))
    out = bytearray(b"IPS32")
    for off, b in recs:
        out += struct.pack(">I", off) + struct.pack(">H", len(b)) + b
    return bytes(out + b"EEOF")


# ---------- config block ----------
def patch_config(nso: bytes, s: Settings) -> bytes:
    i = nso.find(b"SHBCFG01")
    if i < 0 or nso.count(b"SHBCFG01") != 1:
        raise InstallError("bad_module", "config block not found")
    version, size = struct.unpack_from("<II", nso, i + 8)
    if version != 1 or size < 0x48:
        raise InstallError("bad_module", f"config version {version}")
    b = bytearray(nso)
    f = i + 16
    b[f + 0] = int(s.stamina)
    b[f + 1] = s.preset
    b[f + 2], b[f + 3], b[f + 4] = int(s.jump), int(s.attack), int(s.sprint)
    b[f + 5], b[f + 6], b[f + 7] = int(s.block), int(s.shoot), int(s.roll)
    b[f + 8] = int(s.combat_only)
    b[f + 9] = BM_LAYOUT_BYTE.get(s.bm_layout, 0)   # face button positions (was a reserved 0 byte)
    struct.pack_into("<HH", b, f + 10, int(s.enemy_hp), int(s.damage))
    b[f + 14], b[f + 15], b[f + 16] = 0, 0, int(s.clock_24h)
    b[f + 17] = int(s.no_heart_drop)
    struct.pack_into("<H", b, f + 18, int(s.sprint_speed))
    b[f + 20] = int(s.songbook)
    b[f + 21] = int(s.button_mapping)
    b[f + 22] = STYLE_BYTE.get(s.button_style, 0)
    b[f + 23] = int(s.bm_simple)
    b[f + 24] = L_MODE_VALUES.get(s.bm_l_mode, 0)
    b[f + 25] = int(s.quick_boots)
    b[f + 26] = int(s.quick_mask)
    mode = s.hud_mode if s.hud_mode in HUD_MODES else "wide"
    # The HUD layout is four block x positions plus plate/text/icon scales. Wide keeps the user's element
    # switches; simple always shows the four elements in one row behind a soft translucent bar; plain is the
    # same row with no bar at all.
    if mode in ("simple", "plain"):
        pos = [-1685, -1435, -1195, -965]
        rupees = clock = collectibles = True
        sx, sy, ts, isc, plates, maph, hy, ioff, tx, ioff2 = (100, 48, 50, 45, 2 if mode == "simple" else 0,
                                                              0, -1010, -140, 0, -80)
    else:
        rupees, clock, collectibles = s.hud_rupees, s.hud_clock, s.hud_collectibles
        slots = []
        if rupees: slots.append(0)
        if clock: slots.append(1)
        if collectibles: slots += [2, 3]
        # Pack the enabled blocks left to right with an equal gap between plate edges (no hole when off).
        pos = [-32768] * 4
        x = float(HUD_X0)
        for i, block in enumerate(slots):
            pos[block] = int(round(x))
            if i + 1 < len(slots):
                x += (HUD_PLATE_W[block] + HUD_PLATE_W[slots[i + 1]]) * 0.5 + HUD_GAP
        sx, sy, ts, isc, plates, maph, hy, ioff, tx, ioff2 = 46, 62, 0, 85, 1, 0, -960, -165, 30, -165
    b[f + 14], b[f + 15] = int(rupees), int(clock)
    b[f + 27] = int(collectibles)
    struct.pack_into("<4h", b, f + 28, *pos)
    b[f + 36] = int(s.stamina_bar)
    b[f + 37] = 1                       # hud_flags: the layout fields below are valid
    b[f + 38] = HUD_MODE_BYTE[mode]
    b[f + 39], b[f + 40], b[f + 41], b[f + 42] = sx, sy, ts, isc
    b[f + 43], b[f + 44] = plates, maph
    struct.pack_into("<hhh", b, f + 46, hy, ioff, tx)   # struct 62/64/66 (the shorts are 2-byte aligned)
    struct.pack_into("<h", b, f + 52, ioff2)            # struct 68: clock/collectible icon offset
    b[f + 54] = int(s.manual_shield)                    # struct 70: manual shield (was reserved)
    if size >= 0x49:                                    # 73: the new control bytes
        b[f + 55] = int(s.target_r3)                    # struct 71: swap ZL/R3
        b[f + 56] = int(s.sprint_l3)                    # struct 72: L3 sprint
        if size >= 0x4c:                                # 0.6.0 already had this byte as trailing padding
            b[f + 57] = int(s.auto_target)              # struct 73: next target when the current one dies
            b[f + 58] = int(s.switch_target)            # struct 74: switch target with the other shoulder button
            b[f + 59] = LANG_VALUES.get(s.lang, 0)      # struct 75: language of the mod's own HUD labels
            if size >= 0x50:                            # 0.8.0: the two bytes after lang
                b[f + 60] = int(s.no_magic_drop)        # struct 76: magic jars become rupees (romfs)
                b[f + 61] = int(s.hide_minimap)         # struct 77: hide the corner minimap
                b[f + 62] = int(s.sprint_l3_toggle)     # struct 78 (was padding): L3 sprint as a toggle
                if size >= 0x5A and s.bm_remap:         # 0.9.0: button remap
                    roles = normalize_roles(s.bm_roles)
                    if not s.manual_shield:             # the Shield role needs the manual shield
                        roles = {k: ("none" if v == "shield" else v) for k, v in roles.items()}
                    if not s.button_mapping:            # no item slots without the item buttons
                        roles = {k: ("none" if v in ("item1", "item2", "item3", "selector") else v)
                                 for k, v in roles.items()}
                    b[f + 63] = 1                                            # struct 79: bm_remap
                    for k, btn in enumerate(REMAP_BUTTONS[:10]):             # struct 80..89: bm_role[]
                        b[f + 64 + k] = ROLES.index(roles[btn])
                    if size >= 0x5D:                                         # struct 92: bm_role_dr (long press)
                        b[f + 76] = ROLES.index(roles["DR"])
                    # the options the table implies (they used to be separate switches)
                    fl = roles_flags(roles)
                    if s.bm_layout != "gamecube":                             # preserve prior custom-remap geometry
                        b[f + 9] = 0
                    b[f + 24] = 0                                            # bm_l_mode: the selector is a role (VBTN_SELECTOR), not L
                    b[f + 54] = int(s.manual_shield)
                    b[f + 55] = 0                                            # target_r3: target can be on R3 itself
                    b[f + 56] = int(fl["sprint_l3"])
    if size >= 0x5C:                                    # struct 90 / 91 (the old tail padding): HUD scales
        b[f + 74] = hud_scale(s.hud_ctrl_scale)
        b[f + 75] = hud_scale(s.hud_count_scale)
    return bytes(b)


HUD_SCALE_MIN, HUD_SCALE_MAX, HUD_SCALE_STEP = 50, 200, 5


def hud_scale(value) -> int:
    """A HUD scale percent clamped to the slider's range (the module reads 0 as 100)."""
    try:
        v = int(round(float(value)))
    except (TypeError, ValueError):
        return 100
    return max(HUD_SCALE_MIN, min(HUD_SCALE_MAX, v))


def set_installed_lang(mods: Path, name: str, code: str) -> bool:
    """Patch only the HUD-language byte of an already installed module, so changing the language in the app
    does not need a reinstall. It takes effect the next time the game starts."""
    mod = mods / name / "exefs" / "subsdk9"
    try:
        data = bytearray(mod.read_bytes())
    except OSError:
        return False
    i = data.find(b"SHBCFG01")
    if i < 0:
        return False
    size = struct.unpack_from("<I", data, i + 12)[0]
    if size < 0x4C or i + size > len(data):
        return False
    data[i + 75] = LANG_VALUES.get(code, 0)   # struct 75 (the magic is at struct 0)
    try:
        mod.write_bytes(bytes(data))
    except OSError:
        return False
    return True


# ---------- enemy speed components ----------
def enemy_levels(s: Settings) -> tuple[int, int, int, int]:
    """Selected movement / animation / attack / projectile levels. The old enemy_speed bool means the
    original three components x2 (projectiles stay off, as they were not part of that option)."""
    levels = tuple(int(v) if int(v) in ENEMY_FACTORS else 0
                   for v in (s.enemy_move, s.enemy_anim, s.enemy_attack, s.enemy_proj))
    if s.enemy_speed and not any(levels):
        return (2, 2, 2, 0)
    return levels


def _copy_tree(src: Path, dst: Path) -> bool:
    if not src.exists():
        return False
    shutil.copytree(src, dst, dirs_exist_ok=True)
    return True


# Aggression (enemy_aggro): what each level does to the manifest's aggression fields (tools/make_enemy_opts.py).
#   g  the shared pause during which only one enemy attacks Link (x scale)
#   k  how far an enemy keeps chasing before it gives up (x scale)
#   i  how long a melee enemy waits on screen before it fights (x scale)
#   o  "wait until on screen before fighting" (False when off_screen)
AGGRO_LEVELS = (0, 1, 2)
AGGRO = {1: dict(g=0.5, k=1.5, i=1.0, off_screen=False),
         2: dict(g=0.0, k=2.0, i=0.0, off_screen=True)}


def _patch_packs(opts: Path, romfs: Path, base_name: str, manifest_name: str,
                 move: int, attack: int, proj: int, alert: int = 0, aggro: int = 0) -> bool:
    """Write the base packs of a manifest with the movement (m), attack cadence (a) and projectile (p) floats
    scaled, and the aggression fields: alert radius (d) and the aggression level (g / k / i / o). False when the
    payload part is missing or unreadable."""
    base = opts / base_name
    manifest_path = opts / manifest_name
    dictionary_path = opts / "enemy_speed_pack.zsdic"
    if not (base.is_dir() and manifest_path.exists() and dictionary_path.exists()):
        return False
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        dictionary = zstandard.ZstdCompressionDict(dictionary_path.read_bytes())
        move_scale = ENEMY_FACTORS.get(move, 1.0)
        attack_scale = 1.0 / ENEMY_FACTORS[attack] if attack else 1.0
        proj_scale = ENEMY_FACTORS.get(proj, 1.0)
        ag = AGGRO.get(aggro)
        scales = {"m": move_scale, "a": attack_scale, "p": proj_scale, "d": ENEMY_FACTORS.get(alert, 1.0),
                  "g": ag["g"] if ag else 1.0, "k": ag["k"] if ag else 1.0, "i": ag["i"] if ag else 1.0}
        off_screen = bool(ag and ag["off_screen"])
        active = {k for k, v in scales.items() if v != 1.0} | ({"o"} if off_screen else set())
        for pack in manifest.get("packs", []):
            fields = pack.get("fields", [])
            if not any(f.get("k") in active for f in fields):
                continue
            raw = (base / pack["path"]).read_bytes()
            data = bytearray(zstandard.ZstdDecompressor(dict_data=dictionary).decompress(
                raw, max_output_size=200_000_000))
            for f in fields:
                kind = f.get("k")
                if kind == "o":                       # a bool: stored inline as a u32
                    if off_screen:
                        struct.pack_into("<I", data, int(f["o"]), 0)
                    continue
                scale = scales.get(kind, 1.0)
                if scale != 1.0:
                    struct.pack_into("<f", data, int(f["o"]), float(f["v"]) * scale)
            out = zstandard.ZstdCompressor(dict_data=dictionary).compress(bytes(data))
            dst = romfs / Path(pack["path"])
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(out)
    except (OSError, ValueError, KeyError, zstandard.ZstdError):
        return False
    return True


def boss_levels(s: Settings) -> tuple[int, int]:
    return tuple(int(v) if int(v) in ENEMY_FACTORS else 0 for v in (s.boss_move, s.boss_anim))


def install_enemy_mods(romfs: Path, s: Settings, payload: Path | None = None) -> list[tuple[str, str]]:
    """Install the selected enemy-speed components (patched base packs + one animation folder), and the bosses'
    own movement / animation (separate files: bosses and minibosses are left out of the enemy options)."""
    move, anim, attack, proj = enemy_levels(s)
    bmove, banim = boss_levels(s)
    alert = int(s.enemy_alert) if int(s.enemy_alert) in ENEMY_FACTORS else 0
    aggro = int(s.enemy_aggro) if int(s.enemy_aggro) in AGGRO else 0
    if not (move or anim or attack or proj or bmove or banim or alert or aggro):
        return []
    opts = (payload or resource_dir()) / "romfs_opts"
    notes: list[tuple[str, str]] = []
    missing = False

    if move or attack or proj or alert or aggro:
        if not _patch_packs(opts, romfs, "enemy_speed_base", "enemy_speed_manifest.json", move, attack, proj,
                            alert, aggro):
            missing = True
    if anim and not _copy_tree(opts / ENEMY_ANIM_DIR[anim], romfs):
        missing = True
    if bmove and not _patch_packs(opts, romfs, "boss_speed_base", "boss_speed_manifest.json", bmove, 0, 0):
        missing = True
    if banim and not _copy_tree(opts / BOSS_ANIM_DIR[banim], romfs):
        missing = True

    if missing:
        notes.append(("enemy_speed_missing", ""))
    return notes


# ---------- install ----------
@dataclass
class Result:
    target: Path
    variant: str
    notes: list = field(default_factory=list)      # (key, arg) pairs, localized by the window


def ensure_layout(s: "Settings", progress=None) -> str | None:
    """Download (once) the game-wide controller layout the user chose; None when there is none (optional)."""
    style = s.game_layout if s.game_layout in GAME_LAYOUTS else "none"
    if style == "none":
        return None
    try:
        layouts.ensure(style, progress=progress)
    except layouts.LayoutError as ex:
        raise InstallError("layout", style) from ex
    return style


def prepare_gamecube_layout(s: Settings, root: Path, payload: Path) -> tuple[bytes | None, Path | None, tuple | None]:
    """GameCube extras: the patched UI atlas (the Basic Controls diagram) and the button art directory.

    The diagram needs the game's original UI archive, which an emulator install does not have on disk, so a
    missing or unsupported archive skips the diagram (third value = note) instead of failing the install: the
    HUD and the button art do not depend on it.
    """
    if s.bm_layout != "gamecube":
        return None, None, None
    art = None
    if s.button_mapping and s.button_style not in ("xbox", "playstation") and not s.bm_simple:
        art = payload / "button_romfs" / "gamecube"
        if not art.is_dir():
            raise InstallError("no_payload", "buttonMapping (gamecube)")
    try:
        return gamecube.make_atlas(root), art, None
    except ValueError as ex:
        return None, art, ("gamecube_atlas_skipped", str(ex))


def install(emu: Path, s: Settings, name: str = MOD_NAME, progress=None) -> Result:
    mods = find_mods_dir(emu)
    if mods is None:
        raise InstallError("no_mods_dir")
    style = None if s.bm_layout == "gamecube" else ensure_layout(s, progress)
    variant = "standalone"
    pay = resource_dir() / variant
    if not (pay / "subsdk9").exists():
        raise InstallError("no_payload", variant)
    atlas, gamecube_art, gc_note = prepare_gamecube_layout(s, emu, pay.parent)
    target = mods / name
    mods.mkdir(parents=True, exist_ok=True)
    for old in our_mod_folders(mods):        # older versions used other folder names (00-, Super-Hero-Boost)
        if old.name != name:
            shutil.rmtree(old, ignore_errors=True)
    if target.exists():
        if not (target / MARKER).exists():
            raise InstallError("exists_not_ours", str(target))
        try:
            shutil.rmtree(target)
        except OSError:
            raise InstallError("in_use", str(target))
    (target / "exefs").mkdir(parents=True)
    (target / MARKER).write_text(json.dumps({"variant": variant, **s.to_dict()}))

    (target / "exefs" / "subsdk9").write_bytes(patch_config((pay / "subsdk9").read_bytes(), s))
    (target / "exefs" / f"{BUILD_ID}.ips").write_bytes((pay / "ours.ips").read_bytes())

    if s.button_mapping:   # buttonMapping: the button textures of the chosen style + a bigger UI budget
        rep = pay.parent / "button_romfs"
        plat = s.button_style if s.button_style in BUTTON_STYLES else "switch"
        if not (rep / plat).exists():
            raise InstallError("no_payload", "buttonMapping")
        shutil.copytree(rep / plat, target / "romfs", dirs_exist_ok=True)
        if (rep / "common").exists():
            shutil.copytree(rep / "common", target / "romfs", dirs_exist_ok=True)

    if gamecube_art is not None:
        _copy_tree(gamecube_art, target / "romfs")
    if atlas is not None:
        gamecube.write_atlas(target / "romfs", atlas)
    if s.bm_layout == "gamecube":
        gamecube.disable_legacy_atlas(mods)   # a standalone diagram add-on would override ours

    res = Result(target, variant)
    if gc_note:
        res.notes.append(gc_note)
    # romfs files generated from the game's own data (tools/make_romfs_opts.py). The heart and the magic drop are
    # two changes to the same table, so when both are on one combined file is installed.
    drops = (["no_heart_drop"] if s.no_heart_drop else []) + (["no_magic_drop"] if s.no_magic_drop else [])
    if len(drops) == 2:
        drops = ["no_heart_magic_drop"]
    for opt in drops:
        src = pay.parent / "romfs_opts" / opt
        if src.exists():
            shutil.copytree(src, target / "romfs", dirs_exist_ok=True)
        else:
            # The option needs a file generated from the user's own game dump. Never block the
            # rest of the installation just because this optional file was not built.
            missing = ("no_heart_drop_missing", "no_magic_drop_missing") if opt == "no_heart_magic_drop" \
                else (opt + "_missing",)
            for key in missing:
                res.notes.append((key, ""))

    for opt, enabled in (("enemy_double", s.enemy_double),
                         ("farore_outdoor", s.farore_outdoor),
                         ("great_fairy", s.great_fairy)):
        if not enabled:
            continue
        src = pay.parent / "romfs_opts" / opt
        if src.exists():
            shutil.copytree(src, target / "romfs", dirs_exist_ok=True)
        else:
            res.notes.append((opt + "_missing", ""))
    res.notes.extend(install_enemy_mods(target / "romfs", s, pay.parent))
    if style:   # last: the layout table is merged over the final one (font 0x5000 keeps our layout/enemy entries)
        layouts.apply(target / "romfs", style)
        if not layouts.font_ready(style):
            # The layout's own font aborts this build; only the patched one (payload) is installed.
            res.notes.append(("font_missing", style))
    merged = merge_other_tables(mods, {target.name}, target / "romfs" / layouts.REL["table"])
    if merged:
        res.notes.append(("tables_merged", ", ".join(merged)))
    later = later_table_mods(mods, target.name)
    if later:
        res.notes.append(("table_later", ", ".join(later)))

    conflicts = other_exefs_mods(mods, {name})
    if conflicts:
        res.notes.append(("conflicts", ", ".join(conflicts)))
    fighting = other_layout_mods(mods, {name})
    if fighting:
        res.notes.append(("layout_mods", ", ".join(fighting)))
    return res


def uninstall(emu: Path, name: str = MOD_NAME) -> bool:
    mods = find_mods_dir(emu)
    if mods is None:
        return False
    gone = False
    for d in our_mod_folders(mods):
        try:
            shutil.rmtree(d)
            gone = True
        except OSError:
            pass
    return gone
