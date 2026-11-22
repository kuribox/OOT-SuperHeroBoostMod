"""Super Hero Boost - configuration and installer window.

The mod is prebuilt and reads its options when the game loads; this app edits
those options, remembers them and installs the mod into an emulator or into a
Hookshot/Longshot recompiled game (the latter rebuilds the game, with a progress
bar).

Headless use (tests):
    SHB-OcarinaTweaks --cli --emu <folder> [--profile hero] [--preset 2] ...
"""
import argparse
import json
import os
import queue
import re
import sys
import threading
from pathlib import Path

import customtkinter as ctk

sys.path.insert(0, str(Path(__file__).resolve().parent))
import installer as I  # noqa: E402
import ui_kit as UI  # noqa: E402
import layouts  # noqa: E402
import remap_ui as RM  # noqa: E402
import texts_enemies  # noqa: E402,F401  (texts of the aggression options)
import paths  # noqa: E402
import recomp as R  # noqa: E402
import rekarina as RK  # noqa: E402
import updater as U  # noqa: E402
from i18n import LANGS, tr, system_lang  # noqa: E402
from presets import (DEFAULT_PROFILE, KEYS, PROFILES, PROFILE_KEYS,  # noqa: E402
                     STAMINA_MODES, profile_values, settings_kwargs)
from version import VERSION  # noqa: E402


# ---------------------------------------------------------------- settings file
def load_cfg() -> dict:
    for p in (*paths.settings_paths(), *paths.legacy_settings_paths()):
        try:
            data = json.loads(p.read_text(encoding="utf-8"))
            if isinstance(data, dict):
                return data
        except Exception:
            continue
    return {}


def save_cfg(d: dict) -> str:
    for p in paths.settings_paths():
        try:
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(json.dumps(d, indent=1, ensure_ascii=False), encoding="utf-8")
            return str(p)
        except OSError:
            continue
    return ""


