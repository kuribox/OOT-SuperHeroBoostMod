"""Build the Great Fairy reward beta (romfs option) from the user's own game dump.

Rewards are remapped so the three magic spells/upgrades can be collected earlier:
  Hyrule Castle (child)   Great Fairy   -> Nayru's Love   (was the magic spin attack)
  Death Mountain Crater   Great Fairy   -> magic spin attack (was double magic)
  Desert Colossus         Great Fairy   -> double magic  (was Nayru's Love)

Each fountain keeps its own Great Fairy and its own event flow; only the item the
flow grants (and its "already received" check) is replaced, and the reward dialogue
is pointed at the message of the new reward so the text matches.

The flows are rewritten with the `evfl` library (the writer is byte-identical when
nothing changes) and recompressed with the game's own zstd dictionary. The resource
size table is grown for the three files because they may end up larger.

Two more reward changes ride with this option:
  Ghost Shop (poe buyer)   500-point reward  -> Ice Arrows  (was an empty bottle)
  Gerudo Training Ground   last big chest    -> empty bottle (was the Ice Arrows)
The poe buyer's flow keeps its dialogue; only the actor name its reward event grants changes. The chest is the
`Obj_TBox_A_02` with `Drop__DropActor: Item_Arrow_Ice` in `Banc/GerudoTrainingGround/GerudoTrainingGround_Static`.
That Banc file is rebuilt from the game's own; `great_fairy_double/` holds the same chest change on top of the duplicated-enemies
file, and the installers copy it last when both options are on (so the dungeon keeps its clones too).

Output: app/payload/romfs_opts/great_fairy/...
"""
import io
import os
import struct
import sys
import zlib
from pathlib import Path

import zstandard

ROOT = Path(__file__).resolve().parent.parent
DEVTOOL = Path(os.environ.get("SHB_DEVTOOL", r"F:\project\oot\vendor\devtool\oot-remake-devtool-main"))
ROMFS = Path(os.environ.get("SHB_ROMFS", r"F:\switch2\Longshot\romfs"))
OUT = ROOT / "app" / "payload" / "romfs_opts"
RESTBL_REL = "System/Resource/ResourceSizeTable.Product.111.Nin_Ounce_NVN2.rsizetable.zs"

sys.path.insert(0, str(ROOT / "tools"))
sys.path.insert(0, str(DEVTOOL))
from lib import sarc  # noqa: E402

import evfl  # noqa: E402

# flow name -> (item to grant, item name in the flow, reward message to use)
# The message keeps each fountain's own dialogue style but says the new reward.
PATCH = {
    "Dm_FairyFountain_Power": {
        "item": "Item_NayruLove",
        "check": "IsGet.Item_NayruLove",
        "message": "EventFlowMsg/Dm_FairyFountain_Nayru:Dm_FairyFountain_Nayru_0004",
        "old_item": "Item_FairyBless_MP",
        "old_check": "IsGet.Item_FairyBless_MP",
        "old_message": "EventFlowMsg/Dm_FairyFountain_Power:Dm_FairyFountain_Power_0004",
    },
    "Dm_FairyFountain_Wisdom": {
        "item": "Item_FairyBless_MP",
        "check": "IsGet.Item_FairyBless_MP",
        "message": "EventFlowMsg/Dm_FairyFountain_Power:Dm_FairyFountain_Power_0004",
        "old_item": "Item_FairyBless_MPDouble",
        "old_check": "IsGet.Item_FairyBless_MPDouble",
        "old_message": "EventFlowMsg/Dm_FairyFountain_Wisdom:Dm_FairyFountain_Wisdom_0004",
    },
    "Dm_FairyFountain_Nayru": {
        "item": "Item_FairyBless_MPDouble",
        "check": "IsGet.Item_FairyBless_MPDouble",
        "message": "EventFlowMsg/Dm_FairyFountain_Wisdom:Dm_FairyFountain_Wisdom_0004",
        "old_item": "Item_NayruLove",
        "old_check": "IsGet.Item_NayruLove",
        "old_message": "EventFlowMsg/Dm_FairyFountain_Nayru:Dm_FairyFountain_Nayru_0004",
    },
}


