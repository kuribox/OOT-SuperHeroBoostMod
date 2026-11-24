//! Game profiles: a complete configuration each, so a profile cannot mean one thing in the window and another
//! on the command line.
//!
//! Every named profile includes the HUD and buttonMapping, and fixes the stamina system, the difficulty, the
//! heart drops, the sprint speed and the enemy mods. `custom` is the balanced default the window starts from.

use crate::roles;
use crate::settings::{ButtonStyle, HudMode, LMode, Lang, Settings};

/// The stamina costs a profile switches on, in this order.
pub const COST_KEYS: [&str; 6] = ["jump", "attack", "sprint", "block", "shoot", "roll"];
/// The named profiles, in the order the window lists them, plus `custom`.
pub const PROFILE_KEYS: [&str; 16] = [
    "recommended",
    "recommended-lite",
    "souls",
    "souls-hero",
    "base",
    "manual-shield",
    "hero",
    "hero-stamina",
    "hero-explorer",
    "super-hero",
    "ultra-hero",
    "survival",
    "world",
    "beta-enemies",
    "beta-all",
    "custom",
];
pub const DEFAULT_PROFILE: &str = "recommended";

const SPRINT_DEFAULT: u16 = 125; // "custom" starts here: a gentle +25 %
const SPRINT_PROFILE: u16 = 125; // every named profile: the same +25 %

#[derive(Debug, Clone)]
pub struct Profile {
    pub key: &'static str,
    /// buttonMapping (our own item buttons); the style, the layout and the L mode are not part of a profile
    pub button_mapping: bool,
    /// Custom item HUD: our own B / L / R / ZR item buttons and items 1-3 assignable (`hud_items`).
    pub hud_items: bool,
    pub stamina: bool,
    /// 0 hardcore, 1 combat only, 2 sprint only, 3 advanced
    pub mode: u8,
    /// one per `COST_KEYS`
    pub costs: [bool; 6],
    pub combat_only: bool,
    pub enemy_hp: u16,
    pub damage: u16,
    pub no_heart_drop: bool,
    pub sprint_speed: u16,
    pub hud_rupees: bool,
    pub hud_clock: bool,
    pub clock_24h: bool,
    pub songbook: bool,
    pub quick_boots: bool,
    pub quick_mask: bool,
    pub hud_collectibles: bool,
    pub hud_mode: HudMode,
    pub enemy_double: bool,
    pub enemy_move: u8,
    pub enemy_anim: u8,
    pub enemy_attack: u8,
    pub enemy_proj: u8,
    pub enemy_alert: u8,
    pub enemy_aggro: u8,
    pub boss_move: u8,
    pub boss_anim: u8,
    pub great_fairy: bool,
    pub farore_outdoor: bool,
    /// `None`: keep the user's own choice (the profiles that do not care)
    pub stamina_bar: Option<bool>,
    pub manual_shield: bool,
    pub auto_target: bool,
    pub switch_target: bool,
    pub target_r3: Option<bool>,
    pub sprint_l3: Option<bool>,
    pub sprint_l3_toggle: Option<bool>,
    /// the armory on the held D-pad right; `None` follows `button_mapping`
    pub armory: Option<bool>,
    /// a table of `roles::remap_preset` the profile applies (`None`: the user's own)
    pub controls: Option<&'static str>,
}

impl Default for Profile {
    /// The values `presets._profile()` starts from.
    fn default() -> Self {
        Profile {
            key: "custom",
            button_mapping: true,
            hud_items: true,
            stamina: true,
            mode: 0,
            costs: [true; 6],
            combat_only: false,
            enemy_hp: 100,
            damage: 100,
            no_heart_drop: false,
            sprint_speed: SPRINT_PROFILE,
            hud_rupees: true,
            hud_clock: true,
            clock_24h: true,
            songbook: true,
            quick_boots: true,
            quick_mask: true,
            hud_collectibles: false,
            hud_mode: HudMode::Simple,
            enemy_double: false,
            enemy_move: 0,
            enemy_anim: 0,
            enemy_attack: 0,
            enemy_proj: 0,
            enemy_alert: 0,
            enemy_aggro: 0,
            boss_move: 0,
            boss_anim: 0,
            great_fairy: false,
            farore_outdoor: false,
            stamina_bar: None,
            manual_shield: false,
            auto_target: false,
            switch_target: false,
            target_r3: None,
            sprint_l3: None,
            sprint_l3_toggle: None,
            armory: None,
            controls: None,
        }
    }
}

