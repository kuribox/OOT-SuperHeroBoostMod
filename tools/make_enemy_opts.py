"""Build the beta enemy mods from the user's own game dump.

Output (installed by the app only when the corresponding option is enabled):
  app/payload/romfs_opts/enemy_double/...      duplicated generic enemies (Banc placements + field auto-spawn)
  app/payload/romfs_opts/enemy_speed_base/...  original enemy packs (movement, attack cadence and projectile
                                               speeds patched at install time)
  app/payload/romfs_opts/enemy_anim_x15/...    x1.5 animation playback (ASB frame controllers)
  app/payload/romfs_opts/enemy_anim_x2/...     x2 animation playback
  app/payload/romfs_opts/enemy_anim_x3/...     x3 animation playback
  app/payload/romfs_opts/enemy_speed_manifest.json  float offsets of the base packs (k: m/a/p)
  app/payload/romfs_opts/enemy_speed_pack.zsdic     dictionary used to recompress the packs

Movement and attack values live in the same pack for most enemies, so shipping one folder per
combination would repeat ~7.6 MB sixteen times. The app applies the chosen multipliers to the
base packs at install time; the manifest says which floats are movement (m), attack intervals (a)
and projectiles (p). Animations are separate ASB files and are shipped as ready-made folders.

The movement set scales every base speed including JogSpeed, the attack motion speeds
(charge/trans/surprise/fly) and the per-enemy attack move speeds, so a fast enemy does not slide
or overshoot. The attack set scales the interval tables/fields and also the AI action timeouts
(AttackStartTimeoutFrames, TimeoutTime, BackWalkTime...) as requested; note that changing those
timeouts can make the AI less predictable.
"""
import copy
import json
import os
import random
import struct
import sys
import zstandard
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEVTOOL = Path(os.environ.get("SHB_DEVTOOL", r"F:\project\oot\vendor\devtool\oot-remake-devtool-main"))
ROMFS = Path(os.environ.get("SHB_ROMFS", r"F:\switch2\Longshot\romfs"))
OUT = ROOT / "app" / "payload" / "romfs_opts"
DICT_CACHE = ROOT / "build" / "enemy_opts_dicts"
RESTBL_REL = "System/Resource/ResourceSizeTable.Product.111.Nin_Ounce_NVN2.rsizetable.zs"
FACTORS = {1: 1.5, 2: 2.0, 3: 3.0, 4: 1.25, 5: 1.75, 6: 2.5, 7: 4.0, 8: 5.0, 9: 6.0,
           10: 0.5, 11: 0.75}
ANIM_DIR = {1: "enemy_anim_x15", 2: "enemy_anim_x2", 3: "enemy_anim_x3",
            4: "enemy_anim_x125", 5: "enemy_anim_x175", 6: "enemy_anim_x25",
            7: "enemy_anim_x4", 8: "enemy_anim_x5", 9: "enemy_anim_x6",
            10: "enemy_anim_x05", 11: "enemy_anim_x075"}
BASE_DIR = "enemy_speed_base"
# Bosses and minibosses: their own option (the enemy options leave them out). Same files, own folders.
BOSS_KEYS = ("DungeonBoss", "MiddleBoss", "IronKnuckle", "DeadHand", "Guay_Boss", "PhantomGanonHorse")
BOSS_ANIM_DIR = {k: v.replace("enemy_anim_", "boss_anim_") for k, v in ANIM_DIR.items()}
BOSS_BASE_DIR = "boss_speed_base"
BOSS_MANIFEST = OUT / "boss_speed_manifest.json"


def is_boss(stem):
    return stem.startswith("Enemy_") and any(k in stem for k in BOSS_KEYS) and "GhostPicture" not in stem
MANIFEST = OUT / "enemy_speed_manifest.json"
PACK_DICT = OUT / "enemy_speed_pack.zsdic"

sys.path.insert(0, str(ROOT / "tools"))
sys.path.insert(0, str(DEVTOOL))
from lib import sarc  # noqa: E402
import byml  # noqa: E402

