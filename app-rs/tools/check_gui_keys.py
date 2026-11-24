#!/usr/bin/env python3
"""Every text key the window asks for must exist in the generated table.

`shb-gui` looks its texts up by key (`tr(lang, "enemy_hp")`); a typo shows the key itself in the window, which is not
obvious in a screenshot. This walks the window's source, collects every literal key it passes to `tr` (and the keys
built by the few small helpers that do), and compares them with `i18n_table.rs`.

    python app-rs/tools/check_gui_keys.py            # report and exit 1 when a key is missing
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "app-rs" / "crates" / "shb-gui" / "src"
TABLE = SRC / "i18n_table.rs"

# Keys built at run time by a helper: the prefixes those helpers can emit.
DYNAMIC = {
    "preset_desc_key": ["m_hardcore_d", "m_combat_d", "m_sprint_d", "m_advanced_d"],
    "preset_name_key": ["preset_classic", "preset_action_down", "preset_shield", "preset_souls", "preset_triggers", "preset_gamecube"],
    "preset_desc_key_for": [
        "ctrl_preset_classic_d",
        "ctrl_preset_action_down_d",
        "ctrl_preset_shield_d",
        "ctrl_preset_souls_d",
        "ctrl_preset_triggers_d",
        "ctrl_preset_gamecube_d",
    ],
    "role_key": [
        "role_none", "role_jump", "role_sword", "role_action", "role_target", "role_item1", "role_item2",
        "role_item3", "role_item_main", "role_camera", "role_sprint", "role_shield", "role_selector",
        "role_armory", "role_back",
    ],
    "role_help_key": ["role_action_help", "role_item_main_help", "role_armory_help", "role_back_help", "opt_switch_help", "opt_toggle_help"],
    "profile_name": [
        "p_recommended", "p_recommended_lite", "p_souls", "p_souls_hero", "p_base", "p_manual_shield", "p_hero",
        "p_hero_stamina", "p_hero_explorer", "p_super_hero", "p_ultra_hero", "p_survival", "p_world",
        "p_beta_enemies", "p_beta_all", "profile_custom",
    ],
    "profile_desc_key": [
        "p_recommended_d", "p_recommended_lite_d", "p_souls_d", "p_souls_hero_d", "p_base_d", "p_manual_shield_d",
        "p_hero_d", "p_hero_stamina_d", "p_hero_explorer_d", "p_super_hero_d", "p_ultra_hero_d", "p_survival_d",
        "p_world_d", "p_beta_enemies_d", "p_beta_all_d", "p_custom_d",
    ],
    "help_title_key": ["help_start_title", "help_install_title", "help_controls_title", "help_rekarina_title", "help_problems_title"],
    "help_body_key": ["help_start_body", "help_install_body", "help_controls_body", "help_rekarina_body", "help_problems_body"],
}


def table_keys() -> set[str]:
    text = TABLE.read_text(encoding="utf-8")
    keys = set(re.findall(r'^\s*\("([a-z0-9_]+)",\s*\[', text, re.M))
    # The window's own few labels live in i18n.rs (`EXTRA`).
    extra = (SRC / "i18n.rs").read_text(encoding="utf-8")
    block = extra[extra.index("static EXTRA"):]
    keys |= set(re.findall(r'^\s*"([a-z0-9_]+)",\s*$', block, re.M))
    return keys


def used_keys() -> dict[str, list[str]]:
    found: dict[str, list[str]] = {}
    for path in sorted(SRC.glob("*.rs")):
        text = path.read_text(encoding="utf-8")
        if path.name == "i18n.rs":  # the table's own tests pass a key that must not resolve
            continue
        for key in re.findall(r'tr\(\s*(?:lang|self\.ui_lang|Lang::\w+)\s*,\s*"([a-z0-9_]+)"', text):
            found.setdefault(key, []).append(path.name)
        for helper, keys in DYNAMIC.items():
            if helper in text:
                for key in keys:
                    found.setdefault(key, []).append(f"{path.name}:{helper}")
    return found


def main() -> int:
    keys = table_keys()
    used = used_keys()
    missing = sorted(k for k in used if k not in keys)
    print(f"{len(used)} keys used, {len(keys)} in the table")
    if missing:
        print("missing from the table (the window would show the key itself):")
        for key in missing:
            print(f"  {key}  ({', '.join(sorted(set(used[key])))})")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
