//! The settings file is the Python app's, and stays the Python app's.
//!
//! `app/paths.py` writes `SHB-OcarinaTweaks.json` (and the older `SuperHeroBoostConfig.json`) next to the program or
//! in the user's config folder; both apps read and write that same file, so a person can switch between them without
//! losing a single option. The fixtures here were produced by the Python app itself (`Settings().to_dict()` from
//! 0.9.3), and the tests hold this port to them: every key parses, every value survives a round trip, and the file
//! written here has the same keys as the file that app writes.

use shb_core::paths::APP_NAME;
use shb_core::settings::Settings;

const DEFAULT_JSON: &str = include_str!("data/settings-0.9.3-default.json");
const RECOMMENDED_JSON: &str = include_str!("data/settings-0.9.3-recommended.json");

fn keys(value: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

/// The old file loads, keeps every value, and comes back with the same keys.
fn round_trip(json: &str) {
    let old: serde_json::Value = serde_json::from_str(json).expect("the Python file is valid JSON");
    let settings: Settings = serde_json::from_str(json).expect("the old settings file loads");
    let new = serde_json::to_value(&settings).expect("they serialize");
    assert_eq!(keys(&old), keys(&new), "the keys of the file changed");

    // Every value the old file had is the value this port reads back, key by key.
    for key in keys(&old) {
        let (before, after) = (&old[key.as_str()], &new[key.as_str()]);
        assert_eq!(before, after, "{key} changed on the way through");
    }
}

#[test]
fn the_default_file_of_the_python_app_loads() {
    round_trip(DEFAULT_JSON);
}

#[test]
fn the_recommended_file_of_the_python_app_loads() {
    round_trip(RECOMMENDED_JSON);
}

#[test]
fn a_file_without_the_newer_options_still_loads() {
    // A 0.4.x file knows nothing about the button table or the HUD scales; it must load with those at their defaults.
    let old = r#"{"stamina": true, "enemy_hp": 200, "damage": 200, "lang": "es"}"#;
    let settings: Settings = serde_json::from_str(old).expect("a small file loads");
    assert!(settings.stamina);
    assert_eq!(settings.enemy_hp, 200);
    assert_eq!(
        settings.hud_ctrl_scale, 100,
        "a missing field takes its default"
    );
    assert!(
        !settings.bm_roles.is_empty(),
        "the role table falls back to the classic one"
    );
}

#[test]
fn the_file_is_the_one_the_python_app_writes() {
    // The name is the contract: `app/paths.py` reads `SHB-OcarinaTweaks.json` (and the older name) and nothing else.
    assert_eq!(APP_NAME, "SHB-OcarinaTweaks");
    // `app/paths.py`: the portable file is `<program>/SHB-OcarinaTweaks.json`, the per-user one is
    // `<config>/SHB-OcarinaTweaks/settings.json`, and the pre-rename folder is `SuperHeroBoostConfig`.
    let (portable, user) = shb_core::paths::settings_paths();
    assert_eq!(portable.file_name().unwrap(), "SHB-OcarinaTweaks.json");
    assert_eq!(user.file_name().unwrap(), "settings.json");
    assert_eq!(
        user.parent().unwrap().file_name().unwrap(),
        "SHB-OcarinaTweaks"
    );
    let (old_portable, old_user) = shb_core::paths::legacy_settings_paths();
    assert_eq!(
        old_portable.file_name().unwrap(),
        "SuperHeroBoostConfig.json"
    );
    assert_eq!(old_user.file_name().unwrap(), "settings.json");
    assert_eq!(
        old_user.parent().unwrap().file_name().unwrap(),
        "SuperHeroBoostConfig"
    );
}
