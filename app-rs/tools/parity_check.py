#!/usr/bin/env python3
"""Parity check: the Rust app against the Python app.

The Rust port is only trustworthy if it produces the same bytes as `app/installer.py`, so this script:

1. builds the settings of every game profile and a batch of random ones with the Python code,
2. patches the same payload module with the Python code and with `shb patch`,
3. compares the two images byte for byte,

and with `--mode install` it installs both into a throwaway folder and compares every installed file.

    python app-rs/tools/parity_check.py --mode all

The Python app is imported from `--app` (default: `app` next to this script's parent); `zstandard` is stubbed when it
is not installed, because the config patching does not use it.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib
import json
import random
import shutil
import subprocess
import sys
import tempfile
import types
from pathlib import Path

DEFAULT_APP = Path(__file__).resolve().parents[2] / "app"


def stub_zstandard() -> bool:
    """Make `installer` importable without zstandard. Returns True when the real module is there (the enemy-pack
    patching needs it; the config patching does not)."""
    try:
        # A previous call may have left the stub behind: ask it for the symbol the pack patching uses.
        return hasattr(importlib.import_module("zstandard"), "ZstdCompressionDict")
    except ImportError:
        mock = types.ModuleType("zstandard")
        mock.ZstdError = type("ZstdError", (Exception,), {})  # only referenced by the enemy-pack code
        sys.modules["zstandard"] = mock
        return False


def load_python_app(app: Path):
    sys.path.insert(0, str(app))
    installer = importlib.import_module("installer")
    presets = importlib.import_module("presets")
    return installer, presets


def run_shb(shb: Path, args: list[str]) -> None:
    proc = subprocess.run([str(shb), *args], capture_output=True, text=True)
    if proc.returncode != 0:
        raise SystemExit(f"shb {' '.join(args)} failed:\n{proc.stdout}\n{proc.stderr}")


def random_settings(installer, rng: random.Random):
    """A settings object with every option somewhere in its range (the edges included)."""
    settings = installer.Settings()
    settings.stamina = rng.random() < 0.7
    settings.preset = rng.choice([0, 1, 2, 3])
    for key in ("jump", "attack", "sprint", "block", "shoot", "roll"):
        setattr(settings, key, rng.random() < 0.6)
    settings.combat_only = rng.random() < 0.3
    settings.enemy_hp = rng.choice([50, 100, 150, 200, 250, 300, 400, 65535])
    settings.damage = rng.choice([50, 100, 150, 200, 300, 65535])
    settings.sprint_speed = rng.choice([100, 110, 125, 150, 200])
    settings.no_heart_drop = rng.random() < 0.5
    settings.no_magic_drop = rng.random() < 0.5
    settings.hud_rupees = rng.random() < 0.6
    settings.hud_clock = rng.random() < 0.6
    settings.clock_24h = rng.random() < 0.6
    settings.hud_collectibles = rng.random() < 0.6
    settings.hud_mode = rng.choice(list(installer.HUD_MODES))
    settings.quick_boots = rng.random() < 0.5
    settings.quick_mask = rng.random() < 0.5
    settings.songbook = rng.random() < 0.7
    settings.button_mapping = rng.random() < 0.8
    settings.button_style = rng.choice(list(installer.BUTTON_STYLES))
    settings.bm_layout = rng.choice(list(installer.BM_LAYOUTS))
    settings.bm_simple = rng.random() < 0.3
    settings.bm_l_mode = rng.choice(list(installer.L_MODES))
    settings.stamina_bar = rng.random() < 0.5
    settings.manual_shield = rng.random() < 0.6
    settings.target_r3 = rng.random() < 0.5
    settings.sprint_l3 = rng.random() < 0.5
    settings.sprint_l3_toggle = rng.random() < 0.5
    settings.auto_target = rng.random() < 0.5
    settings.switch_target = rng.random() < 0.5
    settings.hide_minimap = rng.random() < 0.5
    settings.lang = rng.choice(list(installer.LANGS))
    settings.bm_remap = rng.random() < 0.8
    table = installer.REMAP_PRESETS[rng.choice(installer.REMAP_PRESET_KEYS)]
    roles = dict(table)
    # A couple of stray, invalid entries: both sides have to sanitize them the same way.
    if rng.random() < 0.5:
        roles[rng.choice(installer.REMAP_BUTTONS)] = rng.choice(installer.ROLES)
    if rng.random() < 0.3:
        roles[rng.choice(installer.HOLD_KEYS)] = rng.choice(installer.ROLES)
    settings.bm_roles = roles
    settings.hud_ctrl_scale = rng.choice([0, 40, 50, 100, 120, 200, 250])
    settings.hud_count_scale = rng.choice([0, 50, 85, 100, 200])
    return settings


def patch_cases(app: Path, payload: Path, shb: Path, random_count: int, seed: int) -> int:
    stub_zstandard()
    installer, presets = load_python_app(app)
    module = (payload / "standalone" / "subsdk9").read_bytes()
    failures = 0
    checked = 0
    work = Path(tempfile.mkdtemp(prefix="shb-parity-"))
    try:
        for key in presets.PROFILE_KEYS:
            settings = installer.Settings(**presets.settings_kwargs(key))
            expected = installer.patch_config(module, settings)
            out = work / f"{key}.subsdk9"
            run_shb(shb, ["patch", "--nso", str(payload / "standalone" / "subsdk9"), "--out", str(out),
                          "--profile", key])
            got = out.read_bytes()
            checked += 1
            if got != expected:
                failures += 1
                print(f"  profile {key:<16} DIFFERENT")
                report_diff(expected, got)
            else:
                print(f"  profile {key:<16} ok ({len(got)} bytes)")

        rng = random.Random(seed)
        for i in range(random_count):
            settings = random_settings(installer, rng)
            expected = installer.patch_config(module, settings)
            settings_file = work / f"random-{i}.json"
            settings_file.write_text(json.dumps(settings.to_dict()), encoding="utf-8")
            out = work / f"random-{i}.subsdk9"
            run_shb(shb, ["patch", "--nso", str(payload / "standalone" / "subsdk9"), "--out", str(out),
                          "--settings", str(settings_file)])
            got = out.read_bytes()
            checked += 1
            if got != expected:
                failures += 1
                print(f"  random #{i} DIFFERENT (settings kept at {settings_file})")
                report_diff(expected, got)
        print(f"patch: {checked - failures}/{checked} images identical")
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return failures


def report_diff(expected: bytes, got: bytes) -> None:
    if len(expected) != len(got):
        print(f"    length {len(expected)} vs {len(got)}")
    diffs = [(i, a, b) for i, (a, b) in enumerate(zip(expected, got)) if a != b]
    print(f"    {len(diffs)} differing bytes, first: {diffs[:12]}")


def zs_payload(blob: bytes, dictionary: bytes) -> bytes:
    """The decompressed content of a `.zs` pack, read with the game's own dictionary."""
    zstandard = importlib.import_module("zstandard")
    decompressor = zstandard.ZstdDecompressor(dict_data=zstandard.ZstdCompressionDict(dictionary))
    return decompressor.decompress(blob, max_output_size=400_000_000)