# Boss / miniboss / scripted or otherwise special actors. Generic enemies pass.
BLOCK = (
    "Boss", "boss", "Ganon", "Gohma", "Dodongo", "Barinade", "PhantomGanon", "Volvagia",
    "Morpha", "BongoBongo", "Twinrova", "Kotake", "Koume", "IronKnuckle", "DeadHand",
    "FlareDancer", "BigOcto", "Naboru", "Meg", "DarkLink", "Skulltula_Gold",
    "Scrub_Business", "Scrub_Mad", "GerudoThief", "WolfosStage", "StalfosStage",
    "LizalfosStage", "DinolfosStage", "EnemyMuseum", "Amy", "Beth", "Joelle",
)
# Enemies that are part of the stage itself: a clone a couple of metres away is a door or a floor tile in the
# wrong place, or a tentacle out of its wall. (Wall and ceiling enemies are found by their rotation, see is_upright.)
NO_DOUBLE = ("TrickyDoor", "CrazyFloorTile", "Tentacle_", "Skullwalltula")
BLOCK_BANCS = (
    "Banc/GameBancParam", "Banc/BancBasedResource", "Banc/DreamWorld",
    "Banc/BossRushIntervalRoom", "Banc/EnemyMuseum", "Banc/FinalBattleField",
    "Banc/CrumblingGanonCastle", "Banc/SageRoom", "Banc/Demo_",
)
MOVE_FIELDS = ("/WalkSpeed", "/RunSpeed", "/DashSpeed", "/EscapeSpeed", "/SpecialMoveSpeed",
               "/StrafeSpeed", "/LiftWalkSpeed", "/WalkBackwardsSpeed", "/JogSpeed")
# Attack motions whose speed must follow the base movement: scaling these together is what keeps a fast enemy
# from sliding/overshooting while it winds up or charges.
ACTION_MOVE_FIELDS = ("/ChargeAttackMoveSpeed", "/AttackTransSpeed", "/SurpriseAttackMoveSpeed",
                      "/AttackFlySpeed")
GAME_MOVE_FIELDS = MOVE_FIELDS + ("/AttackMoveSpeed", "/LandAttackMoveSpeed", "/ChargeAttackMoveSpeed",
                                  "/SurpriseAttackMoveSpeed", "/AttackFlySpeed", "/Attack1stSpeed",
                                  "/Attack2ndSpeed", "/AttackSpeed", "/AttackSpeedInWater",
                                  "/AttackJumpSpeed", "/AttackHitSpeed")
# Enemy projectiles (a separate app option; they are not part of the enemy's walking speed).
PROJECTILE_FIELDS = ("BulletSpeed", "ShootInitialSpeed", "RangedAttackInitSpeed",
                     "SecondBulletFireSpeed", "LightBallSpeed", "BulletMaxSpeed", "BulletMinSpeed")
# Attack cadence: the interval tables and per-enemy interval fields. The AI action timeouts
# (AttackStartTimeoutFrames, TimeoutTime, BackWalkTime...) are deliberately NOT scaled here: changing them is
# what makes the old speed mod unstable. Exact names, so ChangeBrainNotAttackInterval stays alone.
ATTACK_INTERVAL_NAMES = frozenset((
    "AttackInterval", "RangedAttackInterval", "AttackIntervalRandom", "AttackIntervalIsNotOnBasePos",
    "AttackIntervalOutUserPerception", "PreAttackIntervalTime", "AfterAttackWaitFrame",
    "BeforeAttackWaitFrame", "BasicAttackBeforeInterval", "CloseRangeAttackInterval",
    "MiddleRangeAttackInterval", "BulletFireEmitInterval",
))
ATTACK_TABLE_FIELDS = ("/LongIntervalMax", "/LongIntervalMin", "/MediumIntervalMax",
                       "/MediumIntervalMin", "/ShortIntervalMax", "/ShortIntervalMin")
