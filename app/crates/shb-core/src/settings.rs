//! The app's settings: everything the user can choose, with the defaults the Python app used.
//!
//! The JSON is the same file the Python app wrote (`SHB-OcarinaTweaks.json`), so this struct keeps its field
//! names and is `#[serde(default)]`: an old file loads, and a field a newer app added is ignored.

use crate::roles::{self, Button, Role};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An enum whose JSON form is a lowercase string (the Python app used strings for the same options), with one
/// variant marked as the default.
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? } default $default:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub enum $name {
            $(#[serde(rename = $text)] $variant),+
        }

        impl Default for $name {
            fn default() -> Self {
                $name::$default
            }
        }

        impl $name {
            /// Every variant, in the order the app shows them.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }

            pub fn parse(text: &str) -> Option<Self> {
                let lower = text.to_ascii_lowercase();
                Self::ALL.iter().copied().find(|v| v.as_str() == lower)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_enum! {
    /// The HUD layout (config byte `hud_mode`).
    HudMode { Wide => "wide", Simple => "simple", Plain => "plain" } default Wide
}

string_enum! {
    /// The button art buttonMapping installs (config byte `button_style`).
    ButtonStyle { Switch => "switch", Xbox => "xbox", PlayStation => "playstation" } default Switch
}

string_enum! {
    /// Where the face buttons sit in the mod's own HUD (config byte `bm_layout`).
    BmLayout { Nintendo => "nintendo", Ab => "ab", Xbox => "xbox", Gamecube => "gamecube" } default Nintendo
}

string_enum! {
    /// Download the game's own prompts (not part of the module: a romfs layout).
    GameLayout { None => "none", PlayStation => "playstation", Xbox => "xbox" } default None
}

string_enum! {
    /// What the L button does when buttonMapping is on (config byte `bm_l_mode`).
    LMode { Assign => "assign", Selector => "selector" } default Assign
}

string_enum! {
    /// The language of the mod's own HUD labels (config byte `lang`).
    Lang { En => "en", Es => "es", Pt => "pt", Fr => "fr", De => "de", It => "it" } default En
}

impl Lang {
    /// The byte the module reads.
    pub fn byte(self) -> u8 {
        match self {
            Lang::En => 0,
            Lang::Es => 1,
            Lang::Pt => 2,
            Lang::Fr => 3,
            Lang::De => 4,
            Lang::It => 5,
        }
    }

    /// A language code or name (`en-US`, `es-419`, `pt-BR`, `EnglishAmerican`...) as one of our languages.
    pub fn from_tag(value: &str) -> Option<Self> {
        let t = value.trim().to_ascii_lowercase().replace(['_', ' '], "-");
        if t.is_empty() || t.starts_with('#') {
            return None;
        }
        if let Some(lang) = Self::parse(t.split('-').next().unwrap_or("")) {
            return Some(lang);
        }
        for (name, lang) in [
            ("english", Lang::En),
            ("french", Lang::Fr),
            ("german", Lang::De),
            ("spanish", Lang::Es),
            ("italian", Lang::It),
            ("portuguese", Lang::Pt),
        ] {
            if t.starts_with(name) {
                return Some(lang);
            }
        }
        None
    }
}

/// The emulator's system language index → our language (`language_index` in the yuzu family).
pub fn lang_from_index(index: u32) -> Option<Lang> {
    match index {
        1 | 2 => Some(Lang::En),
        3 => Some(Lang::Fr),
        4 => Some(Lang::De),
        5 => Some(Lang::Es),
        6 => Some(Lang::It),
        7 | 8 => Some(Lang::Pt),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub stamina: bool,
    /// 0 hardcore, 1 hardcore combat only, 2 sprint only, 3 advanced
    pub preset: u8,
    pub jump: bool,
    pub attack: bool,
    pub sprint: bool,
    pub block: bool,
    pub shoot: bool,
    pub roll: bool,
    pub combat_only: bool,
    /// enemy health, percent of vanilla
    pub enemy_hp: u16,
    /// sprint (A held while moving) speed, percent of vanilla
    pub sprint_speed: u16,
    /// damage the player receives, percent of vanilla
    pub damage: u16,
    /// hearts dropped by enemies/grass become green rupees (romfs)
    pub no_heart_drop: bool,
    pub hud_rupees: bool,
    pub hud_clock: bool,
    pub clock_24h: bool,
    pub songbook: bool,
    /// hold D-pad down to pick the boots
    pub quick_boots: bool,
    /// hold D-pad up to pick a mask
    pub quick_mask: bool,
    pub hud_collectibles: bool,
    pub hud_mode: HudMode,
    pub enemy_double: bool,
    /// beta: a red health bar over every normal enemy's head
    pub enemy_bars: bool,
    /// beta: the wide on-screen bar with the boss's name
    pub boss_bar: bool,
    /// deprecated: maps to movement/animation/attack at x2
    pub enemy_speed: bool,
    pub enemy_move: u8,
    pub enemy_anim: u8,
    pub enemy_attack: u8,
    pub enemy_proj: u8,
    pub boss_move: u8,
    pub boss_anim: u8,
    /// how far a spotting enemy alerts the others
    pub enemy_alert: u8,
    /// 0 off, 1 aggressive, 2 maximum
    pub enemy_aggro: u8,
    pub great_fairy: bool,
    pub farore_outdoor: bool,
    /// The remap of what every button does (the whole Controls card).
    pub button_mapping: bool,
    /// Custom item HUD: our own B / L / R / ZR item buttons and items 1-3 assignable to any button. Without the
    /// button mapping there is no item side.
    pub hud_items: bool,
    pub button_style: ButtonStyle,
    pub bm_layout: BmLayout,
    pub game_layout: GameLayout,
    pub bm_simple: bool,
    pub bm_l_mode: LMode,
    pub stamina_bar: bool,
    pub manual_shield: bool,
    pub target_r3: bool,
    pub sprint_l3: bool,
    pub auto_target: bool,
    pub switch_target: bool,
    pub lang: Lang,
    pub no_magic_drop: bool,
    pub hide_minimap: bool,
    pub sprint_l3_toggle: bool,
    pub bm_remap: bool,
    pub bm_roles: BTreeMap<Button, Role>,
    /// Per held button (`roles::HOLD_WAIT_KEYS`): `true` = the short press waits for the hold, `false` = it acts at
    /// once and the hold comes on top. A missing button takes its default (D-pad right waits, the rest act).
    pub bm_hold_wait: BTreeMap<Button, bool>,
    /// size of the button cluster HUD, percent
    pub hud_ctrl_scale: u8,
    /// size of the counter row (rupees, clock, collectibles), percent
    pub hud_count_scale: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            stamina: true,
            preset: 0,
            jump: true,
            attack: true,
            sprint: true,
            block: true,
            shoot: true,
            roll: true,
            combat_only: false,
            enemy_hp: 100,
            sprint_speed: 125,
            damage: 100,
            no_heart_drop: false,
            hud_rupees: false,
            hud_clock: false,
            clock_24h: false,
            songbook: true,
            quick_boots: false,
            quick_mask: false,
            hud_collectibles: false,
            hud_mode: HudMode::Wide,
            enemy_double: false,
            enemy_bars: false,
            boss_bar: false,
            enemy_speed: false,
            enemy_move: 0,
            enemy_anim: 0,
            enemy_attack: 0,
            enemy_proj: 0,
            boss_move: 0,
            boss_anim: 0,
            enemy_alert: 0,
            enemy_aggro: 0,
            great_fairy: false,
            farore_outdoor: false,
            button_mapping: true,
            hud_items: true,
            button_style: ButtonStyle::Switch,
            bm_layout: BmLayout::Nintendo,
            game_layout: GameLayout::None,
            bm_simple: false,
            bm_l_mode: LMode::Assign,
            stamina_bar: false,
            manual_shield: false,
            target_r3: false,
            sprint_l3: false,
            auto_target: false,
            switch_target: false,
            lang: Lang::En,
            no_magic_drop: false,
            hide_minimap: false,
            sprint_l3_toggle: false,
            bm_remap: true,
            bm_roles: roles::classic_roles(),
            bm_hold_wait: roles::default_hold_wait(),
            hud_ctrl_scale: 100,
            hud_count_scale: 100,
        }
    }
}

impl Settings {
    /// The settings from a JSON file the Python app wrote (missing fields take the defaults).
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Serialize for the marker file and for `--save-settings`.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn load(path: &std::path::Path) -> crate::Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| crate::Error::io(path, e))?;
        Self::from_json(&text).map_err(|e| crate::Error::io(path, std::io::Error::other(e)))
    }

    /// Write the settings into the file, keeping every other key that is there (the window keeps the emulator, game and
    /// Rekarina folders, the target and the language in the same file; a plain overwrite erased them).
    pub fn save(&self, path: &std::path::Path) -> crate::Result<()> {
        crate::error::write(path, self.merged_into(path).as_bytes())
    }

    fn merged_into(&self, path: &std::path::Path) -> String {
        let Ok(serde_json::Value::Object(mine)) = serde_json::to_value(self) else {
            return self.to_json();
        };
        let mut doc = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| match value {
                serde_json::Value::Object(object) => Some(object),
                _ => None,
            })
            .unwrap_or_default();
        doc.extend(mine);
        serde_json::to_string_pretty(&serde_json::Value::Object(doc)).unwrap_or_else(|_| self.to_json())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_names_match_the_python_app() {
        let s = Settings::default();
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["hud_mode"], "wide");
        assert_eq!(json["button_style"], "switch");
        assert_eq!(json["bm_layout"], "nintendo");
        assert_eq!(json["game_layout"], "none");
        assert_eq!(json["bm_l_mode"], "assign");
        assert_eq!(json["lang"], "en");
        assert_eq!(json["bm_roles"]["B"], "item1");
        assert_eq!(json["bm_roles"]["A_hold"], "none");
        assert_eq!(json["hud_ctrl_scale"], 100);
    }

    #[test]
    fn saving_keeps_the_folders_the_window_stored_in_the_file() {
        let dir = std::env::temp_dir().join(format!("shb-save-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{"emu": "E:/Emu", "recomp": "F:/Game", "target": "recomp", "enemy_hp": 100}"#).unwrap();
        let mut s = Settings::default();
        s.enemy_hp = 300;
        s.save(&path).unwrap();
        let back: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["emu"], "E:/Emu");
        assert_eq!(back["recomp"], "F:/Game");
        assert_eq!(back["target"], "recomp");
        assert_eq!(back["enemy_hp"], 300, "the settings themselves are written");
        assert_eq!(Settings::load(&path).unwrap().enemy_hp, 300);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_old_file_loads_with_defaults() {
        let s = Settings::from_json(
            r#"{"enemy_hp": 200, "bm_roles": {"B": "item2"}, "unknown_key": 1}"#,
        )
        .unwrap();
        assert_eq!(s.enemy_hp, 200);
        assert_eq!(s.damage, 100, "missing fields take the defaults");
        assert_eq!(s.bm_roles[&Button::B], Role::Item2);
        assert_eq!(
            s.bm_roles.len(),
            1,
            "a partial table is fine: normalize fills it"
        );
    }

    #[test]
    fn language_tags_and_indices() {
        assert_eq!(Lang::from_tag("es-419"), Some(Lang::Es));
        assert_eq!(Lang::from_tag("pt_BR"), Some(Lang::Pt));
        assert_eq!(Lang::from_tag("EnglishAmerican"), Some(Lang::En));
        assert_eq!(Lang::from_tag("#comment"), None);
        assert_eq!(Lang::from_tag(""), None);
        assert_eq!(lang_from_index(5), Some(Lang::Es));
        assert_eq!(lang_from_index(9), None);
        assert_eq!(Lang::Es.byte(), 1);
    }
}
