"""Game profiles ("presets") of Super Hero Boost.

Every profile is a complete configuration: it always includes the HUD and
buttonMapping (our own item buttons), and it fixes the stamina system, difficulty, heart drops,
sprint speed and the enemy mods. Changing any option afterwards switches the profile to
Custom and keeps the user's own values.

The default profile is the author's own setup (200 % enemy health and damage, hardcore stamina,
duplicated enemies at x2 with aggressive bosses, the simple HUD, the targeting assists and the
R3/L3 controls); the rest of the profiles are that setup scaled up or down.
"""
from dataclasses import dataclass
from typing import Optional

KEYS = ("jump", "attack", "sprint", "block", "shoot", "roll")
STAMINA_MODES = ("hardcore", "combat", "sprint", "advanced")   # value written to the config block
SPRINT_DEFAULT = 125     # "custom" starts here: a gentle +25 %
SPRINT_PROFILE = 125     # every named profile: the same +25 % (x1.25)


@dataclass(frozen=True)
class Profile:
    key: str
    button_mapping: bool         # buttonMapping (own item buttons); style, layout and L mode are not part of a profile
    stamina: bool
    mode: int                    # index in STAMINA_MODES
    costs: tuple                 # one bool per KEYS entry
    combat_only: bool
    enemy_hp: int                # percent of vanilla
    damage: int                  # percent of vanilla
    no_heart_drop: bool
    sprint_speed: int            # percent of vanilla
    hud_rupees: bool
    hud_clock: bool
    clock_24h: bool
    songbook: bool
    quick_boots: bool            # hold D-pad down to pick boots
    quick_mask: bool             # hold D-pad up to pick a mask
    hud_collectibles: bool       # per-area collectible counter
    hud_mode: str                # "wide" / "simple" / "plain"
    enemy_double: bool           # beta options: a named profile starts with them off
    enemy_move: int              # 0 off, 1 x1.5, 2 x2, 3 x3 ...
    enemy_anim: int
    enemy_attack: int
    enemy_proj: int
    enemy_alert: int             # how far a spotting enemy alerts the others
    enemy_aggro: int             # 0 normal, 1 aggressive, 2 maximum
    boss_move: int               # bosses and minibosses, same levels as the enemies
    boss_anim: int
    great_fairy: bool
    farore_outdoor: bool
    stamina_bar: Optional[bool]  # stamina UI: horizontal bar instead of the ring (None: keep the user's choice)
    manual_shield: bool          # ZL follows the game's Hold/Toggle setting, L guards while targeting
    auto_target: bool            # when the locked enemy dies, lock on to the next one
    switch_target: bool          # tap the target button to switch to the red-marked enemy, hold to drop it
    target_r3: Optional[bool]    # target with R3 (ZL/R3 swap); None: a personal preference the profile keeps
    sprint_l3: Optional[bool]    # hold L3 to sprint; None: a personal preference the profile keeps
    sprint_l3_toggle: Optional[bool]  # with sprint_l3: L3 turns the sprint on / off; None: keeps the user's choice
    armory: bool = False         # holding the D-pad's right arrow opens the armory (sword and shield drawer)
    controls: Optional[str] = None   # a button table of installer.REMAP_PRESETS the profile applies (None: the user's own)