# Aggression (the app's "alert radius" and "aggression" options):
#   d  how far an enemy that spots Link alerts the others (EnemyCommonParam)
#   k  how far it keeps chasing before it gives up (EnemyCommonParam)
#   g  the shared pause during which only one enemy attacks Link at a time (GameBalanceClientParam)
#   i  how long a melee enemy must have been on screen before it fights (GameBalanceClientParam)
#   o  bools: wait until on screen before fighting (GameBalanceClientParam); patched to false
ALERT_FIELDS = ("DiscoverSpreadDist", "DiscoverSpreadDistExtend")
PURSUIT_FIELDS = ("KeepBattleDistance",)
TURN_FIELDS = ("GlobalPlayerNoAttackTime", "GlobalPlayerNoAttackTimePlay")
DISPLAY_TIME_FIELDS = ("InsideDisplayTimeForMeleeBattle",)
DISPLAY_BOOL_FIELDS = ("DoDisplayCheckForMeleeBattle", "AlwaysDoDisplayCheck")
ATTACK_ACTION_FIELDS = ("/AttackStartTimeoutFrames", "/AttackIntervalTimeFromDamage",
                        "/AttackIntervalTimeFromGuard", "/AttackIntervalTimeGuarded",
                        "/AttackIntervalTimeMinGrdedCombo", "/BackWalkTimeMax",
                        "/BackWalkTimeMin", "/TimeoutTime")


def load_dicts():
    DICT_CACHE.mkdir(parents=True, exist_ok=True)
    raw = (ROMFS / "Pack/ZsDic.pack.zs").read_bytes()
    data = zstandard.ZstdDecompressor().decompress(raw, max_output_size=20_000_000)
    files = sarc.read_sarc(data)
    out = {}
    for name, (s, e) in files.items():
        p = DICT_CACHE / Path(name).name
        if not p.exists():
            p.write_bytes(data[s:e])
        out[Path(name).name] = p.read_bytes()
    return out


def pack_of(data, dictionary):
    return zstandard.ZstdDecompressor(dict_data=zstandard.ZstdCompressionDict(dictionary)).decompress(
        data, max_output_size=200_000_000)


def pack_to(data, dictionary):
    return zstandard.ZstdCompressor(dict_data=zstandard.ZstdCompressionDict(dictionary)).compress(data)


def byml_bytes_tree(data):
    b = byml.Byml(data)
    return b, b.tree()


def set_float(buf, off, value):
    struct.pack_into("<f", buf, off, float(value))


def scale_fields(buf, data, suffix, factor, min_positive=0.01):
    """Scale float fields whose path ends with one of suffix (in-place, same file size)."""
    changed = 0
    b = byml.Byml(data)
    for path, typ, value, off in b.flat():
        if typ != byml.T_FLOAT or not isinstance(value, float):
            continue
        if not any(path.endswith(s.lstrip("/")) for s in suffix):
            continue
        if value > min_positive:
            set_float(buf, off, value * factor)
            changed += 1
    return changed


def scan_pack(path, dictionaries):
    """Return [{o: absolute offset in the decompressed pack, k: kind, v: original value}]: floats of kind m / a / p
    (speeds) and d / k / g / i (aggression), and bools of kind o (v = 1)."""
    raw = path.read_bytes()
    data = pack_of(raw, dictionaries["pack.zsdic"])
    files = sarc.read_sarc(data)
    found = {}
    for name, (start, end) in files.items():
        if not (name.endswith(".bgyml") or name.endswith(".byml")):
            continue
        block = data[start:end]
        try:
            b = byml.Byml(block)
        except Exception:
            continue
        movement = "BSA/MovementParam/" in name
        action_seq = "Game/BSA/ActionSeqContainer/" in name
        attack_table = "GameBalance/AttackInterval/" in name
        game_param = (name.startswith("GameParameter/") or name.startswith("Game/")) and not action_seq
        balance = "Component/GameBalanceClientParam/" in name
        if not (movement or action_seq or attack_table or game_param or balance):
            continue
        for path_name, typ, value, off in b.flat():
            last = path_name.rsplit("/", 1)[-1]
            if balance and typ == byml.T_BOOL and bool(value) and last in DISPLAY_BOOL_FIELDS:
                found[start + off] = ("o", 1)
                continue
            if typ != byml.T_FLOAT or not isinstance(value, float):
                continue
            if value < 0.0:
                # WalkBackwardsSpeed is negative by design; other negative floats are sentinels (e.g. -1 =
                # "no interval") and must not be scaled.
                if not path_name.endswith("WalkBackwardsSpeed"):
                    continue
            elif value <= 0.01:
                continue
            kind = None
            if balance:
                if last in TURN_FIELDS:
                    kind = "g"
                elif last in DISPLAY_TIME_FIELDS:
                    kind = "i"
                if kind is not None:
                    found[start + off] = (kind, value)
                continue
            if movement:
                if any(path_name.endswith(s.lstrip("/")) for s in MOVE_FIELDS):
                    kind = "m"
            elif action_seq:
                if any(path_name.endswith(s.lstrip("/")) for s in ACTION_MOVE_FIELDS):
                    kind = "m"
                elif any(path_name.endswith(s.lstrip("/")) for s in ATTACK_ACTION_FIELDS):
                    kind = "a"
            elif attack_table:
                if any(path_name.endswith(s.lstrip("/")) for s in ATTACK_TABLE_FIELDS):
                    kind = "a"
            elif game_param:
                if last in ALERT_FIELDS:
                    kind = "d"
                elif last in PURSUIT_FIELDS:
                    kind = "k"
                elif any(path_name.endswith(s.lstrip("/")) for s in GAME_MOVE_FIELDS):
                    kind = "m"
                elif any(s in path_name for s in PROJECTILE_FIELDS):
                    kind = "p"
                elif last in ATTACK_INTERVAL_NAMES:
                    kind = "a"
            if kind is not None:
                found[start + off] = (kind, value)
    return [{"o": off, "k": kind, "v": value} for off, (kind, value) in sorted(found.items())]