impl Profile {
    fn named(key: &'static str) -> Self {
        Profile {
            key,
            ..Default::default()
        }
    }

    pub fn armory(&self) -> bool {
        self.armory.unwrap_or(self.button_mapping)
    }
}

/// A named profile, or the balanced defaults for `custom` / an unknown key.
pub fn profile(key: &str) -> Profile {
    let mut p = match key {
        // The author's own setup: x2 enemy health and damage, no heart drops, hardcore stamina, every enemy mod
        // at x2, the simple HUD and every control improvement.
        "recommended" => Profile {
            enemy_hp: 200,
            damage: 200,
            no_heart_drop: true,
            hud_collectibles: true,
            great_fairy: true,
            farore_outdoor: true,
            stamina_bar: Some(false),
            enemy_double: true,
            enemy_move: 2,
            enemy_anim: 2,
            enemy_attack: 2,
            enemy_proj: 2,
            enemy_alert: 1,
            enemy_aggro: 1,
            boss_move: 2,
            boss_anim: 2,
            manual_shield: true,
            auto_target: true,
            switch_target: true,
            target_r3: Some(true),
            sprint_l3: Some(true),
            sprint_l3_toggle: Some(true),
            controls: Some("triggers"),
            ..Profile::named("recommended")
        },
        // The same without the experimental enemy mods: a first run, a weaker PC, or the puzzles the duplicated
        // enemies can break.
        "recommended-lite" => Profile {
            enemy_hp: 200,
            damage: 200,
            no_heart_drop: true,
            hud_collectibles: true,
            great_fairy: true,
            farore_outdoor: true,
            manual_shield: true,
            auto_target: true,
            switch_target: true,
            target_r3: Some(true),
            sprint_l3: Some(true),
            sprint_l3_toggle: Some(true),
            controls: Some("triggers"),
            ..Profile::named("recommended-lite")
        },
        // Souls-like controls on the vanilla difficulty.
        "souls" => Profile {
            hud_collectibles: true,
            stamina_bar: Some(true),
            manual_shield: true,
            auto_target: true,
            switch_target: true,
            target_r3: Some(true),
            ..Profile::named("souls")
        },
        // The same at the default difficulty.
        "souls-hero" => Profile {
            enemy_hp: 200,
            damage: 200,
            no_heart_drop: true,
            hud_collectibles: true,
            stamina_bar: Some(true),
            manual_shield: true,
            auto_target: true,
            switch_target: true,
            target_r3: Some(true),
            ..Profile::named("souls-hero")
        },
        // Vanilla difficulty with the pack's HUD and extra buttons.
        "base" => Profile {
            stamina: false,
            hud_collectibles: true,
            ..Profile::named("base")
        },
        // Only the manual shield: no stamina, no item HUD of ours, the wide layout with every counter off. The
        // mapping itself stays on (it is the whole Controls card); what goes is the item side.
        "manual-shield" => Profile {
            stamina: false,
            hud_items: false,
            songbook: false,
            hud_rupees: false,
            hud_clock: false,
            hud_mode: HudMode::Wide,
            quick_boots: false,
            quick_mask: false,
            hud_collectibles: false,
            manual_shield: true,
            ..Profile::named("manual-shield")
        },
        // Classic Hero Mode: x1.5 enemy health, x2 damage, no hearts, no stamina.
        "hero" => Profile {
            stamina: false,
            enemy_hp: 150,
            damage: 200,
            no_heart_drop: true,
            hud_collectibles: true,
            manual_shield: true,
            auto_target: true,
            switch_target: true,
            ..Profile::named("hero")
        },
        // Hero Mode plus stamina on every action.
        "hero-stamina" => Profile {
            enemy_hp: 150,
            damage: 200,
            no_heart_drop: true,
            hud_collectibles: true,
            manual_shield: true,
            auto_target: true,
            switch_target: true,
            ..Profile::named("hero-stamina")
        },
        // Hero Mode with stamina only on sprint.
        "hero-explorer" => Profile {
            mode: 2,
            costs: [false, false, true, false, false, false],
            enemy_hp: 150,
            damage: 200,
            no_heart_drop: true,
            hud_collectibles: true,
            manual_shield: true,
            auto_target: true,
            switch_target: true,
            ..Profile::named("hero-explorer")
        },
        // Above the default: x2.5 and every enemy at x1.5, without duplicating placements.
        "super-hero" => Profile {
            enemy_hp: 250,
            damage: 250,
            no_heart_drop: true,
            hud_collectibles: true,
            great_fairy: true,
            farore_outdoor: true,
            enemy_move: 1,
            enemy_anim: 1,
            enemy_attack: 1,
            enemy_proj: 1,
            boss_move: 1,
            boss_anim: 1,
            manual_shield: true,
            auto_target: true,
            switch_target: true,
            target_r3: Some(true),
            sprint_l3: Some(true),
            sprint_l3_toggle: Some(true),
            controls: Some("triggers"),
            ..Profile::named("super-hero")
        },
        // The default with x3 numbers and every enemy mod at x2.
        "ultra-hero" => Profile {
            enemy_hp: 300,
            damage: 300,
            no_heart_drop: true,
            hud_collectibles: true,
            great_fairy: true,
            farore_outdoor: true,
            enemy_double: true,
            enemy_move: 2,
            enemy_anim: 2,
            enemy_attack: 2,
            enemy_proj: 2,
            enemy_alert: 1,
            enemy_aggro: 1,
            boss_move: 2,
            boss_anim: 2,
            manual_shield: true,
            auto_target: true,
            switch_target: true,
            target_r3: Some(true),
            sprint_l3: Some(true),
            sprint_l3_toggle: Some(true),
            controls: Some("triggers"),
            ..Profile::named("ultra-hero")
        },
        // Endurance run: the default difficulty and hardcore stamina, without the enemy mods.
        "survival" => Profile {
            enemy_hp: 200,
            damage: 200,
            no_heart_drop: true,
            hud_collectibles: true,
            manual_shield: true,
            auto_target: true,
            switch_target: true,
            ..Profile::named("survival")
        },
        // Only the world changes (Great Fairy rewards and Farore's Wind outdoors).
        "world" => Profile {
            stamina: false,
            hud_collectibles: true,
            great_fairy: true,
            farore_outdoor: true,
            ..Profile::named("world")
        },
        // Beta: the enemy mods on top of the vanilla game.
        "beta-enemies" => Profile {
            stamina: false,
            hud_collectibles: true,
            enemy_double: true,
            enemy_move: 2,
            enemy_anim: 2,
            enemy_attack: 2,
            enemy_proj: 2,
            enemy_alert: 1,
            enemy_aggro: 1,
            boss_move: 2,
            boss_anim: 2,
            ..Profile::named("beta-enemies")
        },
        // Beta: the experimental ceiling.
        "beta-all" => Profile {
            enemy_hp: 300,
            damage: 300,
            no_heart_drop: true,
            hud_collectibles: true,
            great_fairy: true,
            farore_outdoor: true,
            enemy_double: true,
            enemy_move: 3,
            enemy_anim: 3,
            enemy_attack: 3,
            enemy_proj: 3,
            enemy_alert: 2,
            enemy_aggro: 2,
            boss_move: 3,
            boss_anim: 3,
            manual_shield: true,
            auto_target: true,
            switch_target: true,
            target_r3: Some(true),
            sprint_l3: Some(true),
            sprint_l3_toggle: Some(true),
            controls: Some("triggers"),
            ..Profile::named("beta-all")
        },
        // `custom`: the balanced starting values (`presets.defaults()`), which are the profile defaults except for
        // the wide HUD and no armory (the classic table, untouched).
        _ => Profile {
            key: "custom",
            hud_mode: HudMode::Wide,
            sprint_speed: SPRINT_DEFAULT,
            armory: Some(false),
            ..Default::default()
        },
    };
    p.key = profile_key(key);
    p
}