def _profile(key, *, button_mapping=True, stamina=True, mode=0, costs=(True,) * 6, combat_only=False,
             enemy_hp=100, damage=100, no_heart_drop=False, sprint_speed=SPRINT_PROFILE,
             hud_rupees=True, hud_clock=True, clock_24h=True, songbook=True,
             quick_boots=True, quick_mask=True, hud_collectibles=False, hud_mode="simple",
             enemy_double=False, enemy_move=0, enemy_anim=0, enemy_attack=0, enemy_proj=0,
             enemy_alert=0, enemy_aggro=0, boss_move=0, boss_anim=0,
             great_fairy=False, farore_outdoor=False, stamina_bar=None, manual_shield=False,
             auto_target=False, switch_target=False, target_r3=None, sprint_l3=None,
             sprint_l3_toggle=None, armory=None, controls=None):
    if armory is None:   # every profile that has the pack's buttons has the armory on the held D-pad right
        armory = button_mapping
    return Profile(key=key, button_mapping=button_mapping, stamina=stamina, mode=mode, costs=costs,
                   combat_only=combat_only, enemy_hp=enemy_hp, damage=damage,
                   no_heart_drop=no_heart_drop, sprint_speed=sprint_speed,
                   hud_rupees=hud_rupees, hud_clock=hud_clock, clock_24h=clock_24h,
                   songbook=songbook, quick_boots=quick_boots, quick_mask=quick_mask,
                   hud_collectibles=hud_collectibles, hud_mode=hud_mode,
                   enemy_double=enemy_double, enemy_move=enemy_move, enemy_anim=enemy_anim,
                   enemy_attack=enemy_attack, enemy_proj=enemy_proj,
                   enemy_alert=enemy_alert, enemy_aggro=enemy_aggro,
                   boss_move=boss_move, boss_anim=boss_anim,
                   great_fairy=great_fairy, farore_outdoor=farore_outdoor, stamina_bar=stamina_bar,
                   manual_shield=manual_shield, auto_target=auto_target, switch_target=switch_target,
                   target_r3=target_r3, sprint_l3=sprint_l3, sprint_l3_toggle=sprint_l3_toggle,
                   armory=armory, controls=controls)


# The targeting improvements every combat profile with the manual shield gets (both need the game's Targeting
# Type on Toggle; the app says so).
_TARGETING = dict(manual_shield=True, auto_target=True, switch_target=True)
# Souls-like controls: stamina on every action shown as a bar, lock-on on R3 that follows the dead to the next
# enemy and switches with a tap, and the manual shield on L.
_SOULS = dict(_TARGETING, stamina_bar=True, target_r3=True)
# The beta enemy mods of the default profile: duplicated placements, every component and the bosses at x2, and
# enemies that alert the others a little further.
_ENEMY_FAST = dict(enemy_double=True, enemy_move=2, enemy_anim=2, enemy_attack=2, enemy_proj=2,
                   enemy_alert=1, enemy_aggro=1, boss_move=2, boss_anim=2)
# The tuned controls of the default: R3 lock-on and L3 sprint as a toggle on top of the targeting assists.
# They come with the author's button table ("triggers": items on the triggers, the item selector on B).
_TUNED_CONTROLS = dict(_TARGETING, target_r3=True, sprint_l3=True, sprint_l3_toggle=True, controls="triggers")