def edit_pack(path, dictionaries, factor, kind):
    """Apply one component (legacy helper; the app patches from the ready-made base packs)."""
    raw = path.read_bytes()
    data = bytearray(pack_of(raw, dictionaries["pack.zsdic"]))
    files = sarc.read_sarc(bytes(data))
    changed = False
    scale = factor if kind == "m" else 1.0 / factor if kind == "a" else factor
    for name, (start, end) in files.items():
        if not (name.endswith(".bgyml") or name.endswith(".byml")):
            continue
        block = data[start:end]
        try:
            before = bytes(block)
            if kind == "m":
                if "BSA/MovementParam/" in name:
                    scale_fields(block, before, MOVE_FIELDS, scale)
                if "Game/BSA/ActionSeqContainer/" in name:
                    scale_fields(block, before, ACTION_MOVE_FIELDS, scale)
                if name.startswith("GameParameter/") or name.startswith("Game/"):
                    scale_fields(block, before, GAME_MOVE_FIELDS, scale)
            if kind == "a":
                if "Game/BSA/ActionSeqContainer/" in name:
                    scale_fields(block, before, ATTACK_ACTION_FIELDS, scale)
                if "GameBalance/AttackInterval/" in name:
                    scale_fields(block, before, ATTACK_TABLE_FIELDS, scale)
                if name.startswith("GameParameter/") or name.startswith("Game/"):
                    scale_fields(block, before, ATTACK_INTERVAL_NAMES, scale)
            if kind == "p":
                if name.startswith("GameParameter/") or name.startswith("Game/"):
                    scale_fields(block, before, PROJECTILE_FIELDS, scale)
            if bytes(block) != before:
                data[start:end] = block
                changed = True
        except Exception:
            continue
    return pack_to(bytes(data), dictionaries["pack.zsdic"]) if changed else None


def edit_asb(path, zs_dict, factor):
    raw = path.read_bytes()
    try:
        data = bytearray(zstandard.ZstdDecompressor(
            dict_data=zstandard.ZstdCompressionDict(zs_dict)).decompress(raw, max_output_size=20_000_000))
    except Exception:
        return None
    if data[:4] != b"ASB ":
        return None
    # Header: 0x14 = element count, 0x18 = element array (stride 0x24).
    count, arr = struct.unpack_from("<II", data, 0x14)
    if not (0 < count < 10000 and 0 < arr < len(data)):
        return None
    changed = False
    for i in range(count):
        off = arr + i * 0x24
        if off + 0x0C > len(data):
            break
        if struct.unpack_from("<H", data, off)[0] != 12:   # FrameController
            continue
        body = struct.unpack_from("<I", data, off + 0x08)[0]
        if not (0 < body <= len(data) - 8):
            continue
        rate = struct.unpack_from("<f", data, body + 0x04)[0]   # playback rate (1.0 = normal)
        if 0.01 < rate < 8.0:
            struct.pack_into("<f", data, body + 0x04, min(rate * factor, 8.0))
            changed = True
    if not changed:
        return None
    return zstandard.ZstdCompressor(dict_data=zstandard.ZstdCompressionDict(zs_dict)).compress(bytes(data))