# ---------------------------------------------------------------- command line
def cli(argv) -> int:
    ap = argparse.ArgumentParser(description="SHB-OcarinaTweaks installer (headless mode)")
    ap.add_argument("--cli", action="store_true")
    ap.add_argument("--emu", help="emulator folder (the one that contains 'user')")
    ap.add_argument("--profile", choices=PROFILE_KEYS, default="custom",
                    help="apply a complete game profile before the individual options")
    ap.add_argument("--list-profiles", action="store_true", help="print the profile keys and exit")
    ap.add_argument("--preset", type=int, choices=range(4), default=None,
                    help="stamina style: 0 hardcore, 1 combat only, 2 sprint only, 3 advanced")
    ap.add_argument("--no-button-mapping", action="store_true", help="no extra item buttons (button mapping)")
    ap.add_argument("--button-style", choices=I.BUTTON_STYLES, default=None,
                    help="button mapping: button pictures")
    ap.add_argument("--bm-layout", choices=I.BM_LAYOUTS, default=None,
                    help="button mapping: face button positions (including Switch GameCube)")
    ap.add_argument("--game-layout", choices=I.GAME_LAYOUTS, default=None,
                    help="download the game's own PlayStation/Xbox prompts (optional; none keeps the game's)")
    ap.add_argument("--bm-simple", action="store_true", help="button mapping: simple layout (assignable buttons only)")
    ap.add_argument("--bm-l", choices=I.L_MODES, default=None,
                    help="button mapping: L is an item button (assign) or the game's item selector")
    ap.add_argument("--no-stamina", action="store_true")
    ap.add_argument("--no-songbook", action="store_true", help="disable playing songs from the native songbook")
    ap.add_argument("--quick-boots", action="store_true", help="hold D-pad down to switch boots")
    ap.add_argument("--quick-mask", action="store_true", help="hold D-pad up to switch masks")
    ap.add_argument("--manual-shield", action="store_true",
                    help="ZL follows the game's Hold/Toggle setting; L holds the shield while targeting")
    ap.add_argument("--no-manual-shield", action="store_true", help="disable the manual shield of a profile")
    ap.add_argument("--target-r3", action="store_true",
                    help="swap ZL and R3: target with R3 (recommended with Toggle targeting)")
    ap.add_argument("--sprint-l3", action="store_true", help="L3 (left stick click) holds A while pressed (sprint)")
    ap.add_argument("--auto-target", action="store_true",
                    help="when the target dies, press target again to lock on to the next enemy")
    ap.add_argument("--switch-target", action="store_true",
                    help="the other shoulder button switches to the next target while targeting")
    ap.add_argument("--enemy-double", action="store_true", help="beta: duplicate generic enemies (romfs)")
    ap.add_argument("--enemy-speed", action="store_true", help="deprecated: enemy movement/animation/attack at x2")
    ap.add_argument("--enemy-move", type=int, choices=I.ENEMY_LEVELS, default=None,
                    help="enemy movement: 0 off, 1 x1.5, 2 x2, 3 x3, 4 x1.25, 5 x1.75 (see the app for the rest)")
    ap.add_argument("--enemy-anim", type=int, choices=I.ENEMY_LEVELS, default=None,
                    help="enemy animation: same levels as --enemy-move")
    ap.add_argument("--enemy-attack", type=int, choices=I.ENEMY_LEVELS, default=None,
                    help="enemy attack cadence: same levels as --enemy-move")
    ap.add_argument("--enemy-proj", type=int, choices=I.ENEMY_LEVELS, default=None,
                    help="enemy projectile speed: same levels as --enemy-move")
    ap.add_argument("--enemy-alert", type=int, choices=I.ENEMY_LEVELS, default=None,
                    help="how far a spotting enemy alerts the others: same levels as --enemy-move")
    ap.add_argument("--enemy-aggro", type=int, choices=I.AGGRO_LEVELS, default=None,
                    help="enemy aggression: 0 normal, 1 aggressive, 2 maximum")
    ap.add_argument("--sprint-l3-toggle", action="store_true",
                    help="with --sprint-l3: L3 turns the sprint on / off (stops when stamina runs out or A is pressed)")
    ap.add_argument("--boss-move", type=int, choices=I.ENEMY_LEVELS, default=None,
                    help="boss and miniboss movement: same levels as --enemy-move")
    ap.add_argument("--boss-anim", type=int, choices=I.ENEMY_LEVELS, default=None,
                    help="boss and miniboss animation: same levels as --enemy-move")
    ap.add_argument("--stamina-bar", action="store_true", help="draw stamina as a segmented horizontal bar instead of the ring")
    ap.add_argument("--great-fairy", action="store_true", help="reorder the three Great Fairy magic rewards (romfs)")
    ap.add_argument("--farore-outdoor", action="store_true", help="allow Farore's Wind points outside dungeons (romfs)")
    ap.add_argument("--hud-collectibles", action="store_true", help="per-area Skulltula / heart-piece counter")
    ap.add_argument("--hud-mode", choices=I.HUD_MODES, default=None,
                    help="HUD layout: wide (plates), simple (one small bar), map (hearts on the Minus map)")
    ap.add_argument("--off", default=None, help="advanced: comma list of jump,attack,sprint,block,shoot,roll to disable")
    ap.add_argument("--combat-only", action="store_true", default=None)
    ap.add_argument("--name", default=I.MOD_NAME)
    ap.add_argument("--lang", choices=tuple(LANGS), default=None,
                    help="language of the mod's own HUD labels (default: the game's language, then the system's)")
    ap.add_argument("--enemy-hp", type=int, default=None, help="enemy health, percent (150 = Hero Mode)")
    ap.add_argument("--damage", type=int, default=None, help="damage taken, percent (200 = Hero Mode)")
    ap.add_argument("--no-heart-drop", action="store_true")
    ap.add_argument("--no-magic-drop", action="store_true", help="dropped magic jars become green rupees (romfs)")
    ap.add_argument("--hide-minimap", action="store_true", help="hide the minimap while playing")
    ap.add_argument("--sprint-speed", type=int, default=None, help="sprint speed, percent (125 = default)")
    ap.add_argument("--rupees", action="store_true", help="always-visible rupee counter")
    ap.add_argument("--clock", action="store_true", help="always-visible clock")
    ap.add_argument("--clock24", action="store_true")
    ap.add_argument("--hud-ctrl-scale", type=int, default=None,
                    help="size of the button cluster HUD, percent (%d..%d, 100 = default)" % (I.HUD_SCALE_MIN, I.HUD_SCALE_MAX))
    ap.add_argument("--hud-count-scale", type=int, default=None,
                    help="size of the counter row HUD (rupees, clock, collectibles), percent (100 = default)")
    a = ap.parse_args(argv)

    if a.list_profiles:
        from i18n import T
        for key in PROFILE_KEYS:
            token = "profile_custom" if key == "custom" else "p_" + key.replace("-", "_")
            name = T["en"].get(token, "").replace("★", "*")
            print(f"{key:13s} {name}")
        return 0
    if not a.emu:
        ap.error("--emu is required")

    s = I.Settings(**settings_kwargs(a.profile))
    if a.off is not None:
        for k in a.off.split(","):
            if k in KEYS:
                setattr(s, k, False)
    s.lang = a.lang or I.detect_lang(Path(a.emu).expanduser()) or system_lang()
    if a.preset is not None:
        s.preset = a.preset
    if a.no_button_mapping:
        s.button_mapping = False
    if a.button_style:
        s.button_style = a.button_style
    if a.bm_layout:
        s.bm_layout = a.bm_layout
    if a.game_layout:
        s.game_layout = a.game_layout
    if a.bm_simple:
        s.bm_simple = True
    if a.bm_l:
        s.bm_l_mode = a.bm_l
    if a.no_stamina:
        s.stamina = False
    if a.no_songbook:
        s.songbook = False
    if a.quick_boots:
        s.quick_boots = True
    if a.quick_mask:
        s.quick_mask = True
    if a.manual_shield:
        s.manual_shield = True
    if a.no_manual_shield:
        s.manual_shield = False
    if a.target_r3:
        s.target_r3 = True
    if a.sprint_l3:
        s.sprint_l3 = True
    if a.sprint_l3_toggle:
        s.sprint_l3_toggle = True
    if a.auto_target:
        s.auto_target = True
    if a.switch_target:
        s.switch_target = True
    s.bm_roles = I.roles_from_flags(s.bm_roles, manual_shield=s.manual_shield, target_r3=s.target_r3,
                                    sprint_l3=s.sprint_l3, l_selector=s.bm_l_mode == "selector")
    if a.enemy_double:
        s.enemy_double = True
    if a.enemy_speed:
        s.enemy_speed = True
    if a.enemy_move is not None:
        s.enemy_move = a.enemy_move
    if a.enemy_anim is not None:
        s.enemy_anim = a.enemy_anim
    if a.enemy_attack is not None:
        s.enemy_attack = a.enemy_attack
    if a.enemy_proj is not None:
        s.enemy_proj = a.enemy_proj
    if a.enemy_alert is not None:
        s.enemy_alert = a.enemy_alert
    if a.enemy_aggro is not None:
        s.enemy_aggro = a.enemy_aggro
    if a.boss_move is not None:
        s.boss_move = a.boss_move
    if a.boss_anim is not None:
        s.boss_anim = a.boss_anim
    if a.stamina_bar:
        s.stamina_bar = True
    if a.great_fairy:
        s.great_fairy = True
    if a.farore_outdoor:
        s.farore_outdoor = True
    if a.hud_collectibles:
        s.hud_collectibles = True
    if a.hud_mode:
        s.hud_mode = a.hud_mode
    if a.combat_only:
        s.combat_only = True
    if a.enemy_hp is not None:
        s.enemy_hp = a.enemy_hp
    if a.damage is not None:
        s.damage = a.damage
    if a.sprint_speed is not None:
        s.sprint_speed = a.sprint_speed
    if a.hud_ctrl_scale is not None:
        s.hud_ctrl_scale = I.hud_scale(a.hud_ctrl_scale)
    if a.hud_count_scale is not None:
        s.hud_count_scale = I.hud_scale(a.hud_count_scale)
    if a.no_heart_drop:
        s.no_heart_drop = True
    if a.no_magic_drop:
        s.no_magic_drop = True
    if a.hide_minimap:
        s.hide_minimap = True
    if a.rupees:
        s.hud_rupees = True
    if a.clock:
        s.hud_clock = True
    if a.clock24:
        s.clock_24h = True

    try:
        last = [-1]

        def cli_progress(done, total):
            if not total:
                return
            step = done * 5 // total          # 20% steps
            if step != last[0]:
                last[0] = step
                print("  layout download: %d%%" % min(100, done * 100 // total), flush=True)

        r = I.install(Path(a.emu), s, a.name, progress=cli_progress)
    except I.InstallError as e:
        print("error:", e.key, e.arg)
        return 1
    print("installed in", r.target, "variant", r.variant)
    for k, arg in r.notes:
        print(" -", k, arg)
    return 0


# ---------------------------------------------------------------- window
BG = UI.BG
SIDEBAR = UI.SIDEBAR
PANEL = UI.PANEL
PANEL2 = UI.PANEL2
BORDER = UI.BORDER
ACCENT = UI.ACCENT
ACCENT2 = UI.ACCENT2
ACCENT_DARK = UI.ACCENT_DARK
TEAL = UI.TEAL
AMBER = UI.AMBER
TEXT = UI.TEXT
DIM = UI.DIM
RED = UI.RED
COLORS = dict(PANEL=PANEL, PANEL2=PANEL2, BORDER=BORDER, ACCENT=ACCENT, ACCENT_DARK=ACCENT_DARK, AMBER=AMBER,
              TEXT=TEXT, DIM=DIM, TEAL=TEAL)


ScrollArea = UI.ScrollArea



def gui() -> None:
    import tkinter as tk
    import tkinter.font as tkfont
    from tkinter import filedialog, messagebox

    if sys.platform == "win32":                    # crisp text on scaled displays
        try:
            import ctypes
            ctypes.windll.shcore.SetProcessDpiAwareness(1)
        except Exception:
            pass

    cfg = load_cfg()
    UI.setup()
    root = ctk.CTk()
    root.title(f"SHB-OcarinaTweaks {VERSION}")
    root.configure(fg_color=BG)
    root.geometry("1320x840")
    root.minsize(1180, 700)
    try:
        base = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent))
        icon = tk.PhotoImage(file=str(base / "assets" / "shb.png"))
        root.iconphoto(True, icon)
        root._shb_icon = icon
        if sys.platform == "win32":
            ico = base / "assets" / "shb.ico"
            if ico.is_file():
                root.iconbitmap(default=str(ico))
    except Exception:
        pass

    fams = set(tkfont.families())
    FAM = next((f for f in ("Segoe UI", "SF Pro Text", "Helvetica Neue", "DejaVu Sans", "Arial") if f in fams), "TkDefaultFont")
    DISPLAY = next((f for f in ("Palatino Linotype", "Book Antiqua", "Georgia", "Cambria", "DejaVu Serif",
                                "Times New Roman") if f in fams), FAM)
    UI.set_fonts(FAM, DISPLAY)

    lang = tk.StringVar(value=cfg.get("lang") if cfg.get("lang") in LANGS else system_lang())
    emu = tk.StringVar(value=cfg.get("emu", ""))
    target = tk.StringVar(value=cfg.get("target") if cfg.get("target") in ("emulator", "recomp", "rekarina")
                          else "emulator")
    recomp_path = tk.StringVar(value=cfg.get("recomp", ""))
    recomp_builder = tk.StringVar(value=cfg.get("recomp_builder", ""))
    rekarina_path = tk.StringVar(value=cfg.get("rekarina", ""))
    busy = {"on": False}
    profile = tk.StringVar(value=cfg.get("profile") if cfg.get("profile") in PROFILE_KEYS else "custom")
    bmap = tk.BooleanVar(value=bool(cfg.get("button_mapping", True)))
    # button style, layout and L mode: personal choices, not part of a profile
    bstyle = tk.StringVar(value=cfg.get("button_style") if cfg.get("button_style") in I.BUTTON_STYLES else "switch")
    bplace = tk.StringVar(value=cfg.get("bm_layout") if cfg.get("bm_layout") in I.BM_LAYOUTS else "nintendo")
    # the game-wide prompts used to follow the button style: keep that choice for configs saved before the split
    _gl = cfg.get("game_layout", cfg.get("button_style") if cfg.get("button_style") in I.GAME_LAYOUTS else "none")
    glayout = tk.StringVar(value=_gl if _gl in I.GAME_LAYOUTS else "none")
    bsimple = tk.BooleanVar(value=bool(cfg.get("bm_simple", False)))
    blmode = tk.StringVar(value=cfg.get("bm_l_mode") if cfg.get("bm_l_mode") in I.L_MODES else "assign")
    # Button remap (app/remap_ui.py): the table of what each button does. The manual shield, the L3 sprint, target
    # on R3 and L as the item selector are roles in it now; their old switches are kept in step with it (profiles,
    # the command line and older settings files still speak in those switches).
    if isinstance(cfg.get("bm_roles"), dict):
        broles = RM.role_vars(cfg.get("bm_roles"))
    else:   # settings from before the remap: the table that does what the old switches did
        broles = RM.role_vars(I.roles_from_flags(
            I.CLASSIC_ROLES, manual_shield=bool(cfg.get("manual_shield", False)),
            target_r3=bool(cfg.get("target_r3", False)), sprint_l3=bool(cfg.get("sprint_l3", False)),
            l_selector=cfg.get("bm_l_mode") == "selector"))
    stamina = tk.BooleanVar(value=bool(cfg.get("stamina", True)))
    stamina_bar = tk.BooleanVar(value=bool(cfg.get("stamina_bar", False)))
    songbook = tk.BooleanVar(value=bool(cfg.get("songbook", True)))
    quick_boots = tk.BooleanVar(value=bool(cfg.get("quick_boots", False)))
    quick_mask = tk.BooleanVar(value=bool(cfg.get("quick_mask", False)))
    manual_shield = tk.BooleanVar(value=bool(cfg.get("manual_shield", False)))
    target_r3 = tk.BooleanVar(value=bool(cfg.get("target_r3", False)))
    sprint_l3 = tk.BooleanVar(value=bool(cfg.get("sprint_l3", False)))
    sprint_l3_toggle = tk.BooleanVar(value=bool(cfg.get("sprint_l3_toggle", False)))
    auto_target = tk.BooleanVar(value=bool(cfg.get("auto_target", False)))
    switch_target = tk.BooleanVar(value=bool(cfg.get("switch_target", False)))
    hud_collectibles = tk.BooleanVar(value=bool(cfg.get("hud_collectibles", False)))
    _hm = cfg.get("hud_mode")
    hud_mode = tk.StringVar(value=_hm if _hm in I.HUD_MODES else "wide")
    enemy_double = tk.BooleanVar(value=bool(cfg.get("enemy_double", False)))
    _legacy_speed = 2 if cfg.get("enemy_speed") else 0
    enemy_move = tk.IntVar(value=cfg.get("enemy_move", _legacy_speed)
                           if cfg.get("enemy_move", _legacy_speed) in I.ENEMY_LEVELS else 0)
    enemy_anim = tk.IntVar(value=cfg.get("enemy_anim", _legacy_speed)
                           if cfg.get("enemy_anim", _legacy_speed) in I.ENEMY_LEVELS else 0)
    enemy_attack = tk.IntVar(value=cfg.get("enemy_attack", _legacy_speed)
                             if cfg.get("enemy_attack", _legacy_speed) in I.ENEMY_LEVELS else 0)
    enemy_proj = tk.IntVar(value=cfg.get("enemy_proj", 0)
                           if cfg.get("enemy_proj", 0) in I.ENEMY_LEVELS else 0)
    enemy_alert = tk.IntVar(value=cfg.get("enemy_alert", 0) if cfg.get("enemy_alert", 0) in I.ENEMY_LEVELS else 0)
    enemy_aggro = tk.IntVar(value=cfg.get("enemy_aggro", 0) if cfg.get("enemy_aggro", 0) in I.AGGRO_LEVELS else 0)
    boss_move = tk.IntVar(value=cfg.get("boss_move", 0) if cfg.get("boss_move", 0) in I.ENEMY_LEVELS else 0)
    boss_anim = tk.IntVar(value=cfg.get("boss_anim", 0) if cfg.get("boss_anim", 0) in I.ENEMY_LEVELS else 0)
    great_fairy = tk.BooleanVar(value=bool(cfg.get("great_fairy", False)))
    farore_outdoor = tk.BooleanVar(value=bool(cfg.get("farore_outdoor", False)))
    mode = tk.StringVar(value=cfg.get("stamina_mode") if cfg.get("stamina_mode") in STAMINA_MODES
                        else STAMINA_MODES[cfg.get("preset", 0)] if cfg.get("preset", 0) in range(4) else "hardcore")

    saved_costs = cfg.get("costs")
    if not isinstance(saved_costs, dict):
        saved_costs = cfg.get("custom") if isinstance(cfg.get("custom"), dict) else {}
    costs = {k: tk.BooleanVar(value=bool(saved_costs.get(k, True))) for k in KEYS}
    combat = tk.BooleanVar(value=bool(saved_costs.get("combat_only", cfg.get("combat_only", False))))
    MULT = (100, 150, 200, 300)
    hp = tk.IntVar(value=cfg.get("enemy_hp", 100) if cfg.get("enemy_hp", 100) in MULT else 100)
    dmg = tk.IntVar(value=cfg.get("damage", 100) if cfg.get("damage", 100) in MULT else 100)
    SPEEDS = (100, 125, 150, 175, 200, 250, 300)
    speed = tk.IntVar(value=cfg.get("sprint_speed", 125) if cfg.get("sprint_speed", 125) in SPEEDS else 125)
    nodrop = tk.BooleanVar(value=bool(cfg.get("no_heart_drop", False)))
    nmagic = tk.BooleanVar(value=bool(cfg.get("no_magic_drop", False)))
    hide_min = tk.BooleanVar(value=bool(cfg.get("hide_minimap", False)))
    rupees = tk.BooleanVar(value=bool(cfg.get("hud_rupees", False)))
    hud_ctrl_scale = tk.IntVar(value=I.hud_scale(cfg.get("hud_ctrl_scale", 100)))
    hud_count_scale = tk.IntVar(value=I.hud_scale(cfg.get("hud_count_scale", 100)))
    CLOCK_MODES = ("off", "12", "24")
    _clock = cfg.get("clock_mode")
    if _clock not in CLOCK_MODES:
        _clock = "24" if cfg.get("clock_24h") else "12" if cfg.get("hud_clock") else "off"
    clock_mode = tk.StringVar(value=_clock)

    status = tk.StringVar(value="")
    guard = {"on": False}
    ui = {"nav": {}, "sections": {}}
    state = {"section": "install", "update": None, "update_dismissed": "", "update_checking": False}

    L = lambda key, **kw: tr(lang.get(), key, **kw)  # noqa: E731
    UI.HELP_TITLE = L("help_title")

    # ---- small widget helpers ----
    def button(parent, text, cmd, primary=False, **kw):
        return UI.Button(parent, text, cmd, primary=primary, **kw)

    def label(parent, text, size=10, color=TEXT, bold=False, italic=False, wrap=760, **kw):
        return UI.Label(parent, text, size=size, fg=color, bold=bold, italic=italic, wrap=wrap,
                        bg=kw.pop("bg", PANEL), **kw)

    def desc_label(parent, text, indent=20, pady=(0, 0), wrap=1000):
        """An option's explanation: one line, with the rest behind a quiet "?"."""
        first, rest = first_sentence(text)
        row = UI.Frame(parent, bg=PANEL)
        row.pack(anchor="w", padx=(indent, 0), pady=pady)
        label(row, first, size=9, color=DIM, bg=PANEL, wrap=wrap).pack(side="left")
        if rest:
            RM.help_icon(row, text, COLORS, FAM).pack(side="left", padx=(6, 0))
        return row

    def rb_tag(parent):
        """'Requires rebuild' tag: options stored in the module's configuration (subsdk9). Shown only for a
        recompiled game (Longshot, Hookshot, ReLink): their exefs is translated into the program, so a change
        there needs the builder. Romfs options never carry it. refresh() sets its text."""
        t = UI.Label(parent, "", size=9, fg=RED, bold=True, bg=PANEL)
        ui.setdefault("rb_tags", []).append(t)
        return t

    def check_rb(parent, text, var, **pack):
        """A feature's main switch: turning it on or off needs the builder on a recompiled game (its sub-options
        do not), so the 'requires rebuild' tag sits beside it."""
        row = UI.Frame(parent, bg=PANEL)
        row.pack(anchor="w", **pack)
        c = check(row, text, var)
        c.pack(side="left")
        rb_tag(row).pack(side="left", padx=(10, 0))
        return c

    def first_sentence(text):
        """One-line card summaries; the rest of a long explanation goes to the inline help strip."""
        text = (text or "").strip()
        m = re.search(r"(?<=[.!?])\s+", text)
        if not m:
            return text, ""
        return text[:m.start()].strip(), text[m.end():].strip()

    def card(parent, title, desc=None, rebuild=False, help=None):
        shown, rest = first_sentence(desc) if desc else ("", "")
        help_text = help or (desc if rest else None)
        f = UI.Card(parent)
        f.pack(fill="x", padx=4, pady=(0, 10))
        head = UI.Frame(f, bg=PANEL)
        head.pack(anchor="w", fill="x", padx=18, pady=(10, 0))
        UI.Label(head, title, size=13, fg=ACCENT, font=(DISPLAY, 15, "bold"), bg=PANEL).pack(side="left")
        if rebuild:
            rb_tag(head).pack(side="left", padx=(10, 0), pady=(3, 0))
        if help_text:
            UI.HelpIcon(head, help_text, title).pack(side="left", padx=(8, 0), pady=(2, 0))
        if shown:
            label(f, shown, size=9, color=DIM, bg=PANEL, wrap=1000).pack(anchor="w", padx=18, pady=(3, 10))
        body = UI.Frame(f, bg=PANEL)
        body.pack(fill="x", padx=18, pady=(0, 14))
        return body

    def check(parent, text, var):
        return UI.Check(parent, text, var)

    def radio(parent, text, var, value, cmd=None):
        return UI.Radio(parent, text, var, value, cmd=cmd)

    def combo(parent, values, var, width=16):
        return UI.Select(parent, values, var, width=width)

    def emblem(canvas, cx, cy, size):
        """Three golden triangles: the pack's emblem (no game assets or text)."""
        h = size * 0.866
        for dx, dy in ((0, -h / 2), (-size / 2, h / 2), (size / 2, h / 2)):
            x, y = cx + dx, cy + dy
            canvas.create_polygon(x, y - h / 2, x - size / 2, y + h / 2, x + size / 2, y + h / 2,
                                  fill=ACCENT, outline="#f1d27a", width=1)

    # ---- profile handling ----
    def profile_name(key):
        return L("profile_custom") if key == "custom" else L("p_" + key.replace("-", "_"))

    def apply_profile(key, save=True):
        if key == "custom":
            refresh()
            return
        v = profile_values(key)
        guard["on"] = True
        profile.set(key)
        bmap.set(v["button_mapping"])
        stamina.set(v["stamina"])
        songbook.set(v["songbook"])
        quick_boots.set(v["quick_boots"])
        quick_mask.set(v["quick_mask"])
        hud_collectibles.set(v["hud_collectibles"])
        hud_mode.set(v["hud_mode"])
        mode.set(STAMINA_MODES[v["mode"]])
        for k, val in zip(KEYS, v["costs"]):
            costs[k].set(val)
        combat.set(v["combat_only"])
        hp.set(v["enemy_hp"])
        dmg.set(v["damage"])
        nodrop.set(v["no_heart_drop"])
        speed.set(v["sprint_speed"])
        rupees.set(v["hud_rupees"])
        clock_mode.set("24" if v["clock_24h"] else "12" if v["hud_clock"] else "off")
        enemy_double.set(v["enemy_double"])
        enemy_move.set(v["enemy_move"])
        enemy_anim.set(v["enemy_anim"])
        enemy_attack.set(v["enemy_attack"])
        enemy_proj.set(v.get("enemy_proj", 0))
        enemy_alert.set(v.get("enemy_alert", 0))
        enemy_aggro.set(v.get("enemy_aggro", 0))
        boss_move.set(v.get("boss_move", 0))
        boss_anim.set(v.get("boss_anim", 0))
        great_fairy.set(v["great_fairy"])
        farore_outdoor.set(v["farore_outdoor"])
        manual_shield.set(v["manual_shield"])
        auto_target.set(v["auto_target"])
        switch_target.set(v["switch_target"])
        # personal preferences: only a profile that sets them (the Souls ones) changes them
        for var, key in ((target_r3, "target_r3"), (sprint_l3, "sprint_l3"), (sprint_l3_toggle, "sprint_l3_toggle"),
                         (stamina_bar, "stamina_bar")):
            if v[key] is not None:
                var.set(v[key])
        RM.set_roles(broles, I.roles_from_flags(RM.roles_value(broles), manual_shield=bool(v["manual_shield"]),
                                                target_r3=v["target_r3"], sprint_l3=v["sprint_l3"]))
        guard["on"] = False
        refresh()
        if save:
            autosave()

    def choose_profile(key):
        if key == "custom":
            profile.set("custom")
            refresh()
            autosave()
        else:
            apply_profile(key)

    def on_setting_change(*_):
        if guard["on"]:
            return
        if profile.get() != "custom":
            guard["on"] = True
            profile.set("custom")
            guard["on"] = False
        refresh()
        autosave()

    for var in (bmap, stamina, songbook, quick_boots, quick_mask, manual_shield, target_r3, sprint_l3, sprint_l3_toggle,
                auto_target, switch_target,
                hud_collectibles, hud_mode,
                enemy_double, enemy_move, enemy_anim, enemy_attack, enemy_proj, boss_move, boss_anim, enemy_alert, enemy_aggro,
                great_fairy, farore_outdoor, mode,
                combat, hp, dmg, speed, nodrop, nmagic, hide_min,
                rupees, clock_mode, *costs.values()):
        var.trace_add("write", on_setting_change)

    def on_mode_change(*_):
        """When a named stamina style is chosen, show what it does in the checkbox list."""
        if guard["on"]:
            return
        view = I.PRESET_VIEW.get(STAMINA_MODES.index(mode.get()))
        if view is not None:
            guard["on"] = True
            for k in KEYS:
                costs[k].set(bool(view[k]))
            combat.set(bool(view["combat_only"]))
            guard["on"] = False
        refresh()
        autosave()

    mode.trace_add("write", on_mode_change)

    def on_pref_change(*_):   # button style / layout / L mode: no profile change
        refresh()
        autosave()

    for var in (bstyle, bplace, glayout, bsimple, blmode):
        var.trace_add("write", on_pref_change)

    def roles_to_flags(*_):
        """The table decides the manual shield, the L3 sprint, target on R3 and the L selector."""
        fl = I.roles_flags(RM.roles_value(broles))
        r = RM.roles_value(broles)
        for var, val in ((sprint_l3, fl["sprint_l3"]), (target_r3, r.get("R3") == "target")):
            if var.get() != val:
                var.set(val)
        if blmode.get() != fl["bm_l_mode"]:
            blmode.set(fl["bm_l_mode"])
        on_pref_change()

    for var in broles.values():
        var.trace_add("write", roles_to_flags)
    stamina_bar.trace_add("write", on_pref_change)   # display preference: does not switch to Custom

    pending = {"id": None}

    def autosave(*_):
        if pending["id"]:
            root.after_cancel(pending["id"])
        pending["id"] = root.after(250, remember)

    def settings():
        # The mod's own HUD labels follow the language the game is played in; the app's language is the fallback
        # when the emulator/recompiler settings cannot be read.
        game = ok_recomp() if target.get() == "recomp" else ok_rekarina() if target.get() == "rekarina" else None
        e = Path(emu.get().strip()).expanduser() if emu.get().strip() else None
        return I.Settings(button_mapping=bmap.get(), button_style=bstyle.get(), bm_simple=bsimple.get(),
                          bm_layout=bplace.get(), game_layout=glayout.get(),
                          bm_l_mode=blmode.get(), stamina=stamina.get(), stamina_bar=stamina_bar.get(),
                          bm_remap=True, bm_roles=RM.roles_value(broles),
                          songbook=songbook.get(),
                          quick_boots=quick_boots.get(), quick_mask=quick_mask.get(),
                          manual_shield=manual_shield.get(),
                          target_r3=target_r3.get(), sprint_l3=sprint_l3.get(),
                          sprint_l3_toggle=sprint_l3_toggle.get(),
                          auto_target=auto_target.get(),
                          switch_target=switch_target.get(),
                          lang=I.detect_lang(e, game) or lang.get(),
                          hud_collectibles=hud_collectibles.get(),
                          hud_mode=hud_mode.get(),
                          enemy_double=enemy_double.get(),
                          enemy_move=enemy_move.get(), enemy_anim=enemy_anim.get(),
                          enemy_attack=enemy_attack.get(), enemy_proj=enemy_proj.get(),
                          enemy_alert=enemy_alert.get(), enemy_aggro=enemy_aggro.get(),
                          boss_move=boss_move.get(), boss_anim=boss_anim.get(),
                          great_fairy=great_fairy.get(), farore_outdoor=farore_outdoor.get(),
                          preset=STAMINA_MODES.index(mode.get()),
                          combat_only=combat.get(),
                          **{k: costs[k].get() for k in KEYS},
                          enemy_hp=hp.get(), damage=dmg.get(), no_heart_drop=nodrop.get(),
                          no_magic_drop=nmagic.get(), hide_minimap=hide_min.get(),
                          sprint_speed=speed.get(), hud_rupees=rupees.get(),
                          hud_clock=clock_mode.get() != "off", clock_24h=clock_mode.get() == "24",
                          hud_ctrl_scale=I.hud_scale(hud_ctrl_scale.get()),
                          hud_count_scale=I.hud_scale(hud_count_scale.get()))

    def remember():
        return save_cfg({
            "lang": lang.get(), "emu": emu.get(), "profile": profile.get(),
            "target": target.get(), "recomp": recomp_path.get(), "recomp_builder": recomp_builder.get(),
            "rekarina": rekarina_path.get(),
            "button_mapping": bmap.get(), "button_style": bstyle.get(), "bm_simple": bsimple.get(),
            "bm_layout": bplace.get(), "game_layout": glayout.get(),
            "bm_l_mode": blmode.get(),
            "bm_roles": RM.roles_value(broles),
            "stamina": stamina.get(), "stamina_bar": stamina_bar.get(), "songbook": songbook.get(),
            "quick_boots": quick_boots.get(), "quick_mask": quick_mask.get(),
            "manual_shield": manual_shield.get(),
            "target_r3": target_r3.get(), "sprint_l3": sprint_l3.get(),
            "sprint_l3_toggle": sprint_l3_toggle.get(),
            "auto_target": auto_target.get(),
            "switch_target": switch_target.get(),
            "hud_collectibles": hud_collectibles.get(),
            "hud_mode": hud_mode.get(),
            "enemy_double": enemy_double.get(),
            "enemy_move": enemy_move.get(), "enemy_anim": enemy_anim.get(),
            "enemy_attack": enemy_attack.get(), "enemy_proj": enemy_proj.get(),
            "enemy_alert": enemy_alert.get(), "enemy_aggro": enemy_aggro.get(),
            "boss_move": boss_move.get(), "boss_anim": boss_anim.get(),
            "great_fairy": great_fairy.get(), "farore_outdoor": farore_outdoor.get(),
            "stamina_mode": mode.get(),
            "costs": {**{k: costs[k].get() for k in KEYS}, "combat_only": combat.get()},
            "combat_only": combat.get(),
            "enemy_hp": hp.get(), "damage": dmg.get(), "no_heart_drop": nodrop.get(),
            "no_magic_drop": nmagic.get(), "hide_minimap": hide_min.get(),
            "sprint_speed": speed.get(), "hud_rupees": rupees.get(),
            "clock_mode": clock_mode.get(),
            "hud_clock": clock_mode.get() != "off", "clock_24h": clock_mode.get() == "24",
            "hud_ctrl_scale": hud_ctrl_scale.get(), "hud_count_scale": hud_count_scale.get(),
        })

    def ok_emu():
        e = emu.get().strip()
        return I.find_mods_dir(Path(e).expanduser()) if e else None

    def ok_recomp():
        p = recomp_path.get().strip()
        return R.find_game(Path(p).expanduser()) if p else None

    def ok_rekarina():
        p = rekarina_path.get().strip()
        return RK.find_game(Path(p).expanduser()) if p else None

    # ---- progress: the build runs in a thread and a queue brings updates to the window ----
    q = queue.Queue()

    def poll():
        try:
            while True:
                fn = q.get_nowait()
                try:
                    fn()
                except Exception:
                    pass
        except queue.Empty:
            pass
        root.after(80, poll)

    # ---- updates: GitHub Releases check, download and self-replace ----
    def show_update_bar():
        info = state.get("update")
        bar = ui.get("update_bar")
        if not info or bar is None or state.get("update_dismissed") == info.version:
            return
        ui["update_label"].config(text=L("update_banner", v=info.version, cur=VERSION))
        bar.pack(side="top", fill="x", pady=(8, 0))

    def dismiss_update():
        info = state.get("update")
        if info:
            state["update_dismissed"] = info.version
        bar = ui.get("update_bar")
        if bar is not None:
            bar.pack_forget()

    def check_done(info, err, manual):
        state["update_checking"] = False
        if err is not None:
            if manual:
                status.set("")
                messagebox.showwarning(L("update_failed"), parent=root)
            return
        if info is None:
            if manual:
                status.set("")
                messagebox.showinfo(L("update_latest", v=VERSION), parent=root)
            return
        state["update"] = info
        state["update_dismissed"] = ""
        show_update_bar()
        if manual:
            status.set("")
            if messagebox.askyesno(L("update_check"), L("update_banner", v=info.version, cur=VERSION), parent=root):
                download_update()

    def check_updates(manual=False):
        if state["update_checking"]:
            return
        state["update_checking"] = True
        if manual:
            status.set(L("update_checking"))

        def work():
            try:
                info, err = U.check(VERSION), None
            except Exception as e:
                info, err = None, e
            q.put(lambda i=info, e=err: check_done(i, e, manual))
        threading.Thread(target=work, daemon=True).start()

    def download_update():
        info = state.get("update")
        if info is None:
            return
        if not info.asset_url:
            U.open_releases()
            return
        try:
            dest, self_update = U.update_path(info.version)
        except Exception:
            messagebox.showerror(L("update_error"), parent=root)
            return

        win = UI.Toplevel(root)
        win.title(L("update_download"))
        win.configure(bg=BG)
        win.transient(root)
        win.resizable(False, False)
        win.geometry("440x120")
        lbl = UI.Label(win, text=L("update_downloading", p=0), bg=BG, fg=TEXT, font=(FAM, 10), anchor="w")
        lbl.pack(fill="x", padx=16, pady=(16, 8))
        bar = UI.Progress(win, style="SHB.Horizontal.TProgressbar", mode="determinate", maximum=100)
        bar.pack(fill="x", padx=16)
        win.protocol("WM_DELETE_WINDOW", lambda: None)
        win.grab_set()
        last = [-1]

        def progress(done, total):
            pct = int(done * 100 / total) if total else -1
            if pct == last[0]:
                return
            last[0] = pct
            q.put(lambda p=pct: (bar.configure(value=max(0, p)),
                                 lbl.configure(text=L("update_downloading", p=max(0, p)))))

        def finished(path):
            win.grab_release()
            win.destroy()
            if self_update and U.apply_and_restart(path):
                messagebox.showinfo(L("update_ready"), parent=root)
                root.after(250, root.destroy)
            else:
                messagebox.showinfo(L("update_manual", a=str(path)), parent=root)
                U.reveal(path)

        def failed():
            win.grab_release()
            win.destroy()
            messagebox.showerror(L("update_error"), parent=root)

        def work():
            try:
                U.download(info, dest, progress)
            except Exception:
                q.put(failed)
                return
            q.put(lambda: finished(dest))

        threading.Thread(target=work, daemon=True).start()

    def append_log(line):
        w = ui.get("log")
        if w is None:
            return
        w.config(state="normal")
        w.insert("end", line + "\n")
        if int(w.index("end-1c").split(".")[0]) > 400:
            w.delete("1.0", "100.0")
        w.see("end")
        w.config(state="disabled")

    def clear_log():
        w = ui.get("log")
        if w is not None:
            w.config(state="normal")
            w.delete("1.0", "end")
            w.config(state="disabled")

    def set_busy(on):
        busy["on"] = on
        for w in ui.get("lock", []):
            try:
                w.config(state="disabled" if on else "normal")
            except Exception:
                pass
        for _, b in ui.get("target_btns", []):
            try:
                b.set_enabled(not on)
            except Exception:
                pass
        for key in ("install", "uninstall", "launch", "rebuild", "reinstall", "builder_btn", "builder_auto", "builder_change"):
            w = ui.get(key)
            if w is not None:
                w.config(state="disabled" if on else "normal")
        c = ui.get("cancel")
        if c is not None:
            if on:
                if not c.winfo_manager():
                    c.pack(side="right", padx=(8, 0))
            else:
                c.pack_forget()
        if on and ui.get("_scroll") is not None:
            # The build's bar and log are at the bottom of the Installation card: show them.
            root.after(60, lambda: ui["_scroll"].canvas.yview_moveto(1.0))
        if not on:
            refresh()

    def update_progress(frac, text, stage):
        frac = max(0.0, min(1.0, frac))
        bar = ui.get("bar")
        if bar is not None:
            bar["value"] = frac
        lab = ui.get("prog_label")
        if lab is not None:
            lab.config(text=L("recomp_progress", p=int(frac * 100)))
        if stage:
            key = "recomp_stage_" + stage
            st = L(key)
            status.set(text if st == key else st)

    def recomp_done(r):
        set_busy(False)
        remember()
        status.set(L("ready"))
        if target.get() == "rekarina" and r.rebuilt:
            msg = L("rekarina_done")
        else:
            msg = L("recomp_done") if r.rebuilt else L("recomp_applied")
        for k, arg in r.notes:
            msg += "\n\n" + L("n_" + k, a=arg)
        messagebox.showinfo(L("recomp_done_title"), msg)
        refresh()

    def recomp_error(ex):
        set_busy(False)
        status.set("")
        if isinstance(ex, I.InstallError):
            if ex.key == "recomp_cancelled":
                status.set(L("recomp_cancelled"))
                messagebox.showinfo(L("recomp_cancel"), L("recomp_cancelled"))
            else:
                messagebox.showerror(L("err_title"), L("e_" + ex.key, a=ex.arg))
        else:
            messagebox.showerror(L("err_title"), str(ex))
        refresh()

    def confirm_build(mode, minutes):
        """The question before a build: what it will do depends on the button."""
        if mode == "recompile":
            return messagebox.askyesno(L("recomp_recompile_title"), L("recomp_recompile_confirm", a=minutes))
        if mode == "reinstall":
            return messagebox.askyesno(L("recomp_reinstall_title"), L("recomp_reinstall_confirm", a=minutes))
        return messagebox.askyesno(L("recomp_confirm_title"), L("recomp_confirm"))

    def run_recomp(mode="install"):
        """mode: install (copy the mod and compile), recompile (only the builder), reinstall (uninstall, then install)."""
        game = ok_recomp()
        if game is None:
            messagebox.showerror(L("err_title"), L("e_recomp_no_game", a=recomp_path.get()))
            return
        if not confirm_build(mode, "25-45"):
            return
        if mode != "recompile" and not need_layout():
            return
        show_section("install")
        set_busy(True)
        clear_log()
        if ui.get("bar") is not None:
            ui["bar"]["value"] = 0
        status.set(L("recomp_stage_start"))
        snap = settings()
        builder = recomp_builder.get().strip() or None

        def worker():
            try:
                kw = dict(builder_path=builder,
                          progress=lambda f, t, s: q.put(lambda f=f, t=t, s=s: update_progress(f, t, s)),
                          log=lambda line: q.put(lambda line=line: append_log(line)))
                if mode == "recompile":
                    r = R.recompile(game, **kw)
                elif mode == "reinstall":
                    r = R.reinstall(game, snap, **kw)
                else:
                    r = R.install(game, snap, rebuild=True, **kw)
                q.put(lambda r=r: recomp_done(r))
            except Exception as ex:
                q.put(lambda ex=ex: recomp_error(ex))

        threading.Thread(target=worker, daemon=True).start()

    def run_rekarina(mode="install"):
        game = ok_rekarina()
        if game is None:
            messagebox.showerror(L("err_title"), L("e_rekarina_no_game", a=rekarina_path.get()))
            return
        if mode == "install":
            if not messagebox.askyesno(L("rekarina_confirm_title"), L("rekarina_confirm")):
                return
        elif not confirm_build(mode, "10-20"):
            return
        show_section("install")
        set_busy(True)
        clear_log()
        if ui.get("bar") is not None:
            ui["bar"]["value"] = 0
        status.set(L("recomp_stage_start"))
        snap = settings()

        def worker():
            try:
                kw = dict(progress=lambda f, t, s: q.put(lambda f=f, t=t, s=s: update_progress(f, t, s)),
                          log=lambda line: q.put(lambda line=line: append_log(line)))
                if mode == "recompile":
                    r = RK.recompile(game, snap, **kw)
                elif mode == "reinstall":
                    r = RK.reinstall(game, snap, **kw)
                else:
                    r = RK.install(game, snap, rebuild=True, **kw)
                q.put(lambda r=r: recomp_done(r))
            except Exception as ex:
                q.put(lambda ex=ex: recomp_error(ex))

        threading.Thread(target=worker, daemon=True).start()

    def browse():
        d = filedialog.askdirectory(title=L("emu_hint"))
        if d:
            emu.set(d)

    def browse_recomp():
        d = filedialog.askdirectory(title=L("recomp_hint"))
        if d:
            recomp_path.set(d)

    def browse_rekarina():
        d = filedialog.askdirectory(title=L("rekarina_hint"))
        if d:
            rekarina_path.set(d)

    def browse_builder():
        d = filedialog.askdirectory(title=L("recomp_builder_hint"))
        if d:
            recomp_builder.set(d)
            show_builder(True)

    builder_open = {"on": False}

    def set_builder_visible(vis):
        frame = ui.get("builder_frame")
        if frame is None:
            return
        try:
            if vis:
                if not frame.winfo_manager():
                    before = ui.get("recomp_note")
                    if before is not None:
                        frame.pack(fill="x", pady=(4, 0), before=before)
                    else:
                        frame.pack(fill="x", pady=(4, 0))
            else:
                frame.pack_forget()
        except tk.TclError:
            pass

    def show_builder(show=None):
        """Open/close the optional builder entry (it stays hidden while a builder is detected)."""
        if show is not None:
            builder_open["on"] = bool(show)
        set_builder_visible(builder_open["on"])

    def clear_builder():
        builder_open["on"] = False
        recomp_builder.set("")
        refresh()

    def on_target(*_):
        if not ui.get("sections"):
            return
        tgt = target.get()
        for key in ("emu_frame", "recomp_frame", "rekarina_frame"):
            w = ui.get(key)
            if w is not None:
                w.pack_forget()
        if tgt == "recomp":
            ui["recomp_frame"].pack(fill="x")
        elif tgt == "rekarina":
            ui["rekarina_frame"].pack(fill="x")
        else:
            ui["emu_frame"].pack(fill="x")
        build_row = ui.get("build_frame")
        if build_row is not None:
            if tgt in ("recomp", "rekarina"):
                if not build_row.winfo_manager():
                    build_row.pack(fill="x", pady=(10, 0))
            else:
                build_row.pack_forget()
        status.set("")
        refresh()
        autosave()

    def do_recomp_install():
        game = ok_recomp()
        if game is None:
            messagebox.showerror(L("err_title"), L("e_recomp_no_game", a=recomp_path.get()))
            return
        if not need_layout():
            return
        try:
            if R.is_built(game):
                R.apply_settings(game, settings())
                remember()
                status.set(L("ready"))
                messagebox.showinfo(L("done_title"), L("recomp_applied"))
                refresh()
            else:
                run_recomp()
        except I.InstallError as ex:
            messagebox.showerror(L("err_title"), L("e_" + ex.key, a=ex.arg))
        except Exception as ex:
            messagebox.showerror(L("err_title"), str(ex))

    def do_rekarina_install():
        game = ok_rekarina()
        if game is None:
            messagebox.showerror(L("err_title"), L("e_rekarina_no_game", a=rekarina_path.get()))
            return
        try:
            if RK.is_built(game):
                RK.apply_settings(game, settings())
                remember()
                status.set(L("ready"))
                messagebox.showinfo(L("done_title"), L("recomp_applied"))
                refresh()
            else:
                run_rekarina()
        except I.InstallError as ex:
            messagebox.showerror(L("err_title"), L("e_" + ex.key, a=ex.arg))
        except Exception as ex:
            messagebox.showerror(L("err_title"), str(ex))

    def do_install():
        show_section("install")
        if target.get() == "recomp":
            do_recomp_install()
            return
        if target.get() == "rekarina":
            do_rekarina_install()
            return
        try:
            status.set(L("working"))
            root.update_idletasks()
            if not need_layout():
                status.set("")
                return
            r = I.install(Path(emu.get().strip()).expanduser(), settings())
            remember()
            msg = L("done_msg", n=r.target.name)
            for k, arg in r.notes:
                msg += "\n\n" + L("n_" + k, a=arg)
            status.set(L("ready"))
            messagebox.showinfo(L("done_title"), msg)
        except I.InstallError as ex:
            status.set("")
            messagebox.showerror(L("err_title"), L("e_" + ex.key, a=ex.arg))
        except Exception as ex:
            status.set("")
            messagebox.showerror(L("err_title"), str(ex))

    def do_launch():
        """Open the emulator, the recompiled game through its launcher, or Rekarina."""
        if target.get() == "recomp":
            game = ok_recomp()
            exe = R.launcher_of(game) if game else None
            if exe is None:
                messagebox.showerror(L("err_title"), L("e_launch_recomp", a=recomp_path.get()))
                return
            if busy["on"]:
                return
        elif target.get() == "rekarina":
            game = ok_rekarina()
            exe = RK.launcher_of(game) if game else None
            if exe is None:
                messagebox.showerror(L("err_title"), L("e_launch_recomp", a=rekarina_path.get()))
                return
            if busy["on"]:
                return
        else:
            exe = I.emulator_exe(Path(emu.get().strip())) if emu.get().strip() else None
            if exe is None:
                messagebox.showerror(L("err_title"), L("e_launch_emu", a=emu.get()))
                return
        try:
            I.launch(exe)
            status.set(L("launched", a=exe.name))
        except OSError as ex:
            messagebox.showerror(L("err_title"), str(ex))

    def do_remove():
        if target.get() == "recomp":
            game = ok_recomp()
            if game is None:
                status.set(L("nothing"))
                return
            if not messagebox.askyesno(L("uninstall"), L("recomp_remove_confirm")):
                return
            if R.game_running():
                messagebox.showerror(L("err_title"), L("e_recomp_running"))
                return
            try:
                ok = R.uninstall(game)
            except Exception as ex:
                messagebox.showerror(L("err_title"), str(ex))
                return
            status.set(L("uninstalled") if ok else L("nothing"))
            refresh()
            return
        if target.get() == "rekarina":
            game = ok_rekarina()
            if game is None:
                status.set(L("nothing"))
                return
            if not messagebox.askyesno(L("uninstall"), L("recomp_remove_confirm")):
                return
            if RK.game_running(game):
                messagebox.showerror(L("err_title"), L("e_recomp_running"))
                return
            try:
                ok = RK.uninstall(game)
            except Exception as ex:
                messagebox.showerror(L("err_title"), str(ex))
                return
            status.set(L("uninstalled") if ok else L("nothing"))
            refresh()
            return
        e = emu.get().strip()
        status.set(L("uninstalled") if e and I.uninstall(Path(e).expanduser()) else L("nothing"))

    def show_build_buttons(installed):
        """Recompile and Reinstall-and-recompile exist only while the mod is in the recompiled game: before that,
        Install already copies everything and compiles."""
        for key in ("rebuild", "reinstall"):
            w = ui.get(key)
            if w is None:
                continue
            if installed:
                if not w.winfo_manager():
                    w.pack(side="right", padx=(0, 8))
                w.config(state="normal" if not busy["on"] else "disabled")
            else:
                w.pack_forget()

    def refresh(*_):
        if not ui["sections"]:
            return
        stam = stamina.get()
        adv = stam and mode.get() == "advanced"
        for w in list(ui.get("modes", [])) + list(ui.get("display_modes", [])):
            w.config(state="normal" if stam else "disabled")
        for w in ui.get("costs", []):
            w.config(state="normal" if adv else "disabled")
        for key in ("combat", "sprint"):
            w = ui.get(key)
            if w is not None:
                w.config(state="normal" if stam else "disabled")
        on = bmap.get()
        if "game_layout" in ui:   # the game-wide prompts do not depend on the button mapping
            refresh_layout_note()
        for w in ui.get("bm", []):
            w.config(state=("readonly" if isinstance(w, UI.Select) else "normal") if on else "disabled")
        ui["note"].config(text="" if stam else L("stamina_off_note"), fg=AMBER)
        paint = ui.get("stamina_preview_paint")
        if paint is not None:
            paint()
        rec = target.get() in ("recomp", "rekarina")
        for t in ui.get("rb_tags", []):
            try:
                t.config(text=L("needs_rebuild") if rec else "")
            except Exception:
                pass
        mods = ok_emu()
        if target.get() == "recomp":
            game = ok_recomp()
            built = bool(game) and R.is_built(game)
            if "recomp_status" in ui:
                ui["recomp_status"].config(text=L("recomp_found", a=game) if game else L("recomp_missing"),
                                           fg=ACCENT if game else DIM)
            if "recomp_note" in ui:
                ui["recomp_note"].config(
                    text=L("recomp_built") if built else (L("recomp_not_built") if game else ""),
                    fg=DIM if built else AMBER)
            if "builder_line" in ui:
                user = recomp_builder.get().strip()
                builder = None
                if user:
                    cand = Path(user).expanduser()
                    if R.is_builder(cand):
                        builder = cand
                    ui["builder_line"].config(
                        text=L("recomp_builder_manual", a=builder or cand), fg=DIM if builder else RED)
                    set_builder_visible(True)
                else:
                    builder = R.find_builder(game) if game else None
                    ui["builder_line"].config(
                        text=L("recomp_builder_found", a=builder) if builder else L("recomp_builder_hint"),
                        fg=DIM if builder else AMBER)
                    # Optional: no entry while the game's own launcher has a builder; shown when it does not
                    # (or when "Change" opened it).
                    set_builder_visible(not builder or builder_open["on"])
            else:
                builder = None
            installed = bool(game) and R.is_installed(game)
            show_build_buttons(installed)
            if "builder_btn" in ui:
                if game and not builder:
                    if not ui["builder_btn"].winfo_manager():
                        ui["builder_btn"].pack(side="right", padx=(0, 8))
                else:
                    ui["builder_btn"].pack_forget()
            ui["install"].config(text=L("recomp_apply") if built else L("recomp_install"),
                                 state="normal" if game and not busy["on"] else "disabled")
            if not game and not status.get():
                status.set(L("recomp_missing"))
        elif target.get() == "rekarina":
            game = ok_rekarina()
            built = bool(game) and RK.is_built(game)
            setup = RK.find_setup(game) if game else None
            if "rekarina_status" in ui:
                if game is None:
                    ui["rekarina_status"].config(text=L("rekarina_missing"), fg=DIM)
                elif setup is None:
                    ui["rekarina_status"].config(text=L("e_rekarina_no_setup", a=""), fg=AMBER)
                else:
                    ui["rekarina_status"].config(text=L("rekarina_found", a=game), fg=ACCENT)
            if "rekarina_note" in ui:
                ui["rekarina_note"].config(
                    text=L("rekarina_built") if built else (L("rekarina_not_built") if game and setup else ""),
                    fg=DIM if built else AMBER)
            installed = bool(game) and RK.is_installed(game)
            show_build_buttons(installed)
            if "builder_btn" in ui:
                ui["builder_btn"].pack_forget()
            ui["install"].config(text=L("recomp_apply") if built else L("recomp_install"),
                                 state="normal" if game and setup and not busy["on"] else "disabled")
            if not game and not status.get():
                status.set(L("rekarina_missing"))
        else:
            ui["install"].config(text=L("install"),
                                 state="normal" if mods and not busy["on"] else "disabled")
            show_build_buttons(False)
            if "builder_btn" in ui:
                ui["builder_btn"].pack_forget()
            ui["mods"].config(text=L("mods_found", a=mods) if mods else L("mods_missing"),
                              fg=ACCENT if mods else DIM)
            if not mods and not status.get():
                status.set(L("pick_emu"))
            if mods and status.get() == L("pick_emu"):
                status.set("")
        token = profile.get().replace("-", "_")
        if "profile_desc" in ui:
            ui["profile_desc"].config(text=L("p_" + token + "_d") if profile.get() in PROFILES
                                      else L("p_custom_d"))
        paint = ui.get("profile_paint")
        if paint is not None:
            paint()
        ui["profile_badge"].config(text=L("profile_current", a=profile_name(profile.get())),
                                   fg=AMBER if profile.get() == "custom" else ACCENT)
        ui["reapply"].config(state="normal" if profile.get() != "custom" else "disabled")
        # Keep the comboboxes in sync with the model, so a profile changes every visible control.
        for cb, var, svar, names, choices in ui.get("mult", []):
            idx = choices.index(var.get()) if var.get() in choices else 0
            svar.set(names[idx])
            cb.current(idx)
        for cb, var in ui.get("enemy", []):
            idx = I.ENEMY_LEVELS.index(var.get()) if var.get() in I.ENEMY_LEVELS else 0
            cb.current(idx)
        if "aggro" in ui:
            ui["aggro"].current(enemy_aggro.get() if enemy_aggro.get() in I.AGGRO_LEVELS else 0)
        if "sprint" in ui:
            idx = SPEEDS.index(speed.get()) if speed.get() in SPEEDS else 0
            ui["sprint_var"].set(ui["sprint_values"][idx])
            ui["sprint"].current(idx)
        for key, var, choices in (("variant", bstyle, I.BUTTON_STYLES), ("game_layout", glayout, I.GAME_LAYOUTS)):
            if key in ui:
                idx = choices.index(var.get()) if var.get() in choices else 0
                ui[key + "_var"].set(ui[key + "_values"][idx])
                ui[key].current(idx)
        if "clock" in ui:
            idx = CLOCK_MODES.index(clock_mode.get()) if clock_mode.get() in CLOCK_MODES else 0
            ui["clock_var"].set(ui["clock_values"][idx])
            ui["clock"].current(idx)
        if "hud_mode" in ui:
            idx = I.HUD_MODES.index(hud_mode.get()) if hud_mode.get() in I.HUD_MODES else 0
            ui["hud_mode_var"].set(ui["hud_mode_values"][idx])
            ui["hud_mode"].current(idx)

    # ---- sections ----
    def build_home(parent):
        body = card(parent, L("home_title"), L("home_desc"))
        prof = UI.Frame(body, bg=PANEL)
        prof.pack(fill="x")
        ui["profile_cards"] = []
        for key in PROFILE_KEYS:
            full = L("p_" + key.replace("-", "_") + "_d")
            shown, rest = first_sentence(full)
            c = UI.ChoiceCard(prof, profile_name(key), shown, command=lambda k=key: choose_profile(k),
                              desc_wrap=900, title_size=10, help_text=full if rest else None)
            c.pack(fill="x", pady=(0, 4))
            ui["profile_cards"].append((key, c))

        def paint_profiles():
            for key, c in ui.get("profile_cards", []):
                c.set_active(key == profile.get())

        paint_profiles()
        ui["profile_paint"] = paint_profiles
        desc_label(body, L("profile_shield_note"), indent=0, pady=(8, 0))
        ui["reapply"] = button(body, L("reapply"), lambda: apply_profile(profile.get()))
        ui["reapply"].pack(anchor="w", pady=(8, 0))

    def build_install(parent):
        body = card(parent, L("install_title"), L("install_desc"))
        label(body, L("target"), size=10, color=ACCENT, bold=True, bg=PANEL).pack(anchor="w", pady=(0, 6))
        trow = UI.Frame(body, bg=PANEL)
        trow.pack(fill="x", pady=(0, 10))
        for i in range(3):
            trow.grid_columnconfigure(i, weight=1)
        trefs = []
        ui["target_btns"] = []

        def paint_targets():
            for val, btn in ui.get("target_btns", []):
                btn.set_active(target.get() == val)

        def select_target(val):
            if busy["on"]:
                return
            target.set(val)
            paint_targets()
            on_target()

        for i, (val, key, sub) in enumerate((("emulator", "target_emu", "target_emu_d"),
                                              ("recomp", "target_recomp", "target_recomp_d"),
                                              ("rekarina", "target_rekarina", "target_rekarina_d"))):
            b = UI.ChoiceCard(trow, L(key + "_short"), L(sub), command=lambda v=val: select_target(v))
            b.grid(row=0, column=i, sticky="ew", padx=(0, 8) if i < 2 else (0, 0))
            trefs.append(b)
            ui.setdefault("target_btns", []).append((val, b))
        paint_targets()

        ef = UI.Frame(body, bg=PANEL)
        ui["emu_frame"] = ef
        row = UI.Frame(ef, bg=PANEL)
        row.pack(fill="x")
        entry_emu = UI.Entry(row, textvariable=emu, bg=PANEL2, fg=TEXT, insertbackground=ACCENT, relief="flat",
                             font=(FAM, 10), highlightthickness=1, highlightbackground=BORDER,
                             highlightcolor=ACCENT)
        entry_emu.pack(side="left", fill="x", expand=True, ipady=4)
        b_emu = button(row, L("browse"), browse)
        b_emu.pack(side="left", padx=(8, 0))
        label(ef, L("emu_hint"), size=9, color=DIM, bg=PANEL).pack(anchor="w", pady=(5, 0))
        ui["mods"] = label(ef, "", size=9, color=DIM, bg=PANEL)
        ui["mods"].pack(anchor="w", pady=(2, 0))
        label(ef, L("install_hint"), size=9, color=AMBER, bg=PANEL).pack(anchor="w", pady=(6, 0))
        label(ef, L("autosave"), size=9, color=DIM, bg=PANEL).pack(anchor="w", pady=(2, 0))

        rf = UI.Frame(body, bg=PANEL)
        ui["recomp_frame"] = rf
        row = UI.Frame(rf, bg=PANEL)
        row.pack(fill="x")
        entry_rec = UI.Entry(row, textvariable=recomp_path, bg=PANEL2, fg=TEXT, insertbackground=ACCENT,
                             relief="flat", font=(FAM, 10), highlightthickness=1,
                             highlightbackground=BORDER, highlightcolor=ACCENT)
        entry_rec.pack(side="left", fill="x", expand=True, ipady=4)
        b_rec = button(row, L("browse"), browse_recomp)
        b_rec.pack(side="left", padx=(8, 0))
        label(rf, L("recomp_hint"), size=9, color=DIM, bg=PANEL).pack(anchor="w", pady=(5, 0))
        ui["recomp_status"] = label(rf, "", size=9, color=DIM, bg=PANEL)
        ui["recomp_status"].pack(anchor="w", pady=(2, 0))
        bline = UI.Frame(rf, bg=PANEL)
        bline.pack(fill="x", pady=(8, 0))
        ui["builder_line"] = label(bline, "", size=9, color=DIM, bg=PANEL)
        ui["builder_line"].pack(side="left")
        b_change = button(bline, L("recomp_builder_change"), lambda: show_builder(True))
        b_change.pack(side="left", padx=(8, 0))
        ui["builder_change"] = b_change
        # Optional: only shown when no builder was detected, when one was chosen by hand, or by "Change".
        ui["builder_frame"] = UI.Frame(rf, bg=PANEL)
        label(ui["builder_frame"], L("recomp_builder"), size=9, color=DIM,
              bg=PANEL).pack(anchor="w", pady=(2, 0))
        brow = UI.Frame(ui["builder_frame"], bg=PANEL)
        brow.pack(fill="x", pady=(2, 0))
        entry_b = UI.Entry(brow, textvariable=recomp_builder, bg=PANEL2, fg=TEXT, insertbackground=ACCENT,
                           relief="flat", font=(FAM, 10), highlightthickness=1,
                           highlightbackground=BORDER, highlightcolor=ACCENT)
        entry_b.pack(side="left", fill="x", expand=True, ipady=4)
        b_build = button(brow, L("browse"), browse_builder)
        b_build.pack(side="left", padx=(8, 0))
        ui["builder_btn"] = b_build
        b_auto = button(brow, L("recomp_builder_auto"), clear_builder)
        b_auto.pack(side="left", padx=(8, 0))
        ui["builder_auto"] = b_auto
        ui["recomp_note"] = label(rf, "", size=9, color=AMBER, bg=PANEL)
        ui["recomp_note"].pack(anchor="w", pady=(8, 0))

        # Rekarina: same install card, its own path (the game folder Rekarina.exe made).
        kf = UI.Frame(body, bg=PANEL)
        ui["rekarina_frame"] = kf
        row = UI.Frame(kf, bg=PANEL)
        row.pack(fill="x")
        entry_rek = UI.Entry(row, textvariable=rekarina_path, bg=PANEL2, fg=TEXT, insertbackground=ACCENT,
                             relief="flat", font=(FAM, 10), highlightthickness=1,
                             highlightbackground=BORDER, highlightcolor=ACCENT)
        entry_rek.pack(side="left", fill="x", expand=True, ipady=4)
        b_rek = button(row, L("browse"), browse_rekarina)
        b_rek.pack(side="left", padx=(8, 0))
        label(kf, L("rekarina_hint"), size=9, color=DIM, bg=PANEL).pack(anchor="w", pady=(5, 0))
        ui["rekarina_status"] = label(kf, "", size=9, color=DIM, bg=PANEL)
        ui["rekarina_status"].pack(anchor="w", pady=(2, 0))
        ui["rekarina_note"] = label(kf, "", size=9, color=AMBER, bg=PANEL, wrap=760)
        ui["rekarina_note"].pack(anchor="w", pady=(8, 0))

        # The progress bar and the log are shared by the two recompilers.
        bf = UI.Frame(body, bg=PANEL)
        ui["build_frame"] = bf
        prow = UI.Frame(bf, bg=PANEL)
        prow.pack(fill="x", pady=(0, 2))
        ui["bar"] = UI.Progress(prow, style="SHB.Horizontal.TProgressbar", mode="determinate",
                                    maximum=1.0, value=0)
        ui["bar"].pack(side="left", fill="x", expand=True)
        ui["prog_label"] = label(prow, L("recomp_progress", p=0), size=9, color=DIM, bg=PANEL)
        ui["prog_label"].pack(side="left", padx=(10, 0))
        label(bf, L("recomp_log"), size=9, color=DIM, bg=PANEL).pack(anchor="w", pady=(6, 2))
        ui["log"] = UI.Textbox(bf, height=6, bg=PANEL2, fg=DIM, insertbackground=ACCENT, relief="flat",
                            font=(FAM, 8), highlightthickness=1, highlightbackground=BORDER,
                            state="disabled", wrap="none")
        ui["log"].pack(fill="x")
        ui["lock"] = [entry_emu, b_emu, entry_rec, b_rec, entry_rek, b_rek]
        tgt = target.get()
        if tgt == "recomp":
            rf.pack(fill="x")
        elif tgt == "rekarina":
            kf.pack(fill="x")
        else:
            ef.pack(fill="x")
        if tgt in ("recomp", "rekarina"):
            bf.pack(fill="x", pady=(10, 0))

    def build_stamina(parent):
        body = card(parent, L("stamina_title"), L("stamina_desc"))
        check_rb(body, L("stamina_on"), stamina)
        desc_label(body, L("stamina_on_desc"))
        label(body, L("stamina_hearts"), size=9, color=AMBER, bg=PANEL).pack(
            anchor="w", padx=(20, 0), pady=(2, 0))

        label(body, L("stamina_display"), size=10, color=ACCENT, bold=True, bg=PANEL).pack(anchor="w", pady=(8, 0))
        row = UI.Frame(body, bg=PANEL)
        row.pack(fill="x", pady=(2, 0))
        opts = UI.Frame(row, bg=PANEL)
        opts.pack(side="left", anchor="n")
        ui["display_modes"] = []
        for value, key in ((False, "stamina_ring"), (True, "stamina_bar")):
            full = L(key + "_d")
            shown, rest = first_sentence(full)
            line = UI.Frame(opts, bg=PANEL)
            line.pack(anchor="w")
            r = radio(line, L(key), stamina_bar, value)
            r.pack(side="left")
            ui["display_modes"].append(r)
            if rest:
                RM.help_icon(line, full, COLORS, FAM).pack(side="left", padx=(6, 0))
            label(opts, shown, size=9, color=DIM, bg=PANEL, wrap=560).pack(
                anchor="w", padx=(20, 0), pady=(0, 3))

        prev = UI.Frame(row, bg=PANEL)
        prev.pack(side="left", anchor="n", padx=(36, 0))
        label(prev, L("preview"), size=9, color=DIM, bold=True, bg=PANEL).pack(anchor="w")
        cv = tk.Canvas(prev, width=262, height=122, bg=PANEL2, highlightthickness=1,
                       highlightbackground=BORDER)
        cv.pack(anchor="w", pady=(2, 0))
        ui["stamina_preview"] = cv

        def paint_preview():
            if not cv.winfo_exists():
                return
            cv.delete("all")
            on = stamina.get()
            bar = stamina_bar.get()
            if bar:
                col = ACCENT if on else DISABLED
                x0, y0, seg_w, gap, count = 35, 55, 11, 1, 16
                for i in range(count):
                    x = x0 + i * (seg_w + gap)
                    cv.create_rectangle(x, y0, x + seg_w, y0 + 12, fill=col, outline="")
            else:
                col = TEAL if on else DISABLED
                cv.create_oval(89, 17, 173, 101, outline=col, width=8)
            if not on:
                cv.create_text(214, 18, text="OFF", anchor="e", fill=DISABLED,
                               font=(FAM, 8, "bold"))
        ui["stamina_preview_paint"] = paint_preview
        paint_preview()

        ui["note"] = label(body, "", size=9, color=AMBER, bg=PANEL, wrap=1000)
        ui["note"].pack(anchor="w", pady=(6, 0))

        body = card(parent, L("stamina_mode"))
        ui["modes"] = []
        for name in STAMINA_MODES:
            r = radio(body, L("m_" + name), mode, name)
            r.pack(anchor="w")
            ui["modes"].append(r)
            label(body, L("m_" + name + "_d"), size=9, color=DIM, bg=PANEL).pack(
                anchor="w", padx=(20, 0), pady=(0, 3))

        body = card(parent, L("costs_title"), L("costs_desc"))
        grid = UI.Frame(body, bg=PANEL)
        grid.pack(fill="x")
        ui["costs"] = []
        for i, k in enumerate(KEYS):
            c = check(grid, L("a_" + k), costs[k])
            c.grid(row=i // 2, column=i % 2, sticky="w", padx=(0, 28), pady=1)
            ui["costs"].append(c)
        ui["combat"] = check(body, L("combat_only"), combat)
        ui["combat"].pack(anchor="w", pady=(6, 0))

    def build_difficulty(parent):
        body = card(parent, L("difficulty_title"), L("difficulty_desc"))
        row = UI.Frame(body, bg=PANEL)
        row.pack(fill="x")
        names = [L("mult_van"), L("mult_15"), L("mult_2"), L("mult_3")]
        ui["mult"] = []
        for n, (lab, var) in enumerate(((L("enemy_hp"), hp), (L("damage"), dmg))):
            label(row, lab + ":", size=10, bg=PANEL).grid(row=0, column=n * 2, sticky="w", padx=(0, 6))
            svar = tk.StringVar(value=names[MULT.index(var.get())])
            cb = combo(row, names, svar, width=14)
            cb.grid(row=0, column=n * 2 + 1, padx=(0, 24))
            cb.bind("<<ComboboxSelected>>", lambda e, c=cb, v=var: v.set(MULT[c.current()]))
            ui["mult"].append((cb, var, svar, names, MULT))
        c = check(body, L("no_heart_drop"), nodrop)
        c.pack(anchor="w", pady=(10, 0))
        desc_label(body, L("no_heart_drop_desc"))
        check(body, L("no_magic_drop"), nmagic).pack(anchor="w", pady=(10, 0))
        desc_label(body, L("no_magic_drop_desc"))
        label(body, L("difficulty_note"), size=9, color=DIM, bg=PANEL).pack(anchor="w", pady=(8, 0))
        build_enemies(parent)

    def build_hud(parent):
        body = card(parent, L("hud_title"), L("hud_desc"))
        mrow = UI.Frame(body, bg=PANEL)
        mrow.pack(fill="x", pady=(0, 8))
        label(mrow, L("hud_mode") + ":", size=10, bg=PANEL).grid(row=0, column=0, sticky="w", padx=(0, 6))
        mvalues = [L("hud_mode_wide"), L("hud_mode_simple"), L("hud_mode_plain")]
        mvar = tk.StringVar(value=mvalues[I.HUD_MODES.index(hud_mode.get())])
        mcb = combo(mrow, mvalues, mvar, width=24)
        mcb.grid(row=0, column=1, sticky="w")
        mcb.bind("<<ComboboxSelected>>", lambda e: hud_mode.set(I.HUD_MODES[mcb.current()]))
        ui["hud_mode"] = mcb
        ui["hud_mode_var"] = mvar
        ui["hud_mode_values"] = mvalues
        label(body, L("hud_mode_wide_desc"), size=9, color=DIM, bg=PANEL).pack(anchor="w", padx=(20, 0))
        label(body, L("hud_mode_simple_desc"), size=9, color=DIM, bg=PANEL).pack(anchor="w", padx=(20, 0))
        label(body, L("hud_mode_plain_desc"), size=9, color=DIM, bg=PANEL).pack(anchor="w", padx=(20, 0), pady=(0, 8))
        row = UI.Frame(body, bg=PANEL)
        row.pack(fill="x")
        check(row, L("hud_rupees"), rupees).grid(row=0, column=0, sticky="w", padx=(0, 30))
        label(row, L("clock_mode") + ":", size=10, bg=PANEL).grid(row=0, column=1, sticky="w", padx=(0, 6))
        values = [L("clock_off"), L("clock_12"), L("clock_24")]
        cvar = tk.StringVar(value=values[CLOCK_MODES.index(clock_mode.get())])
        cb = combo(row, values, cvar, width=18)
        cb.grid(row=0, column=2, sticky="w")
        cb.bind("<<ComboboxSelected>>", lambda e: clock_mode.set(CLOCK_MODES[cb.current()]))
        ui["clock"] = cb
        ui["clock_var"] = cvar
        ui["clock_values"] = values
        check(body, L("hud_collectibles"), hud_collectibles).pack(anchor="w", pady=(8, 0))
        desc_label(body, L("hud_collectibles_desc"))
        check(body, L("hide_minimap"), hide_min).pack(anchor="w", pady=(8, 0))
        desc_label(body, L("hide_minimap_desc"))
        scale_row(body, "hud_count_scale", hud_count_scale, pady=(10, 0))
        label(body, L("hud_note"), size=9, color=DIM, bg=PANEL).pack(anchor="w", pady=(8, 0))
        build_game_prompts(parent)

    def build_game_prompts(parent):
        """HUD section: the game's own button prompts (independent of the button mapping, optional)."""
        body = card(parent, L("game_layout"), L("game_layout_d"))
        row = UI.Frame(body, bg=PANEL)
        row.pack(anchor="w")
        gnames = [L("game_layout_none" if k == "none" else "g_" + k) for k in I.GAME_LAYOUTS]
        gvar = tk.StringVar(value=gnames[I.GAME_LAYOUTS.index(glayout.get())])
        ui["game_layout_var"] = gvar
        ui["game_layout_values"] = gnames
        gcb = combo(row, gnames, gvar, width=16)
        gcb.pack(side="left")
        gcb.bind("<<ComboboxSelected>>", lambda e: (glayout.set(I.GAME_LAYOUTS[gcb.current()]),
                                                    refresh_layout_note()))
        ui["game_layout"] = gcb
        ui["layout_dl"] = button(row, L("layout_btn"), lambda: download_layout(glayout.get()))
        ui["layout_dl"].pack(side="left", padx=(8, 0))
        note = label(body, "", size=9, color=DIM, bg=PANEL)
        note.pack(anchor="w", pady=(2, 0))
        ui["layout_note"] = note
        refresh_layout_note()

    def build_controls(parent):
        pbody = card(parent, L("ctrl_preset_title"), L("ctrl_preset_desc"),
                     help=L("ctrl_preset_desc") + "\n\n" + L("ctrl_hero_desc") + "\n\n" + L("rm_help"))
        pgrid = UI.Frame(pbody, bg=PANEL)
        pgrid.pack(fill="x")
        for i in range(2):
            pgrid.grid_columnconfigure(i, weight=1)
        ui["ctrl_presets"] = []

        def paint_presets():
            current = RM.preset_of(RM.roles_value(broles))
            if bplace.get() == "gamecube":
                current = ("gamecube" if current == "classic" and bmap.get() and
                           bstyle.get() == "switch" and not bsimple.get() else "")
            for key, btn in ui.get("ctrl_presets", []):
                btn.set_active(key == current)

        def apply_preset(key):
            table = I.REMAP_PRESETS["classic" if key == "gamecube" else key]
            if key == "gamecube":
                bplace.set("gamecube")
                bstyle.set("switch")
                bsimple.set(False)
                bmap.set(True)
            elif bplace.get() == "gamecube":
                bplace.set("nintendo")
            if "shield" in table.values() and not manual_shield.get():
                manual_shield.set(True)
            RM.set_roles(broles, table)
            paint_presets()

        for i, key in enumerate((*I.REMAP_PRESET_KEYS, "gamecube")):
            b = UI.ChoiceCard(pgrid, L("preset_" + key), L("ctrl_preset_" + key + "_d"),
                              command=lambda k=key: apply_preset(k), desc_wrap=480)
            b.grid(row=i // 2, column=i % 2, sticky="ew", padx=(0, 8) if i % 2 == 0 else (0, 0),
                   pady=(0, 8))
            ui["ctrl_presets"].append((key, b))

        srow = UI.Frame(pbody, bg=PANEL)
        srow.pack(fill="x", pady=(2, 0))
        ui["ctrl_summary"] = label(srow, "", size=9, color=DIM, bg=PANEL, wrap=1000)
        ui["ctrl_summary"].pack(side="left", fill="x", expand=True)

        abody = card(parent, L("buttons_title"), L("ctrl_advanced_desc"),
                     help=L("buttons_desc") + "\n\n" + L("bm_note") + "\n\n" + L("rm_help"))
        build_buttons(abody, embedded=True)
        RM.RemapEditor(abody, L, FAM, COLORS,
                       dict(bmap=bmap, roles=broles, bstyle=bstyle, bplace=bplace, bsimple=bsimple, switch_target=switch_target,
                            auto_target=auto_target, sprint_toggle=sprint_l3_toggle, mshield=manual_shield))

        for var, tid in ui.get("ctrl_trace_ids", []):
            try:
                var.trace_remove("write", tid)
            except (tk.TclError, ValueError):
                pass
        ui["ctrl_trace_ids"] = []

        def refresh_controls():
            if not ui.get("ctrl_summary"):
                return
            roles = RM.roles_value(broles)
            if not bmap.get():
                roles = {b: ("none" if r in ("item1", "item2", "item3", "selector") else r)
                         for b, r in roles.items()}
            parts = []
            for b in I.REMAP_BUTTONS:
                role = roles.get(b, "none")
                if role != "none":
                    parts.append(f"{b} {L('role_' + role)}")
            ui["ctrl_summary"].config(text=L("ctrl_summary") + "  " + "  ·  ".join(parts))
            paint_presets()

        for var in (bmap, bplace, bstyle, bsimple, manual_shield, *broles.values()):
            ui["ctrl_trace_ids"].append((var, var.trace_add("write", lambda *_: refresh_controls())))

        ui["ctrl_refresh"] = refresh_controls
        refresh_controls()
        build_quick_select(parent)

    def help_check(body, text, var, tip, rb=True):
        """A switch with its explanation behind a "?"."""
        c = check_rb(body, text, var) if rb else check(body, text, var)
        if not rb:
            c.pack(anchor="w")
        RM.help_icon(c.master, tip, COLORS, FAM).pack(side="left", padx=(8, 0))
        return c

    def scale_row(body, key, var, pady=(8, 0)):
        """A HUD size slider (percent, 100 = the current size) with its explanation behind a "?"."""
        row = UI.Frame(body, bg=PANEL)
        row.pack(anchor="w", pady=pady)
        label(row, L(key) + ":", size=10, bg=PANEL).pack(side="left", padx=(0, 10))
        UI.Slider(row, var, I.HUD_SCALE_MIN, I.HUD_SCALE_MAX, I.HUD_SCALE_STEP, bg=PANEL).pack(side="left")
        RM.help_icon(row, L(key + "_desc"), COLORS, FAM).pack(side="left", padx=(8, 0))
        return row

    def build_quick_select(parent):
        body = card(parent, L("quick_title"), help=L("quick_desc") + "\n\n" + L("quick_note"))
        help_check(body, L("quick_boots_on"), quick_boots, L("quick_boots_desc"))
        help_check(body, L("quick_mask_on"), quick_mask, L("quick_mask_desc"))

    def refresh_layout_note(*_):
        note, btn = ui.get("layout_note"), ui.get("layout_dl")
        if note is None or not note.winfo_exists():
            return
        gamecube = bplace.get() == "gamecube"
        ui["game_layout"].config(state="disabled" if gamecube else "readonly")
        if gamecube:
            note.config(text=L("bm_place_gamecube_help"), fg=DIM)
            if btn is not None:
                btn.pack_forget()
            return
        style = glayout.get()
        if style == "none":
            note.config(text=L("layout_note_none"), fg=DIM)
            if btn is not None:
                btn.pack_forget()
            return
        name = L("g_" + style)
        if layouts.available(style):
            note.config(text=L("layout_note_ready", a=name), fg=ACCENT)
            if btn is not None:
                btn.pack_forget()
        else:
            note.config(text=L("layout_note_missing", a=name), fg=AMBER)
            if btn is not None and not btn.winfo_manager():
                btn.pack(side="left", padx=(8, 0))

    def download_layout(style, ask=True):
        """Download the game-wide layout with a progress window; True when it is ready."""
        if style not in layouts.FILES or layouts.available(style):
            refresh_layout_note()
            return True
        name = L("g_" + style)
        if ask and not messagebox.askyesno(L("layout_title"), L("layout_confirm", a=name), parent=root):
            return False
        win = UI.Toplevel(root)
        win.title(L("layout_title"))
        win.configure(bg=BG)
        win.transient(root)
        win.resizable(False, False)
        win.geometry("460x120")
        lbl = UI.Label(win, text=L("layout_downloading", a=name, p=0), bg=BG, fg=TEXT,
                       font=(FAM, 10), anchor="w")
        lbl.pack(fill="x", padx=16, pady=(16, 8))
        bar = UI.Progress(win, style="SHB.Horizontal.TProgressbar", mode="determinate", maximum=100)
        bar.pack(fill="x", padx=16)
        win.protocol("WM_DELETE_WINDOW", lambda: None)
        win.grab_set()
        qq = queue.Queue()
        ok = {"value": False}

        def progress(done, total):
            qq.put(("p", int(done * 100 / total) if total else 0))

        def work():
            try:
                layouts.ensure(style, progress=progress)
            except Exception:
                qq.put(("err",))
                return
            qq.put(("ok",))

        def poll():
            try:
                while True:
                    msg = qq.get_nowait()
                    if msg[0] == "p":
                        bar.configure(value=msg[1])
                        lbl.configure(text=L("layout_downloading", a=name, p=msg[1]))
                        continue
                    ok["value"] = msg[0] == "ok"
                    try:
                        win.grab_release()
                    except Exception:
                        pass
                    win.destroy()
                    return
            except queue.Empty:
                pass
            win.after(80, poll)

        threading.Thread(target=work, daemon=True).start()
        win.after(80, poll)
        win.wait_window()
        if ok["value"]:
            status.set(L("layout_note_ready", a=name))
        else:
            messagebox.showerror(L("err_title"), L("e_layout", a=name), parent=root)
        refresh_layout_note()
        return ok["value"]

    def need_layout() -> bool:
        style = glayout.get()
        return bplace.get() == "gamecube" or style == "none" or layouts.available(style) or download_layout(style)

    def build_buttons(parent, embedded=False):
        """The mod's item buttons on the HUD: on / off, their pictures and how many are drawn."""
        if embedded:
            body = parent
        else:
            body = card(parent, L("buttons_title"), help=L("buttons_desc") + "\n\n" + L("bm_note"))
        help_check(body, L("bm_on"), bmap, L("bm_on_desc"))
        ui["bm"] = []
        row = UI.Frame(body, bg=PANEL)
        row.pack(anchor="w", pady=(10, 0))
        label(row, L("variant") + ":", size=10, bg=PANEL).pack(side="left", padx=(0, 8))
        style_names = {"switch": "g_default", "playstation": "g_playstation", "xbox": "g_xbox"}
        names = [L(style_names[k]) for k in I.BUTTON_STYLES]
        vvar = tk.StringVar(value=names[I.BUTTON_STYLES.index(bstyle.get())])
        ui["variant_var"] = vvar
        ui["variant_values"] = names
        cb = combo(row, names, vvar, width=16)
        cb.pack(side="left")
        cb.bind("<<ComboboxSelected>>", lambda e: bstyle.set(I.BUTTON_STYLES[cb.current()]))
        ui["variant"] = cb
        ui["bm"].append(cb)
        RM.help_icon(row, L("variant_d"), COLORS, FAM).pack(side="left", padx=(8, 0))

        row = UI.Frame(body, bg=PANEL)
        row.pack(anchor="w", pady=(8, 0))
        label(row, L("bm_layout") + ":", size=10, bg=PANEL).pack(side="left", padx=(0, 8))
        for value, key in ((False, "bm_ext"), (True, "bm_simple")):
            r = radio(row, L(key), bsimple, value)
            r.pack(side="left")
            ui["bm"].append(r)
            RM.help_icon(row, L(key + "_d"), COLORS, FAM).pack(side="left", padx=(4, 14))
        scale_row(body, "hud_ctrl_scale", hud_ctrl_scale)
        if embedded:
            sep = tk.Frame(body, bg=BORDER, height=1)
            sep.pack(fill="x", pady=(12, 10))


    def build_enemies(parent):
        body = card(parent, L("enemies_title"), L("enemies_desc"))
        ui["enemy"] = []
        label(body, L("beta_warning"), size=9, color=RED, bold=True, bg=PANEL).pack(anchor="w", pady=(0, 6))
        label(body, L("enemy_double"), size=10, color=ACCENT, bold=True, bg=PANEL).pack(anchor="w")
        check(body, L("enemy_double_on"), enemy_double).pack(anchor="w", padx=(0, 0))
        desc_label(body, L("enemy_double_desc"), pady=(0, 8))
        label(body, L("enemy_speed"), size=10, color=ACCENT, bold=True, bg=PANEL).pack(anchor="w")
        row = UI.Frame(body, bg=PANEL)
        row.pack(fill="x", pady=(2, 0))
        names = [L("enemy_off") if l == 0 else "×%g" % I.ENEMY_FACTORS[l] for l in I.ENEMY_LEVELS]
        for i, (key, var) in enumerate((("enemy_move", enemy_move), ("enemy_anim", enemy_anim),
                                         ("enemy_attack", enemy_attack), ("enemy_proj", enemy_proj))):
            rowi, col = i // 2, (i % 2) * 2
            label(row, L(key) + ":", size=10, bg=PANEL).grid(row=rowi, column=col, sticky="w",
                                                             padx=(0, 6), pady=(0, 2))
            idx = I.ENEMY_LEVELS.index(var.get()) if var.get() in I.ENEMY_LEVELS else 0
            svar = tk.StringVar(value=names[idx])
            cb = combo(row, names, svar, width=7)
            cb.grid(row=rowi, column=col + 1, sticky="w", padx=(0, 18), pady=(0, 2))
            cb.bind("<<ComboboxSelected>>", lambda e, c=cb, v=var: v.set(I.ENEMY_LEVELS[c.current()]))
            ui["enemy"].append((cb, var))
        desc_label(body, L("enemy_speed_desc"), indent=0, pady=(6, 0))
        # aggression: alert radius (a multiplier, like the speeds) and the aggression level
        label(body, L("aggro_title"), size=10, color=ACCENT, bold=True, bg=PANEL).pack(anchor="w", pady=(10, 0))
        row = UI.Frame(body, bg=PANEL)
        row.pack(fill="x", pady=(2, 0))
        label(row, L("enemy_alert") + ":", size=10, bg=PANEL).grid(row=0, column=0, sticky="w", padx=(0, 6))
        idx = I.ENEMY_LEVELS.index(enemy_alert.get()) if enemy_alert.get() in I.ENEMY_LEVELS else 0
        cb = combo(row, names, tk.StringVar(value=names[idx]), width=7)
        cb.grid(row=0, column=1, sticky="w")
        cb.bind("<<ComboboxSelected>>", lambda e, c=cb: enemy_alert.set(I.ENEMY_LEVELS[c.current()]))
        ui["enemy"].append((cb, enemy_alert))
        RM.help_icon(row, L("enemy_alert_help"), COLORS, FAM).grid(row=0, column=2, sticky="w", padx=(4, 18))
        label(row, L("enemy_aggro") + ":", size=10, bg=PANEL).grid(row=0, column=3, sticky="w", padx=(0, 6))
        anames = [L("aggro_%d" % k) for k in I.AGGRO_LEVELS]
        acb = combo(row, anames, tk.StringVar(value=anames[enemy_aggro.get() if enemy_aggro.get() in I.AGGRO_LEVELS else 0]),
                    width=10)
        acb.grid(row=0, column=4, sticky="w")
        acb.bind("<<ComboboxSelected>>", lambda e: enemy_aggro.set(I.AGGRO_LEVELS[acb.current()]))
        ui["aggro"] = acb
        RM.help_icon(row, L("enemy_aggro_help"), COLORS, FAM).grid(row=0, column=5, sticky="w", padx=(4, 0))
        label(body, L("boss_speed"), size=10, color=ACCENT, bold=True, bg=PANEL).pack(anchor="w", pady=(10, 0))
        row = UI.Frame(body, bg=PANEL)
        row.pack(fill="x", pady=(2, 0))
        for i, (key, var) in enumerate((("enemy_move", boss_move), ("enemy_anim", boss_anim))):
            label(row, L(key) + ":", size=10, bg=PANEL).grid(row=0, column=i * 2, sticky="w", padx=(0, 6), pady=(0, 2))
            idx = I.ENEMY_LEVELS.index(var.get()) if var.get() in I.ENEMY_LEVELS else 0
            svar = tk.StringVar(value=names[idx])
            cb = combo(row, names, svar, width=7)
            cb.grid(row=0, column=i * 2 + 1, sticky="w", padx=(0, 18), pady=(0, 2))
            cb.bind("<<ComboboxSelected>>", lambda e, c=cb, v=var: v.set(I.ENEMY_LEVELS[c.current()]))
            ui["enemy"].append((cb, var))
        label(body, L("boss_speed_desc"), size=9, color=DIM, bg=PANEL).pack(anchor="w", pady=(6, 0))
        label(body, L("enemy_beta_note"), size=9, color=AMBER, bg=PANEL).pack(anchor="w", pady=(8, 0))

    def build_misc(parent):
        body = card(parent, L("misc_title"), L("misc_desc"))
        label(body, L("songbook_title"), size=11, color=ACCENT, bold=True, bg=PANEL).pack(anchor="w", pady=(2, 0))
        check_rb(body, L("songbook_on"), songbook, pady=(2, 0))
        desc_label(body, L("songbook_desc"))
        label(body, L("songbook_note"), size=9, color=AMBER, bg=PANEL).pack(anchor="w", pady=(8, 0))

        body = card(parent, L("world_beta_title"), L("world_beta_desc"))
        label(body, L("great_fairy"), size=10, color=ACCENT, bold=True, bg=PANEL).pack(anchor="w")
        check(body, L("great_fairy_on"), great_fairy).pack(anchor="w")
        desc_label(body, L("great_fairy_desc"), pady=(0, 8))
        label(body, L("farore"), size=10, color=ACCENT, bold=True, bg=PANEL).pack(anchor="w")
        check(body, L("farore_on"), farore_outdoor).pack(anchor="w")
        desc_label(body, L("farore_desc"))

    def open_github():
        import webbrowser
        webbrowser.open("https://github.com/kuribox/OOT-SuperHeroBoostMod")

    def build_about(parent):
        body = card(parent, L("about_title"))
        label(body, f"SHB-OcarinaTweaks {VERSION}", size=10, color=ACCENT, bold=True, bg=PANEL).pack(anchor="w")
        upd = UI.Label(body, text=L("update_check"), bg=PANEL, fg=ACCENT, cursor="hand2",
                       font=(FAM, 9, "underline"), anchor="w", justify="left")
        upd.pack(anchor="w", pady=(2, 0))
        upd.bind("<Button-1>", lambda e: check_updates(manual=True))
        label(body, L("about_body"), size=10, bg=PANEL).pack(anchor="w")
        label(body, L("about_credits"), size=9, color=DIM, bg=PANEL).pack(anchor="w", pady=(8, 0))
        link = UI.Label(body, text=L("about_link"), bg=PANEL, fg=ACCENT, cursor="hand2",
                        font=(FAM, 9, "underline"), anchor="w", justify="left")
        link.pack(anchor="w", pady=(8, 0))
        link.bind("<Button-1>", lambda e: open_github())
        label(body, L("audit"), size=9, color=DIM, bg=PANEL).pack(anchor="w", pady=(6, 0))

    # ---- shell ----
    def build_header(shell):
        hd = UI.Frame(shell, bg=BG)
        hd.pack(fill="x", padx=18, pady=(14, 8))
        row = UI.Frame(hd, bg=BG)
        row.pack(fill="x")
        left = UI.Frame(row, bg=BG)
        left.pack(side="left", fill="x", expand=True)
        brand = UI.Frame(left, bg=BG)
        brand.pack(anchor="w")
        logo = tk.Canvas(brand, width=58, height=48, bg=BG, highlightthickness=0)
        logo.pack(side="left", padx=(0, 12))
        emblem(logo, 29, 25, 30)
        names = UI.Frame(brand, bg=BG)
        names.pack(side="left")
        UI.Label(names, text="SHB-OcarinaTweaks", bg=BG, fg=ACCENT, font=(DISPLAY, 24, "bold")).pack(anchor="w")
        UI.Label(names, text=L("subtitle"), bg=BG, fg=DIM, font=(FAM, 10)).pack(anchor="w")
        right = UI.Frame(row, bg=BG)
        right.pack(side="right")
        ui["profile_badge"] = UI.Chip(right, text="")
        ui["profile_badge"].grid(row=0, column=0, sticky="e", padx=(0, 18))
        UI.Label(right, text=L("language"), bg=BG, fg=DIM, font=(FAM, 9)).grid(row=0, column=1, sticky="e", padx=(0, 6))
        lang_display = tk.StringVar(value=LANGS.get(lang.get(), list(LANGS.values())[0]))
        lcb = UI.Select(right, list(LANGS.values()), lang_display, width=12)
        lcb.current(list(LANGS).index(lang.get()) if lang.get() in LANGS else 0)
        lcb.grid(row=0, column=2)
        lcb.bind("<<ComboboxSelected>>", lambda e: change_lang(list(LANGS)[lcb.current()]))
        ui["update_bar"] = UI.Frame(hd, bg=PANEL2, highlightbackground=BORDER, highlightthickness=1)
        ui["update_label"] = UI.Label(ui["update_bar"], text="", bg=PANEL2, fg=TEXT, font=(FAM, 9),
                                      anchor="w", justify="left", wraplength=560)
        ui["update_label"].pack(side="left", fill="x", expand=True, padx=(12, 8), pady=7)
        button(ui["update_bar"], L("update_download"), download_update, primary=True).pack(
            side="right", padx=(0, 8), pady=5)
        button(ui["update_bar"], L("update_later"), dismiss_update).pack(side="right", padx=(0, 6), pady=5)
        if state.get("update"):
            show_update_bar()

    def change_lang(code):
        lang.set(code)
        remember()
        render()
        # The mod draws a few labels of its own (the jump label, the A fallbacks and the assign legend). They follow
        # the game's language; only when that cannot be read does the app's language decide, so refresh the byte of
        # an already installed module (it applies on the next game start).
        try:
            if target.get() == "emulator" and emu.get().strip():
                mods = ok_emu()
                e = Path(emu.get().strip()).expanduser()
                if mods:
                    I.set_installed_lang(mods, I.MOD_NAME, I.detect_lang(e) or code)
            elif target.get() == "rekarina":
                game = ok_rekarina()
                if game and RK.is_built(game):
                    RK.apply_settings(game, settings())
        except Exception:
            pass

    def build_sidebar(shell):
        side = UI.Frame(shell, bg=SIDEBAR, width=204)
        side.pack(side="left", fill="y", padx=(10, 0), pady=(0, 10))
        side.pack_propagate(False)
        nav = (("install", "nav_install"), ("home", "nav_home"), ("stamina", "nav_stamina"),
               ("difficulty", "nav_difficulty"), ("controls", "nav_controls"), ("hud", "nav_hud"),
               ("misc", "nav_misc"), ("about", "nav_about"))
        for key, text in nav:
            b = ctk.CTkButton(side, text="   " + L(text), command=lambda k=key: show_section(k),
                              anchor="w", fg_color="transparent", hover_color=PANEL, text_color=TEXT,
                              corner_radius=10, height=40, font=(FAM, 10, "bold"))
            b.pack(fill="x", padx=8, pady=1)
            ui["nav"][key] = b
        return side

    def build_content(shell):
        wrap = UI.Frame(shell, bg=BG)
        wrap.pack(side="left", fill="both", expand=True, padx=(12, 12), pady=(0, 8))
        banner = UI.HelpBanner(wrap)
        ui["help_banner"] = banner
        scroll = ScrollArea(wrap, BG)
        scroll.frame.pack(fill="both", expand=True)

        def show_help(title, text):
            banner.show(title, text)
            if not banner.winfo_manager():
                banner.pack(fill="x", padx=4, pady=(0, 6), before=scroll.frame)
            ui["_scroll"].refresh()

        UI.set_help_handler(show_help)
        builders = {"home": build_home, "install": build_install, "stamina": build_stamina,
                    "difficulty": build_difficulty, "controls": build_controls, "hud": build_hud,
                    "misc": build_misc, "about": build_about}
        for key, fn in builders.items():
            f = UI.Frame(scroll.inner, bg=BG)
            fn(f)
            ui["sections"][key] = f
        ui["_scroll"] = scroll
        return scroll

    def run_recompile():
        """Only recompile: the mod files are not copied (hand changes in the game folder stay)."""
        if target.get() == "rekarina":
            run_rekarina("recompile")
        else:
            run_recomp("recompile")

    def run_reinstall():
        """Uninstall, copy everything again and recompile."""
        if target.get() == "rekarina":
            run_rekarina("reinstall")
        else:
            run_recomp("reinstall")

    def build_footer(shell):
        ft = UI.Frame(shell, bg=BG)
        ft.pack(side="bottom", fill="x", padx=18, pady=(0, 12))
        UI.Label(ft, textvariable=status, bg=BG, fg=ACCENT, font=(FAM, 9, "italic"), anchor="w").pack(side="left")
        ui["install"] = button(ft, L("install"), do_install, primary=True)
        ui["install"].pack(side="right")
        ui["uninstall"] = button(ft, L("uninstall"), do_remove)
        ui["uninstall"].pack(side="right", padx=(0, 8))
        ui["launch"] = button(ft, L("launch"), do_launch)
        ui["launch"].pack(side="right", padx=(0, 8))
        # Shown only for a Hookshot/Longshot target (refresh): rebuild, builder and cancel.
        ui["rebuild"] = button(ft, L("recomp_recompile"), run_recompile)
        ui["reinstall"] = button(ft, L("recomp_reinstall"), run_reinstall)
        ui["builder_btn"] = button(ft, L("recomp_builder_btn"), browse_builder)
        ui["cancel"] = button(ft, L("recomp_cancel"), lambda: (R.cancel(), RK.cancel()))

    def show_section(key):
        state["section"] = key
        banner = ui.get("help_banner")
        if banner is not None:
            banner.hide()
        for k, f in ui["sections"].items():
            f.pack_forget()
        ui["sections"][key].pack(fill="x", padx=4, pady=(4, 0))
        ui["_scroll"].refresh()
        ui["_scroll"].canvas.yview_moveto(0)
        for k, b in ui["nav"].items():
            b.configure(fg_color=PANEL if k == key else "transparent",
                        text_color=ACCENT if k == key else TEXT)

    def render():
        UI.HELP_TITLE = L("help_title")
        old = ui.get("_scroll")
        if old is not None:
            old.destroy()
        for w in root.winfo_children():
            w.destroy()
        ui["nav"] = {}
        ui["sections"] = {}
        ui["rb_tags"] = []
        shell = UI.Frame(root, bg=BG)
        shell.pack(fill="both", expand=True)
        build_header(shell)
        build_footer(shell)
        body = UI.Frame(shell, bg=BG)
        body.pack(side="top", fill="both", expand=True)
        build_sidebar(body)
        build_content(body)
        show_section(state["section"])
        refresh()

    # New installations start from the flagship profile; saved/legacy settings keep their values.
    if not cfg:
        profile.set(DEFAULT_PROFILE)
        apply_profile(DEFAULT_PROFILE, save=False)
    elif "profile" not in cfg:
        profile.set("custom")
    elif profile.get() != "custom":
        apply_profile(profile.get(), save=False)

    for v in (emu, bmap, bstyle, bplace, glayout, bsimple, blmode, stamina, songbook, quick_boots, quick_mask, hud_collectibles,
              target_r3, sprint_l3, sprint_l3_toggle,
              enemy_double, enemy_move, enemy_anim, enemy_attack, enemy_proj, boss_move, boss_anim, enemy_alert, enemy_aggro, stamina_bar,
              great_fairy, farore_outdoor, mode, combat, hp, dmg, speed,
              nodrop, rupees, clock_mode, hud_ctrl_scale, hud_count_scale, *costs.values()):
        v.trace_add("write", autosave)
    emu.trace_add("write", lambda *_: (status.set(""), refresh()))
    recomp_path.trace_add("write", lambda *_: (status.set(""), refresh()))
    recomp_builder.trace_add("write", lambda *_: (autosave(), refresh()))
    rekarina_path.trace_add("write", lambda *_: (status.set(""), refresh()))

    render()
    start = os.environ.get("SHB_UI_START")
    if start in ui["sections"]:
        show_section(start)
    if os.environ.get("SHB_UI_SCROLL"):
        try:
            ui["_scroll"].canvas.yview_moveto(float(os.environ["SHB_UI_SCROLL"]))
        except Exception:
            pass
    root.after(80, poll)
    root.after(1800, lambda: check_updates(manual=False))
    root.protocol("WM_DELETE_WINDOW", lambda: (remember(), root.destroy()))
    root.mainloop()


if __name__ == "__main__":
    if any(a in sys.argv for a in ("--cli", "--list-profiles", "-h", "--help")):
        raise SystemExit(cli(sys.argv[1:]))
    gui()