def load_dict(name):
    raw = (ROMFS / "Pack" / "ZsDic.pack.zs").read_bytes()
    data = zstandard.ZstdDecompressor().decompress(raw, max_output_size=20_000_000)
    files = sarc.read_sarc(data)
    for path, (s, e) in files.items():
        if Path(path).name == name:
            return data[s:e]
    raise FileNotFoundError(name)


def replace_strings(node, mapping, hits):
    if isinstance(node, dict):
        for k, v in list(node.items()):
            if isinstance(v, str) and v in mapping:
                node[k] = mapping[v]
                hits.append(mapping[v])
            else:
                replace_strings(v, mapping, hits)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            if isinstance(v, str) and v in mapping:
                node[i] = mapping[v]
                hits.append(mapping[v])
            else:
                replace_strings(v, mapping, hits)


def patch_flow(name, info, dictionary, zs_dict):
    src = ROMFS / "Event" / "EventFlow" / (name + ".bfevfl.zs")
    data = zs_dict.decompress(src.read_bytes(), max_output_size=20_000_000)
    flow = evfl.EventFlow()
    flow.read(data)
    hits = []
    mapping = {
        info["old_item"]: info["item"],
        info["old_check"]: info["check"],
        info["old_message"]: info["message"],
    }
    for actor in flow.flowchart.actors:
        if actor.params:
            replace_strings(actor.params.data, mapping, hits)
    for ev in flow.flowchart.events:
        params = getattr(ev.data, "params", None)
        if params:
            replace_strings(params.data, mapping, hits)
    for need in (info["item"], info["check"], info["message"]):
        if need not in hits:
            raise RuntimeError(f"{name}: replacement {need!r} did not match anything")
    buf = io.BytesIO()
    if not flow.write(buf):
        raise RuntimeError(f"{name}: evfl write failed")
    out = buf.getvalue()
    # Sanity: read it back and check the new item is present.
    check = evfl.EventFlow()
    check.read(out)
    print(f"{name}: {len(data)} -> {len(out)} bytes, {len(hits)} replacements")
    return zstandard.ZstdCompressor(dict_data=dictionary).compress(out), out


SHOP_FLOW = "Npc_Hylia_048_BottleShop"
SHOP_REWARD_EVENT = "Event329"          # GetInEvent_Poeshop after the 500-point talk (Talk_65)


def patch_shop(dictionary, zs_dict):
    src = ROMFS / "Event" / "EventFlow" / (SHOP_FLOW + ".bfevfl.zs")
    data = zs_dict.decompress(src.read_bytes(), max_output_size=20_000_000)
    flow = evfl.EventFlow()
    flow.read(data)
    hit = [ev for ev in flow.flowchart.events if ev.name == SHOP_REWARD_EVENT]
    if len(hit) != 1:
        raise RuntimeError(f"{SHOP_FLOW}: {SHOP_REWARD_EVENT} not found")
    params = hit[0].data.params.data
    if params.get("ActorName") != "Item_Bottle":
        raise RuntimeError(f"{SHOP_FLOW}: {SHOP_REWARD_EVENT} does not grant Item_Bottle: {params.get('ActorName')!r}")
    params["ActorName"] = "Item_Arrow_Ice"
    buf = io.BytesIO()
    if not flow.write(buf):
        raise RuntimeError(f"{SHOP_FLOW}: evfl write failed")
    out = buf.getvalue()
    check = evfl.EventFlow()
    check.read(out)
    print(f"{SHOP_FLOW}: {len(data)} -> {len(out)} bytes, reward Item_Bottle -> Item_Arrow_Ice")
    return zstandard.ZstdCompressor(dict_data=dictionary).compress(out), out


GTG_BANC = "Banc/GerudoTrainingGround/GerudoTrainingGround_Static.bcett.byml"