def is_upright(actor, min_up=0.85):
    """The placement stands on the floor: its local up vector, rotated by the actor's Euler angles, still points up.
    Wall and ceiling enemies (Skullwalltula, Keese perched on walls or hanging from ceilings, the Tailpasaran in
    Jabu-Jabu) are turned to face their surface; their clone, moved 2.5 m sideways, would float in the air or sit in
    the wall. A rotation of 180 degrees on X and Z together is only a turn around Y (how the editor stores some
    yaws), so it counts as upright."""
    import math
    r = actor.get("Rotate")
    if r is None or len(r) != 3:
        return True
    rx, ry, rz = float(r[0].v), float(r[1].v), float(r[2].v)
    # y of Rx(rx) * Ry(ry) * Rz(rz) * (0, 1, 0)
    up_y = math.cos(rx) * math.cos(rz) - math.sin(rx) * math.sin(ry) * math.sin(rz)
    return up_y >= min_up


def new_hash(rng, used):
    while True:
        v = rng.getrandbits(64)
        if v not in used:
            used.add(v)
            return v


def duplicate_banc(path, rng, size_changes):
    raw = path.read_bytes()
    try:
        data = zstandard.ZstdDecompressor(
            dict_data=zstandard.ZstdCompressionDict(DICTS["bcett.byml.zsdic"])).decompress(
            raw, max_output_size=100_000_000)
    except Exception:
        return None
    try:
        b, tree = byml_bytes_tree(data)
    except Exception:
        return None
    actors = tree.get("Actors")
    if actors is None:
        return None
    used = {a.get("Hash").v for a in actors if isinstance(a, dict) and a.get("Hash") is not None}
    # Spawn zones (StalchildAutoPlacementArea...): the group each one creates is in the zone itself
    # (CreateStalchildCountMin/Max and the AtAdultNight variants; adult Link's Stalchildren only come from these,
    # the scene parameter keeps them off). Double every count above 0.
    areas = 0
    for a in actors:
        g = a.get("Gyaml")
        dyn = a.get("Dynamic")
        if g is None or "AutoPlacementArea" not in g.v or not isinstance(dyn, byml.BDict):
            continue
        for k, v in list(dyn.items()):
            if k.startswith("Create") and "Count" in k and ("Min" in k or "Max" in k) \
                    and isinstance(v, byml.Val) and isinstance(v.v, int) and v.v > 0:
                dyn[k] = byml.Val(v.t, v.v * 2)
                areas += 1
    add = []
    for a in actors:
        g = a.get("Gyaml")
        if g is None:
            continue
        name = g.v
        if not name.startswith("Enemy_") or any(x in name for x in BLOCK) or any(x in name for x in NO_DOUBLE):
            continue
        if a.get("Links") is not None:
            continue
        if not is_upright(a):   # on a wall or a ceiling: a clone beside it would hang in the air
            continue
        c = copy.deepcopy(a)
        h = new_hash(rng, used)
        c["Hash"] = byml.Val(byml.T_U64, h)
        ph = c.get("Phive")
        if ph is not None and ph.get("Placement") is not None:
            ph["Placement"]["ID"] = byml.Val(byml.T_U64, h)
        # same spatial cell as the original (the clone is only a couple of metres away), otherwise the game can
        # cull it with the wrong cell and it never appears
        if a.get("SRTHash") is not None:
            c["SRTHash"] = byml.Val(byml.T_UINT, a["SRTHash"].v)
        tr = c.get("Translate")
        if tr is not None and len(tr) == 3:
            tr[0] = byml.Val(byml.T_FLOAT, tr[0].v + 2.5)
            tr[2] = byml.Val(byml.T_FLOAT, tr[2].v + 2.5)
        add.append(c)
    if not add and not areas:
        return None
    actors.extend(add)
    out = byml.write(tree, version=b.version, e=b.e)
    size_changes.append((str(path.relative_to(ROMFS)).replace("\\", "/").removesuffix(".zs"), len(out)))
    return zstandard.ZstdCompressor(
        dict_data=zstandard.ZstdCompressionDict(DICTS["bcett.byml.zsdic"])).compress(out)