PROFILES = {
    # The default: the author's own setup. x2 enemy health and damage, no heart drops, hardcore stamina, the beta
    # enemy mods, the simple HUD and every control improvement of the pack. Back up the save: the duplication is
    # experimental.
    "recommended": _profile("recommended", enemy_hp=200, damage=200, no_heart_drop=True,
                            hud_collectibles=True, great_fairy=True, farore_outdoor=True,
                            stamina_bar=False, **_ENEMY_FAST, **_TUNED_CONTROLS),
    # The same setup without the experimental enemy mods: a first run, a weaker PC, or the puzzles that the
    # duplicated enemies can break.
    "recommended-lite": _profile("recommended-lite", enemy_hp=200, damage=200, no_heart_drop=True,
                                 hud_collectibles=True, great_fairy=True, farore_outdoor=True,
                                 **_TUNED_CONTROLS),
    # Souls-like controls on the vanilla difficulty: stamina bar, R3 lock-on, switch/next target, manual shield.
    "souls": _profile("souls", hud_collectibles=True, **_SOULS),
    # The same controls at the default difficulty: x2 enemy health and damage, no heart drops.
    "souls-hero": _profile("souls-hero", enemy_hp=200, damage=200, no_heart_drop=True,
                           hud_collectibles=True, **_SOULS),
    # Vanilla difficulty, but with the pack's HUD and extra buttons.
    "base": _profile("base", stamina=False, hud_collectibles=True),
    # Only the manual shield: vanilla difficulty/HUD/controls off, no stamina, no extra buttons.
    # The wide layout keeps its element switches, so with every counter off nothing is drawn (the "simple"
    # row always shows the four counters).
    "manual-shield": _profile("manual-shield", stamina=False, button_mapping=False, songbook=False,
                              hud_rupees=False, hud_clock=False, hud_mode="wide", quick_boots=False,
                              quick_mask=False, hud_collectibles=False, manual_shield=True),
    # Classic Hero Mode: x1.5 enemy health, x2 damage, no hearts, no stamina system.
    "hero": _profile("hero", stamina=False, enemy_hp=150, damage=200, no_heart_drop=True,
                     hud_collectibles=True, **_TARGETING),
    # Hero Mode plus stamina on every action.
    "hero-stamina": _profile("hero-stamina", enemy_hp=150, damage=200, no_heart_drop=True,
                             hud_collectibles=True, **_TARGETING),
    # Hero Mode with stamina only on sprint.
    "hero-explorer": _profile("hero-explorer", mode=2, enemy_hp=150, damage=200, no_heart_drop=True,
                              hud_collectibles=True, costs=(False, False, True, False, False, False),
                              **_TARGETING),
    # Above the default: x2.5 enemy health and damage and every enemy at x1.5, without duplicating placements.
    "super-hero": _profile("super-hero", enemy_hp=250, damage=250, no_heart_drop=True,
                           hud_collectibles=True, great_fairy=True, farore_outdoor=True,
                           enemy_move=1, enemy_anim=1, enemy_attack=1, enemy_proj=1,
                           boss_move=1, boss_anim=1, **_TUNED_CONTROLS),
    # The default with x3 numbers and every enemy mod at x2.
    "ultra-hero": _profile("ultra-hero", enemy_hp=300, damage=300, no_heart_drop=True,
                           hud_collectibles=True, great_fairy=True, farore_outdoor=True,
                           **_ENEMY_FAST, **_TUNED_CONTROLS),
    # Endurance run: the default difficulty and hardcore stamina, without the enemy mods.
    "survival": _profile("survival", enemy_hp=200, damage=200, no_heart_drop=True,
                         hud_collectibles=True, **_TARGETING),
    # Only the world changes (Great Fairy rewards + Farore's Wind outdoors) on vanilla difficulty.
    "world": _profile("world", stamina=False, hud_collectibles=True, great_fairy=True,
                      farore_outdoor=True),
    # Beta options: the enemy mods on top of the vanilla game (all components x2 plus the duplication).
    "beta-enemies": _profile("beta-enemies", stamina=False, hud_collectibles=True, **_ENEMY_FAST),
    # Beta options: the experimental ceiling, x3 numbers and everything at x3 with the most aggressive enemies.
    "beta-all": _profile("beta-all", enemy_hp=300, damage=300, no_heart_drop=True,
                         hud_collectibles=True, great_fairy=True, farore_outdoor=True,
                         enemy_double=True, enemy_move=3, enemy_anim=3, enemy_attack=3, enemy_proj=3,
                         enemy_alert=2, enemy_aggro=2, boss_move=3, boss_anim=3,
                         **_TUNED_CONTROLS),
}

PROFILE_KEYS = ("recommended", "recommended-lite", "souls", "souls-hero", "base", "manual-shield", "hero",
                "hero-stamina", "hero-explorer", "super-hero", "ultra-hero", "survival", "world",
                "beta-enemies", "beta-all", "custom")
DEFAULT_PROFILE = "recommended"


def defaults() -> dict:
    """Balanced starting values (same shape as a profile, without a fixed key)."""
    return dict(button_mapping=True, stamina=True, mode=0, costs=(True,) * 6, combat_only=False,
                enemy_hp=100, damage=100, no_heart_drop=False, sprint_speed=SPRINT_DEFAULT,
                hud_rupees=True, hud_clock=True, clock_24h=True, songbook=True,
                quick_boots=True, quick_mask=True, hud_collectibles=False, hud_mode="wide",
                enemy_double=False, enemy_move=0, enemy_anim=0, enemy_attack=0, enemy_proj=0,
                enemy_alert=0, enemy_aggro=0, boss_move=0, boss_anim=0,
                great_fairy=False, farore_outdoor=False, stamina_bar=None, manual_shield=False,
                auto_target=False, switch_target=False, target_r3=None, sprint_l3=None,
                sprint_l3_toggle=None, armory=False, controls=None)