fn profile_key(key: &str) -> &'static str {
    PROFILE_KEYS
        .iter()
        .find(|k| **k == key)
        .copied()
        .unwrap_or("custom")
}

/// What choosing a profile does to the settings on the window (`main.py`'s `apply_profile`): its values go in, the
/// choices a profile does not make — the style, the layout, the L mode, the language, the scales, the magic drops —
/// stay the user's, and so do the stamina display, the R3 target and the L3 sprint when the profile does not say.
/// The button table is the user's, with the profile's own table over it when it has one.
pub fn apply_profile(s: &mut Settings, key: &str) {
    if key == "custom" || !PROFILE_KEYS.contains(&key) {
        return;
    }
    let p = profile(key);
    s.button_mapping = p.button_mapping;
    s.hud_items = p.hud_items;
    s.stamina = p.stamina;
    s.songbook = p.songbook;
    s.preset = p.mode;
    s.combat_only = p.combat_only;
    [s.jump, s.attack, s.sprint, s.block, s.shoot, s.roll] = p.costs;
    s.enemy_hp = p.enemy_hp;
    s.damage = p.damage;
    s.no_heart_drop = p.no_heart_drop;
    s.sprint_speed = p.sprint_speed;
    s.hud_rupees = p.hud_rupees;
    s.hud_clock = p.hud_clock;
    s.clock_24h = p.clock_24h;
    s.quick_boots = p.quick_boots;
    s.quick_mask = p.quick_mask;
    s.hud_collectibles = p.hud_collectibles;
    s.hud_mode = p.hud_mode;
    s.enemy_double = p.enemy_double;
    s.enemy_move = p.enemy_move;
    s.enemy_anim = p.enemy_anim;
    s.enemy_attack = p.enemy_attack;
    s.enemy_proj = p.enemy_proj;
    s.enemy_alert = p.enemy_alert;
    s.enemy_aggro = p.enemy_aggro;
    s.boss_move = p.boss_move;
    s.boss_anim = p.boss_anim;
    s.great_fairy = p.great_fairy;
    s.farore_outdoor = p.farore_outdoor;
    s.manual_shield = p.manual_shield;
    s.auto_target = p.auto_target;
    s.switch_target = p.switch_target;
    // Personal preferences: only a profile that sets them changes them.
    if let Some(v) = p.stamina_bar {
        s.stamina_bar = v;
    }
    if let Some(v) = p.target_r3 {
        s.target_r3 = v;
    }
    if let Some(v) = p.sprint_l3 {
        s.sprint_l3 = v;
    }
    if let Some(v) = p.sprint_l3_toggle {
        s.sprint_l3_toggle = v;
    }
    let mut table = s.bm_roles.clone();
    if let Some(preset) = p.controls.and_then(roles::remap_preset) {
        table = roles::keep_holds(&preset, &table);
    }
    let table = roles::roles_from_flags(
        &table,
        Some(p.manual_shield),
        p.target_r3,
        p.sprint_l3,
    );
    s.bm_roles = if p.armory() {
        roles::roles_with_armory(&table)
    } else {
        table
    };
}