def edit_autoplace(path, size_changes):
    raw = path.read_bytes()
    try:
        b, tree = byml_bytes_tree(raw)
    except Exception:
        return None
    changed = False

    def walk(node):
        nonlocal changed
        if isinstance(node, byml.BDict):
            for k, v in list(node.items()):
                if k in ("CreateActorCountMinAtOnce", "CreateActorCountMaxAtOnce",
                         "MinExistActorCountToCreate", "MaxCreateStalchildByFixedSpawn") \
                        and isinstance(v, byml.Val) and isinstance(v.v, int):
                    # 0 means the spawn is off there (Enemy_Stalchild_Adult: no Stalchildren for adult Link): keep
                    # it off instead of turning it on with a minimum of 1
                    if v.v > 0:
                        node[k] = byml.Val(v.t, v.v * 2)
                        changed = True
                elif k in ("IntervalToCreateFrame", "IntervalRandomToCreateFrame") and isinstance(v, byml.Val) \
                        and isinstance(v.v, (int, float)):
                    node[k] = byml.Val(v.t, v.v / 2.0)
                    changed = True
                walk(v)
        elif isinstance(node, byml.BArr):
            for v in node:
                walk(v)

    walk(tree)
    if not changed:
        return None
    out = byml.write(tree, version=b.version, e=b.e)
    size_changes.append((str(path.relative_to(ROMFS)).replace("\\", "/"), len(out)))
    return out


def update_restbl(size_changes, base_path=None):
    if not size_changes:
        return None
    p = Path(base_path) if base_path else (ROMFS / RESTBL_REL)
    data = bytearray(zstandard.ZstdDecompressor().decompress(p.read_bytes(), max_output_size=20_000_000))
    n = struct.unpack_from("<I", data, 0x0E)[0]
    base = 0x16
    by_crc = {}
    for i in range(n):
        crc, size = struct.unpack_from("<II", data, base + i * 8)
        by_crc[crc] = (base + i * 8, size)
    changed = False
    import binascii
    for path, size in size_changes:
        crc = binascii.crc32(path.encode()) & 0xFFFFFFFF
        hit = by_crc.get(crc)
        if hit is None:
            continue
        off, old = hit
        new = max(old, (size + 0x2000 + 7) & ~7)
        if new != old:
            struct.pack_into("<I", data, off + 4, new)
            changed = True
    if not changed:
        return None
    return zstandard.ZstdCompressor().compress(bytes(data))


def build_packs():
    import shutil
    MANIFEST.unlink(missing_ok=True)
    BOSS_MANIFEST.unlink(missing_ok=True)
    # Movement and attack cadence: original packs + a manifest of the float offsets.
    packs = []
    for p in sorted((ROMFS / "Pack/Actor").glob("Enemy_*.pack.zs")):
        if any(x in p.stem for x in BLOCK):
            continue
        fields = scan_pack(p, DICTS)
        if not fields:
            continue
        rel = str(p.relative_to(ROMFS)).replace("\\", "/")
        dst = OUT / BASE_DIR / p.relative_to(ROMFS)
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(p, dst)
        packs.append({"path": rel, "fields": fields})
    MANIFEST.write_text(json.dumps({"version": 3, "factors": {str(k): v for k, v in FACTORS.items()},
                                    "packs": packs}, separators=(",", ":")), encoding="utf-8")
    boss_packs = []
    for p in sorted((ROMFS / "Pack/Actor").glob("Enemy_*.pack.zs")):
        if not is_boss(p.name.split(".")[0]):
            continue
        fields = scan_pack(p, DICTS)
        if not fields:
            continue
        rel = str(p.relative_to(ROMFS)).replace("\\", "/")
        dst = OUT / BOSS_BASE_DIR / p.relative_to(ROMFS)
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(p, dst)
        boss_packs.append({"path": rel, "fields": fields})
    BOSS_MANIFEST.write_text(json.dumps({"version": 3, "factors": {str(k): v for k, v in FACTORS.items()},
                                         "packs": boss_packs}, separators=(",", ":")), encoding="utf-8")
    PACK_DICT.write_bytes(DICTS["pack.zsdic"])
    print('  base packs:', len(packs), 'boss packs:', len(boss_packs), 'kinds:', sorted({f['k'] for q in packs for f in q['fields']}))
    return packs, boss_packs