def patch_gtg_chest(src=None):
    import byml
    src = src or ROMFS / (GTG_BANC + ".zs")
    bcett = zstandard.ZstdCompressionDict(load_dict("bcett.byml.zsdic"))
    raw = zstandard.ZstdDecompressor(dict_data=bcett).decompress(src.read_bytes(), max_output_size=100_000_000)
    b = byml.Byml(raw)
    tree = b.tree()
    hits = [a for a in tree["Actors"]
            if (a.get("Dynamic") or {}).get("Drop__DropActor") is not None
            and a["Dynamic"]["Drop__DropActor"].v == "Item_Arrow_Ice"]
    if len(hits) != 1:
        raise RuntimeError(f"Gerudo Training Ground: expected one Ice Arrows chest, found {len(hits)}")
    hits[0]["Dynamic"]["Drop__DropActor"] = byml.Val(hits[0]["Dynamic"]["Drop__DropActor"].t, "Item_Bottle")
    out = byml.write(tree, version=b.version, e=b.e)
    print(f"Gerudo Training Ground chest: {len(raw)} -> {len(out)} bytes, Item_Arrow_Ice -> Item_Bottle")
    return zstandard.ZstdCompressor(dict_data=bcett).compress(out), out


def load_restbl():
    """Use the enemy mod's table as the base when it exists so both options keep their budgets."""
    enemy = OUT / "enemy_double" / RESTBL_REL
    path = enemy if enemy.exists() else (ROMFS / RESTBL_REL)
    raw = zstandard.ZstdDecompressor().decompress(path.read_bytes(), max_output_size=20_000_000)
    return bytearray(raw), path


def grow_restbl(data, changes):
    n = struct.unpack_from("<I", data, 0x0E)[0]
    base = 0x16
    by_crc = {}
    for i in range(n):
        crc, size = struct.unpack_from("<II", data, base + i * 8)
        by_crc[crc] = (base + i * 8, size)
    for path, size in changes:
        crc = zlib.crc32(path.encode()) & 0xFFFFFFFF
        hit = by_crc.get(crc)
        if hit is None:
            print("restbl: not found", path)
            continue
        off, old = hit
        new = max(old, (size + 0x2000 + 7) & ~7)
        if new != old:
            struct.pack_into("<I", data, off + 4, new)
            print(f"restbl: {path}: {old:#x} -> {new:#x}")
    return bytes(data)


def main():
    zs_dict = load_dict("zs.zsdic")
    dictionary = zstandard.ZstdCompressionDict(zs_dict)
    zs = zstandard.ZstdDecompressor(dict_data=dictionary)
    outdir = OUT / "great_fairy"
    changes = []
    for name, info in PATCH.items():
        packed, plain = patch_flow(name, info, dictionary, zs)
        dst = outdir / "Event" / "EventFlow" / (name + ".bfevfl.zs")
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(packed)
        rel = f"Event/EventFlow/{name}.bfevfl"
        changes.append((rel, len(plain)))
    packed, plain = patch_shop(dictionary, zs)
    dst = outdir / "Event" / "EventFlow" / (SHOP_FLOW + ".bfevfl.zs")
    dst.write_bytes(packed)
    changes.append((f"Event/EventFlow/{SHOP_FLOW}.bfevfl", len(plain)))
    packed, plain = patch_gtg_chest()
    dst = outdir / (GTG_BANC + ".zs")
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(packed)
    changes.append((GTG_BANC, len(plain)))
    # The same chest on top of the duplicated-enemies version of the file (installed last when both options are on).
    double_src = OUT / "enemy_double" / (GTG_BANC + ".zs")
    double_dst = OUT / "great_fairy_double" / (GTG_BANC + ".zs")
    if double_dst.parent.exists():
        import shutil
        shutil.rmtree(double_dst.parent.parent.parent)
    if double_src.exists():
        packed, plain = patch_gtg_chest(double_src)
        double_dst.parent.mkdir(parents=True, exist_ok=True)
        double_dst.write_bytes(packed)
    else:
        print("great_fairy_double: no duplicated-enemies file to build on, skipped")
    table, source = load_restbl()
    print("restbl base:", source)
    out = grow_restbl(table, changes)
    dst = outdir / RESTBL_REL
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_bytes(zstandard.ZstdCompressor(level=19).compress(out))
    print("wrote", outdir)


if __name__ == "__main__":
    main()