def profile_values(key: str) -> dict:
    """Settings kwargs for a profile. 'custom' returns the balanced defaults."""
    p = PROFILES.get(key)
    if p is None:
        return defaults()
    return dict(button_mapping=p.button_mapping, stamina=p.stamina, mode=p.mode, costs=p.costs,
                combat_only=p.combat_only, enemy_hp=p.enemy_hp, damage=p.damage,
                no_heart_drop=p.no_heart_drop, sprint_speed=p.sprint_speed,
                hud_rupees=p.hud_rupees, hud_clock=p.hud_clock, clock_24h=p.clock_24h,
                songbook=p.songbook, quick_boots=p.quick_boots, quick_mask=p.quick_mask,
                hud_collectibles=p.hud_collectibles, hud_mode=p.hud_mode, enemy_double=p.enemy_double,
                enemy_move=p.enemy_move, enemy_anim=p.enemy_anim, enemy_attack=p.enemy_attack,
                enemy_proj=p.enemy_proj, enemy_alert=p.enemy_alert, enemy_aggro=p.enemy_aggro,
                boss_move=p.boss_move, boss_anim=p.boss_anim,
                great_fairy=p.great_fairy, farore_outdoor=p.farore_outdoor,
                stamina_bar=p.stamina_bar, manual_shield=p.manual_shield,
                auto_target=p.auto_target, switch_target=p.switch_target,
                target_r3=p.target_r3, sprint_l3=p.sprint_l3,
                sprint_l3_toggle=p.sprint_l3_toggle, armory=p.armory, controls=p.controls)


def settings_kwargs(key: str) -> dict:
    """Keyword arguments for installer.Settings.

    The app window, the command line and the presets pack all build their Settings from here, so a profile
    cannot mean one thing in the window and another in the zip.
    """
    import installer as I   # (late: the installer does not import the profiles)
    v = profile_values(key)
    costs = dict(zip(KEYS, v["costs"]))
    table = I.REMAP_PRESETS[v["controls"]] if v["controls"] else I.CLASSIC_ROLES
    return dict(bm_roles=I.roles_with_armory(table) if v["armory"] else dict(I.normalize_roles(table)), button_mapping=v["button_mapping"], stamina=v["stamina"], songbook=v["songbook"],
                preset=v["mode"], combat_only=v["combat_only"], **costs,
                enemy_hp=v["enemy_hp"], damage=v["damage"], no_heart_drop=v["no_heart_drop"],
                sprint_speed=v["sprint_speed"], hud_rupees=v["hud_rupees"], hud_clock=v["hud_clock"],
                clock_24h=v["clock_24h"], quick_boots=v["quick_boots"], quick_mask=v["quick_mask"],
                hud_collectibles=v["hud_collectibles"], hud_mode=v["hud_mode"],
                enemy_double=v["enemy_double"], enemy_move=v["enemy_move"], enemy_anim=v["enemy_anim"],
                enemy_attack=v["enemy_attack"], enemy_proj=v["enemy_proj"],
                enemy_alert=v["enemy_alert"], enemy_aggro=v["enemy_aggro"],
                boss_move=v["boss_move"], boss_anim=v["boss_anim"],
                great_fairy=v["great_fairy"], farore_outdoor=v["farore_outdoor"],
                stamina_bar=bool(v["stamina_bar"]), manual_shield=v["manual_shield"],
                auto_target=v["auto_target"], switch_target=v["switch_target"],
                target_r3=bool(v["target_r3"]), sprint_l3=bool(v["sprint_l3"]),
                sprint_l3_toggle=bool(v["sprint_l3_toggle"]))