def write_double_restbl(size_changes):
    """The resource size table for the bigger Banc files. The button mapping's own table (bigger UI budget) is the
    base when it exists: the enemy_double table overwrites it, so it must keep both changes."""
    button_table = ROOT / "app" / "payload" / "button_romfs" / "common" / RESTBL_REL
    table = update_restbl(size_changes, button_table if button_table.exists() else None)
    if table:
        dst = OUT / "enemy_double" / RESTBL_REL
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(table)


def main():
    global DICTS
    if not (ROMFS / "Banc").exists():
        print("enemy options: game dump not found (set ROMFS); keeping existing payload")
        return
    import shutil
    DICTS = load_dicts()
    # --double-only: rebuild only the duplicated enemies (the speed and animation folders are left as they are)
    double_only = "--double-only" in sys.argv
    # --packs-only: rebuild only the base packs and their manifests (speeds and aggression)
    packs_only = "--packs-only" in sys.argv
    folders = ("enemy_double",) if double_only else (BASE_DIR, BOSS_BASE_DIR) if packs_only else         ("enemy_double", "enemy_speed", BASE_DIR, *ANIM_DIR.values(), BOSS_BASE_DIR, *BOSS_ANIM_DIR.values())
    for name in folders:
        d = OUT / name
        if d.exists():
            shutil.rmtree(d)
        if name != "enemy_speed":          # the old combined x2 folder is retired
            d.mkdir(parents=True, exist_ok=True)
    if packs_only:
        build_packs()
        return
    if not double_only:
        MANIFEST.unlink(missing_ok=True)
        BOSS_MANIFEST.unlink(missing_ok=True)
    rng = random.Random(0x5A17B005)
    size_changes = []

    # Field enemies that are auto-placed: double counts and halve intervals.
    for p in sorted((ROMFS / "Scene/Component/AutoPlacementMgrParam").glob("*.bgyml")):
        out = edit_autoplace(p, size_changes)
        if out:
            dst = OUT / "enemy_double" / p.relative_to(ROMFS)
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(out)

    # Hand-placed generic enemies: one clone each.
    for p in sorted((ROMFS / "Banc").rglob("*.bcett.byml.zs")):
        rel = str(p.relative_to(ROMFS)).replace("\\", "/")
        if any(rel.startswith(x) for x in BLOCK_BANCS):
            continue
        out = duplicate_banc(p, rng, size_changes)
        if out:
            dst = OUT / "enemy_double" / p.relative_to(ROMFS)
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(out)

    if double_only:
        write_double_restbl(size_changes)
        print("enemy_double written to", OUT / "enemy_double")
        return

    # Enemy animation: ASB frame-controller playback rates, one ready-made folder per multiplier
    # (bosses into their own folders).
    for level, factor in FACTORS.items():
        for p in sorted((ROMFS / "AS").glob("Enemy_*.root.asb.zs")):
            stem = p.name.split(".")[0]
            boss = is_boss(stem)
            if not boss and any(x in p.stem for x in BLOCK):
                continue
            out = edit_asb(p, DICTS["zs.zsdic"], factor)
            if out:
                dst = OUT / (BOSS_ANIM_DIR if boss else ANIM_DIR)[level] / p.relative_to(ROMFS)
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(out)

    packs, boss_packs = build_packs()

    # Start from the buttonMapping size table when it exists (it carries the bigger UI budget): the installed
    write_double_restbl(size_changes)
    print("enemy options written to", OUT)
    print("  base packs:", len(packs), "(%d floats)" % sum(len(p["fields"]) for p in packs))
    print("  boss packs:", len(boss_packs), "(%d floats)" % sum(len(p["fields"]) for p in boss_packs))
    for level, folder in ANIM_DIR.items():
        n = sum(1 for _ in (OUT / folder / "AS").glob("*.asb.zs")) if (OUT / folder / "AS").exists() else 0
        print("  %s: %d animation files" % (folder, n))


if __name__ == "__main__":
    main()