/// The settings a profile means, ready to patch.
pub fn settings_for(key: &str) -> Settings {
    let p = profile(key);
    let table = p
        .controls
        .and_then(roles::remap_preset)
        .unwrap_or_else(roles::classic_roles);
    Settings {
        bm_roles: if p.armory() {
            roles::roles_with_armory(&table)
        } else {
            roles::normalize(&table)
        },
        button_mapping: p.button_mapping,
        hud_items: p.hud_items,
        stamina: p.stamina,
        songbook: p.songbook,
        preset: p.mode,
        combat_only: p.combat_only,
        jump: p.costs[0],
        attack: p.costs[1],
        sprint: p.costs[2],
        block: p.costs[3],
        shoot: p.costs[4],
        roll: p.costs[5],
        enemy_hp: p.enemy_hp,
        damage: p.damage,
        no_heart_drop: p.no_heart_drop,
        sprint_speed: p.sprint_speed,
        hud_rupees: p.hud_rupees,
        hud_clock: p.hud_clock,
        clock_24h: p.clock_24h,
        quick_boots: p.quick_boots,
        quick_mask: p.quick_mask,
        hud_collectibles: p.hud_collectibles,
        hud_mode: p.hud_mode,
        enemy_double: p.enemy_double,
        enemy_move: p.enemy_move,
        enemy_anim: p.enemy_anim,
        enemy_attack: p.enemy_attack,
        enemy_proj: p.enemy_proj,
        enemy_alert: p.enemy_alert,
        enemy_aggro: p.enemy_aggro,
        boss_move: p.boss_move,
        boss_anim: p.boss_anim,
        great_fairy: p.great_fairy,
        farore_outdoor: p.farore_outdoor,
        stamina_bar: p.stamina_bar.unwrap_or(false),
        manual_shield: p.manual_shield,
        auto_target: p.auto_target,
        switch_target: p.switch_target,
        target_r3: p.target_r3.unwrap_or(false),
        sprint_l3: p.sprint_l3.unwrap_or(false),
        sprint_l3_toggle: p.sprint_l3_toggle.unwrap_or(false),
        // The style, the layout, the L mode and the scales are not part of a profile: the user's own choice.
        ..Settings {
            button_style: ButtonStyle::Switch,
            bm_layout: crate::settings::BmLayout::Nintendo,
            game_layout: crate::settings::GameLayout::None,
            bm_l_mode: LMode::Assign,
            lang: Lang::En,
            ..Settings::default()
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn choosing_a_profile_keeps_what_the_profile_does_not_choose() {
        let mut s = Settings::default();
        s.stamina_bar = true;
        s.hud_ctrl_scale = 140;
        s.no_magic_drop = true;
        // `base` says nothing about the stamina display, the R3 target or the L3 sprint.
        apply_profile(&mut s, "base");
        assert!(s.stamina_bar, "the display is the player's");
        assert_eq!(s.hud_ctrl_scale, 140);
        assert!(s.no_magic_drop);
        assert!(!s.stamina && s.hud_collectibles);
        // `recommended` sets them.
        apply_profile(&mut s, "recommended");
        assert!(!s.stamina_bar && s.target_r3 && s.sprint_l3 && s.sprint_l3_toggle);
        assert_eq!((s.enemy_hp, s.damage), (200, 200));
        assert_eq!(s.bm_roles[&crate::roles::Button::R3], crate::roles::Role::Target);
        assert_eq!(s.bm_roles[&crate::roles::Button::Dr], crate::roles::Role::Armory);
        assert_eq!(s.hud_mode, HudMode::Simple);
        // `custom` changes nothing.
        let before = serde_json::to_string(&s).unwrap();
        apply_profile(&mut s, "custom");
        assert_eq!(serde_json::to_string(&s).unwrap(), before);
    }

    use super::*;

    #[test]
    fn the_default_profile_is_the_authors_setup() {
        let s = settings_for("recommended");
        assert_eq!(s.enemy_hp, 200);
        assert_eq!(s.damage, 200);
        assert!(s.stamina && !s.stamina_bar);
        assert!(s.hud_collectibles);
        assert_eq!(s.enemy_move, 2);
        assert_eq!(s.enemy_anim, 2);
        assert_eq!(s.boss_move, 2);
        assert!(s.manual_shield && s.target_r3 && s.sprint_l3 && s.sprint_l3_toggle);
        assert_eq!(s.preset, 0);
        // The "triggers" table with the armory on the held D-pad right.
        assert_eq!(
            s.bm_roles[&crate::roles::Button::B],
            crate::roles::Role::Selector
        );
        assert_eq!(
            s.bm_roles[&crate::roles::Button::Dr],
            crate::roles::Role::Armory
        );
        assert_eq!(
            s.bm_roles[&crate::roles::Button::BHold],
            crate::roles::Role::Back
        );
    }

    #[test]
    fn profiles_differ_where_they_should() {
        let base = settings_for("base");
        assert!(!base.stamina);
        assert!(base.button_mapping);
        let shield = settings_for("manual-shield");
        assert!(shield.manual_shield);
        assert!(
            shield.button_mapping,
            "the mapping is the whole Controls card and stays on"
        );
        assert!(!shield.hud_items, "what goes is the item HUD");
        assert!(!shield.songbook);
        assert_eq!(shield.hud_mode, HudMode::Wide);
        assert!(!shield.hud_rupees && !shield.hud_clock);
        let explorer = settings_for("hero-explorer");
        assert_eq!(explorer.preset, 2);
        assert_eq!((explorer.jump, explorer.sprint), (false, true));
        let hero = settings_for("hero");
        assert_eq!(hero.enemy_hp, 150);
        assert!(!hero.stamina);
    }

    #[test]
    fn custom_is_the_balanced_default_with_the_wide_hud() {
        let s = settings_for("custom");
        assert_eq!(s.enemy_hp, 100);
        assert_eq!(s.sprint_speed, 125);
        assert_eq!(s.hud_mode, HudMode::Wide);
        // `presets.defaults()` of the Python app: every counter on and the classic table as it is (no armory: that
        // is a profile's own addition).
        assert!(s.hud_rupees && s.hud_clock && s.clock_24h && s.quick_boots && s.quick_mask);
        assert!(s.stamina && s.songbook && s.button_mapping);
        assert_eq!(s.bm_roles, roles::normalize(&roles::classic_roles()));
    }

    #[test]
    fn every_key_resolves() {
        for key in PROFILE_KEYS {
            let s = settings_for(key);
            assert!(s.enemy_hp >= 100, "{key}");
        }
        assert_eq!(settings_for("nope").enemy_hp, 100);
    }

    #[test]
    fn the_armory_only_comes_with_the_buttons() {
        let s = settings_for("recommended-lite");
        assert_eq!(
            s.bm_roles[&crate::roles::Button::Dr],
            crate::roles::Role::Armory
        );
        // The table keeps every basic role: the game needs them.
        assert!(roles::roles_missing(&s.bm_roles).is_empty());
    }
}
