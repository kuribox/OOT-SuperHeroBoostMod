"""Build one GameBanana-ready zip with every profile of the app as a ready-to-install mod.

For each named profile in app/presets.py this stages the exact folder the app's Install would write
(module patched with the profile, IPS patch, button textures and the optional romfs files) and packs
all of them into:

    release/SuperHeroBoost-<version>-presets.zip

The zip has no executables: README.txt plus one numbered folder per profile, each holding a
`zz-Super-Hero-Boost` folder to copy into `user/load/0400486024DEC000`.

Run it after `tools/make_release.py` (the payload must be staged) or just run `tools/build_app.py`,
which calls it at the end.
"""
import hashlib
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "app"))
import i18n          # noqa: E402
import installer as I  # noqa: E402
import presets       # noqa: E402
from version import VERSION  # noqa: E402

DIST = ROOT / "release"
OUT = DIST / f"SuperHeroBoost-{VERSION}-presets.zip"


def settings_for(key: str) -> I.Settings:
    return I.Settings(**presets.settings_kwargs(key),
                      button_style="switch", bm_simple=False, bm_l_mode="assign")


def label(key: str) -> str:
    text = i18n.tr("en", "p_" + key.replace("-", "_"))
    text = text.replace("\u2605", "").replace(":", " -")
    return " ".join(text.split())


def description(key: str) -> str:
    return i18n.tr("en", "p_" + key.replace("-", "_") + "_d")


def main() -> None:
    if not (ROOT / "app" / "payload" / "standalone" / "subsdk9").exists():
        raise SystemExit("app/payload is missing: run tools/make_release.py first")
    DIST.mkdir(parents=True, exist_ok=True)

    rows = []
    notes = []
    with tempfile.TemporaryDirectory(prefix="shb-presets-") as td:
        td = Path(td)
        stage = td / "pack"
        stage.mkdir()
        for i, key in enumerate(presets.PROFILES, 1):
            folder = f"{i:02d} {label(key)}"
            emu = td / f"emu-{i}"
            (emu / "user" / "load" / I.TITLE_ID).mkdir(parents=True)
            res = I.install(emu, settings_for(key))
            for n in res.notes:
                notes.append(f"{folder}: {n[0]} {n[1]}".strip())
            dst = stage / folder / I.MOD_NAME
            shutil.copytree(res.target, dst)
            size = sum(f.stat().st_size for f in dst.rglob("*") if f.is_file())
            rows.append((folder, label(key), description(key), size))

        if notes:
            print("ERROR: the payload is incomplete; run tools/make_release.py with the game dump:")
            for n in notes:
                print("  " + n)
            raise SystemExit(1)

        lines = [
            f"Super Hero Boost {VERSION} - presets",
            "=" * (24 + len(VERSION)),
            "",
            "One zip with every game profile of the companion app. Each numbered folder contains",
            "a ready-to-install `zz-Super-Hero-Boost` folder; pick one profile.",
            "",
            "INSTALL",
            "-------",
            "1. Close the game and back up your save.",
            "2. Copy the `zz-Super-Hero-Boost` folder from the profile you want into:",
            "",
            "   user/load/0400486024DEC000",
            "",
            "   (merge it if that folder already exists). Install only one profile at a time:",
            "   all of them use the same folder name, so replace the previous one.",
            "3. Start the game.",
            "",
            "This pack contains no Windows executables. The optional world changes, no-heart-drop",
            "and beta enemy mods are already included in the profiles that use them; nothing else",
            "has to be generated.",
            "",
            "PROFILES",
            "--------",
        ]
        for folder, name, desc, size in rows:
            lines.append(f"- {folder}  ({size / 1024 / 1024:.1f} MB)")
            lines.append(f"  {desc}")
        lines += [
            "",
            "QUICK SELECT",
            "------------",
            "Every profile includes the boots/mask quick select: hold D-pad Down for boots or Up for masks.",
            "",
            "CUSTOM SETTINGS",
            "---------------",
            "For any other combination, or to install the module into Longshot, use the companion",
            "app (Tools section on GameBanana and GitHub Releases):",
            "",
            "https://github.com/kuribox/OOT-SuperHeroBoostMod/releases",
            "",
            "Longshot is supported and tested; Hookshot uses the same builder and procedure but is",
            "not tested yet.",
            "",
            "UNINSTALL",
            "---------",
            "Delete the `zz-Super-Hero-Boost` folder.",
            "",
            "CREDITS",
            "-------",
            "Kuribox - creator (module, HUD, stamina, item buttons, app)",
            "Anthropic (Claude) - AI assistant used for coding, reverse engineering and testing",
            "DeepSeek - AI assistant used for coding, reverse engineering and testing",
            "PixelSage / Hyrule Armory - the original Armory (https://gamebanana.com/mods/740938)",
            "DoctorTwitch / Dualsense/XBOX/Steamdeck UI - PlayStation / Xbox prompts the app downloads (https://gamebanana.com/mods/737074)",
            "Kabosune / oot-remake-modding - modding documentation and help",
            "platinumagpie / oot-remake-devtool - development tools used during research",
            "",
        ]
        (stage / "README.txt").write_text("\n".join(lines), encoding="utf-8")

        with zipfile.ZipFile(OUT, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
            for f in sorted(stage.rglob("*")):
                if f.is_file():
                    z.write(f, f.relative_to(stage).as_posix())

    data = OUT.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    (OUT.with_suffix(".zip.sha256")).write_text(digest + "  " + OUT.name + "\n", encoding="ascii")
    print(f"wrote {OUT} ({len(data) / 1024 / 1024:.1f} MB, {len(rows)} profiles)")
    print("sha256", digest)


if __name__ == "__main__":
    main()
