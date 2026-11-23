"""Developer step (NOT done by the app): compile the prebuilt module and stage it as the app payload.

  standalone  module linked right after subsdk0 (base 0x4F80000): stamina, songbook and buttonMapping (our own
              item buttons.)

Output: app/payload/<variant>/{subsdk9,ours.ips} + app/payload/manifest.json + app/payload/button_romfs
"""
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "app" / "payload"
BUILD_ID = "E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348"
VARIANTS = {"standalone": "0x4F80000", "relink": "0x4F80000", "rekarina": "0x5200000"}
# buttonMapping needs the equip-call hooks too (--slots)
VARIANT_ARGS = {
    "standalone": ["--features", "controls,manual_shield,stamina,songbook,button_mapping,quick_select", "--slots"],
    # ReLink: no lazy-binding check in the input wrappers (its translator turns that check into a
    # tail call and drops the dispatch, so no control/stamina feature would ever run)
    "relink": ["--features", "controls,manual_shield,stamina,songbook,button_mapping,quick_select", "--slots",
               "--no-lazy"],
    # Rekarina: a module linked right before sdk (base main+0x5200000) with the synthetic function-seed table its
    # lifter reads (recomp/module.py eh_frame_functions). No --base: --link-base writes the matching linker script.
    "rekarina": ["--features", "controls,manual_shield,stamina,songbook,button_mapping,quick_select", "--slots",
                 "--link-base", "0x5200000", "--rekarina"],
}


def stage_rupee_ips():
    """rupee.ips: skip the 'hide the numeric display when its value is stable' call (main+0x33C3B80 -> branch over it)."""
    import struct
    rec = struct.pack(">I", 0x33C3B80 + 0x100) + struct.pack(">H", 4) + struct.pack("<I", 0x14000000 | 3)
    (OUT / "rupee.ips").write_bytes(b"IPS32" + rec + b"EEOF")


def stage_buttons():
    """Romfs of our buttonMapping: the button textures per platform (tools/make_buttons.py) and a resource size
    table with a bigger UI budget (the 12 extra HUD parts do not fit the stock one; the game hangs at boot)."""
    subprocess.run([sys.executable, str(ROOT / "tools" / "make_buttons.py")], check=True, cwd=ROOT)
    for plat in ("switch", "xbox", "playstation"):
        shutil.copytree(ROOT / "build" / "buttons" / plat / "romfs", OUT / "button_romfs" / plat)
    subprocess.run([sys.executable, str(ROOT / "tools" / "make_gamecube_buttons.py")], check=True, cwd=ROOT)
    raw = ROOT / "build" / "rsizetable.bin"   # decompressed vanilla table (the dump may not be available)
    r = subprocess.run([sys.executable, str(ROOT / "tools" / "make_restbl.py"), str(OUT / "button_romfs" / "common")]
                       + (["--vanilla-raw", str(raw)] if raw.exists() else []), cwd=ROOT)
    if r.returncode:
        print("WARNING: no resource size table: our HUD will not fit the stock UI budget")
    # The game's own PlayStation prompts are glyphs of the External font: the app installs this patched font next to the
    # layout's atlas (app/layouts.py apply). This step wipes app/payload, so the font is made here every time; without it
    # the atlas alone changes the controller picture in the menu and the prompts everywhere else stay Switch.
    r = subprocess.run([sys.executable, str(ROOT / "tools" / "make_button_font.py")], cwd=ROOT)
    if r.returncode:
        print("WARNING: no PlayStation button font: the game's own prompts stay Switch (needs the dump and the layout download)")


def main():
    keep = None
    if (OUT / "romfs_opts").exists():
        keep = ROOT / "build" / "payload_keep" / "romfs_opts"
        shutil.rmtree(keep.parent, ignore_errors=True)
        shutil.copytree(OUT / "romfs_opts", keep)
    if OUT.exists():
        shutil.rmtree(OUT)
    r = subprocess.run([sys.executable, str(ROOT / "tools" / "make_collectibles.py")], cwd=ROOT)
    if r.returncode:
        print("WARNING: collectible table not built (game dump missing?): the HUD counter stays empty")
    man = {"build_id": BUILD_ID, "variants": {}}
    for name, base in VARIANTS.items():
        extra = [] if "--link-base" in VARIANT_ARGS[name] else ["--base", base]
        subprocess.run([sys.executable, str(ROOT / "tools" / "build_core.py"), *VARIANT_ARGS[name], *extra],
                       check=True, cwd=ROOT)
        d = OUT / name
        d.mkdir(parents=True)
        src = ROOT / "build" / "native" / "exefs"
        shutil.copy2(src / "subsdk9", d / "subsdk9")
        shutil.copy2(src / f"{BUILD_ID}.ips", d / "ours.ips")
        man["variants"][name] = {"base": base, "sha256": hashlib.sha256((d / "subsdk9").read_bytes()).hexdigest()}
    if keep is not None and keep.exists():
        shutil.copytree(keep, OUT / "romfs_opts")
    stage_buttons()
    r = subprocess.run([sys.executable, str(ROOT / "tools" / "make_romfs_opts.py")], cwd=ROOT)
    if r.returncode:
        print("WARNING: romfs options not built (game dump missing?): the payload keeps any previous 'romfs_opts'")
    r = subprocess.run([sys.executable, str(ROOT / "tools" / "make_enemy_opts.py")], cwd=ROOT)
    if r.returncode:
        print("WARNING: beta enemy mods not built (game dump missing?): the payload keeps any previous files")
    r = subprocess.run([sys.executable, str(ROOT / "tools" / "make_farore_opts.py")], cwd=ROOT)
    if r.returncode:
        print("WARNING: Farore's Wind beta not built (game dump missing?): the payload keeps any previous files")
    r = subprocess.run([sys.executable, str(ROOT / "tools" / "make_great_fairy_opts.py")], cwd=ROOT)
    if r.returncode:
        print("WARNING: Great Fairy beta not built (game dump or evfl missing?): the payload keeps any previous files")
    (OUT / "manifest.json").write_text(json.dumps(man, indent=2))
    print("payload staged in", OUT)


if __name__ == "__main__":
    main()