def tree_digest(root: Path) -> dict[str, str]:
    out = {}
    for path in sorted(root.rglob("*")):
        if path.is_file():
            out[path.relative_to(root).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return out


def install_cases(app: Path, payload: Path, shb: Path, profiles: list[str]) -> int:
    have_zstd = stub_zstandard()
    installer, presets = load_python_app(app)
    installer.resource_dir = lambda: payload  # the script's payload, not the app's own folder
    module_before = (payload / "standalone" / "subsdk9").read_bytes()
    failures = 0
    work = Path(tempfile.mkdtemp(prefix="shb-install-"))
    try:
        checked = 0
        for key in profiles:
            settings = installer.Settings(**presets.settings_kwargs(key))
            wants_packs = (
                any(installer.enemy_levels(settings))
                or any(installer.boss_levels(settings))
                or bool(settings.enemy_alert)
                or bool(settings.enemy_aggro)
            )
            if wants_packs and not have_zstd:
                # The enemy speed packs are patched through a zstd dictionary by the Python app and are not ported
                # to the Rust one yet, so there is nothing to compare.
                print(f"  install {key:<16} skipped (needs zstandard for the enemy packs)")
                continue
            checked += 1
            py_emu = work / f"py-{key}"
            rs_emu = work / f"rs-{key}"
            for emu in (py_emu, rs_emu):
                (emu / "user" / "load").mkdir(parents=True)

            py_result = installer.install(py_emu, settings)
            run_shb(shb, ["install", "--emu", str(rs_emu), "--profile", key, "--payload", str(payload)])

            py_tree = tree_digest(py_result.target)
            rs_tree = tree_digest(rs_emu / "user" / "load" / installer.TITLE_ID / installer.MOD_NAME)
            # The marker file is metadata, not the mod: its format is allowed to differ.
            marker = installer.MARKER
            py_files = {k: v for k, v in py_tree.items() if k != marker}
            rs_files = {k: v for k, v in rs_tree.items() if k != marker}
            differing = [n for n in set(py_files) | set(rs_files) if py_files.get(n) != rs_files.get(n)]
            # The enemy packs are compressed: what matters is the content the game decompresses, and two zstd
            # encoders (Python's libzstd and the Rust crate) are not required to emit the same bytes.
            recompressed = []
            if differing and have_zstd:
                dictionary_path = payload / "romfs_opts" / "enemy_speed_pack.zsdic"
                dictionary = dictionary_path.read_bytes() if dictionary_path.is_file() else None
                for name in list(differing):
                    if dictionary is None or not name.endswith(".zs"):
                        continue
                    py_path = py_result.target / name
                    rs_path = rs_emu / "user" / "load" / installer.TITLE_ID / installer.MOD_NAME / name
                    if not (py_path.is_file() and rs_path.is_file()):
                        continue
                    if zs_payload(py_path.read_bytes(), dictionary) == zs_payload(
                        rs_path.read_bytes(), dictionary
                    ):
                        differing.remove(name)
                        recompressed.append(name)
            if not differing:
                extra = f", {len(recompressed)} packs equal after decompression" if recompressed else ""
                print(f"  install {key:<16} ok ({len(py_files)} files, module + ips + romfs identical{extra})")
            else:
                failures += 1
                print(f"  install {key:<16} DIFFERENT")
                for name in sorted(differing):
                    print(f"    {name}: py={py_files.get(name)} rs={rs_files.get(name)}")
        print(f"install: {checked - failures}/{checked} trees identical")
    finally:
        shutil.rmtree(work, ignore_errors=True)
        # The script's payload must come out unchanged.
        after = (payload / "standalone" / "subsdk9").read_bytes()
        if after != module_before:
            print("WARNING: the payload module changed during the check")
    return failures


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--app", type=Path, default=DEFAULT_APP, help="the Python app folder")
    ap.add_argument("--payload", type=Path, default=None, help="the payload folder (app/payload)")
    ap.add_argument("--shb", type=Path, default=None, help="the shb binary (default: target/debug/shb)")
    ap.add_argument("--mode", choices=["patch", "install", "all"], default="patch")
    ap.add_argument("--random", type=int, default=40, help="random settings to check")
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--profiles", default="souls,hero,souls-hero,world,base,manual-shield,recommended",
                    help="profiles to install (the ones with enemy packs need zstandard installed)")
    args = ap.parse_args()

    app = args.app.resolve()
    payload = (args.payload or app / "payload").resolve()
    shb = (args.shb or Path(__file__).resolve().parents[1] / "target" / "debug" / "shb.exe").resolve()
    if not shb.exists():
        raise SystemExit(f"build it first: cargo build (looked for {shb})")
    if not (payload / "standalone" / "subsdk9").is_file():
        raise SystemExit(f"no payload at {payload} (use --payload)")

    failures = 0
    if args.mode in ("patch", "all"):
        print(f"patch parity ({app})")
        failures += patch_cases(app, payload, shb, args.random, args.seed)
    if args.mode in ("install", "all"):
        print(f"install parity ({payload})")
        failures += install_cases(app, payload, shb, [p for p in args.profiles.split(",") if p])
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
