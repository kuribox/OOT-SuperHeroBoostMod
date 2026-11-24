//! Settings paths. Portable first (the file next to the program), per-user second: the app may live in a folder
//! it cannot write to (`C:\Program Files`, an AppImage mount, `/Applications`).
//!
//! The names are the Python app's, including the legacy one, so an update keeps the user's settings.

use crate::settings::Settings;
use std::path::PathBuf;

pub const APP_NAME: &str = "SHB-OcarinaTweaks";
pub const LEGACY_APP_NAME: &str = "SuperHeroBoostConfig";

/// The folder of the executable.
pub fn program_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The user's home folder.
pub fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// The per-user configuration folder of the platform.
pub fn user_config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join("AppData/Roaming"));
        base.join(APP_NAME)
    }
    #[cfg(target_os = "macos")]
    {
        home().join("Library/Application Support").join(APP_NAME)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"));
        base.join(APP_NAME)
    }
}

/// The per-user data folder: the payload unpacked from the executable lives here (a cache, not the settings).
pub fn user_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join("AppData/Local"));
        base.join(APP_NAME)
    }
    #[cfg(target_os = "macos")]
    {
        home().join("Library/Caches").join(APP_NAME)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".cache"));
        base.join(APP_NAME)
    }
}

/// Where a new settings file goes: portable first, per-user second.
pub fn settings_paths() -> (PathBuf, PathBuf) {
    (
        program_dir().join(format!("{APP_NAME}.json")),
        user_config_dir().join("settings.json"),
    )
}

/// Where the pre-0.9 app saved them (read only, so an update keeps them).
pub fn legacy_settings_paths() -> (PathBuf, PathBuf) {
    (
        program_dir().join(format!("{LEGACY_APP_NAME}.json")),
        user_config_dir()
            .with_file_name(LEGACY_APP_NAME)
            .join("settings.json"),
    )
}

/// Load the settings: portable file, per-user file, legacy file, then the defaults.
pub fn load_settings() -> (Settings, Option<PathBuf>) {
    let (portable, user) = settings_paths();
    let (old_portable, old_user) = legacy_settings_paths();
    for path in [&portable, &user, &old_portable, &old_user] {
        if path.is_file()
            && let Ok(settings) = Settings::load(path)
        {
            return (settings, Some(path.clone()));
        }
    }
    (Settings::default(), None)
}

/// Save where `load_settings` would look first, falling back to the per-user folder when the program's own folder
/// is read-only. Returns the file actually written.
pub fn save_settings(settings: &Settings) -> crate::Result<PathBuf> {
    let (portable, user) = settings_paths();
    match settings.save(&portable) {
        Ok(()) => Ok(portable),
        Err(_) => {
            if let Some(dir) = user.parent() {
                std::fs::create_dir_all(dir).map_err(|e| crate::Error::io(dir, e))?;
            }
            settings.save(&user)?;
            Ok(user)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_have_the_python_names() {
        let (portable, user) = settings_paths();
        assert!(portable.ends_with("SHB-OcarinaTweaks.json"));
        assert!(user.ends_with("settings.json"));
        assert!(user.to_string_lossy().contains("SHB-OcarinaTweaks"));
        let (legacy, _) = legacy_settings_paths();
        assert!(legacy.ends_with("SuperHeroBoostConfig.json"));
    }
}

/// What the window remembers about where things are: the folders of the three targets, the builder, the profile and
/// the language. The Python app keeps them in its own settings file (`emu`, `recomp`, `rekarina`, `recomp_builder`,
/// `target`, `profile`, `lang`), so this port does the same: the same file, the same keys, and a user keeps their
/// folders when they switch between the two.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiState {
    pub emu: String,
    pub recomp: String,
    pub rekarina: String,
    pub builder: String,
    pub target: String,
    pub profile: String,
    pub lang: String,
}

impl UiState {
    pub fn is_empty(&self) -> bool {
        *self == UiState::default()
    }
}

/// The file the UI state lives in: the first settings file that exists (portable, per-user, then the old name's).
fn any_settings_file() -> Option<PathBuf> {
    let (portable, user) = settings_paths();
    let (old_portable, old_user) = legacy_settings_paths();
    [portable, user, old_portable, old_user]
        .into_iter()
        .find(|path| path.is_file())
}

/// Read the UI state, if there is a settings file with any of those keys.
pub fn load_ui_state() -> UiState {
    let Some(path) = any_settings_file() else {
        return UiState::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return UiState::default();
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
        return UiState::default();
    };
    let text_of = |key: &str| {
        doc.get(key)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string()
    };
    UiState {
        emu: text_of("emu"),
        recomp: text_of("recomp"),
        rekarina: text_of("rekarina"),
        builder: text_of("recomp_builder"),
        target: text_of("target"),
        profile: text_of("profile"),
        lang: text_of("lang"),
    }
}

/// Write those keys into the settings file, keeping everything else that is there.
///
/// The file may not exist yet, or hold an older version: the keys are merged into what is on disk instead of writing a
/// file of their own, so the mod settings and the UI state never disagree.
pub fn save_ui_state(state: &UiState) -> crate::Result<()> {
    let (portable, user) = settings_paths();
    let path = if portable.is_file() || !user.exists() {
        portable
    } else {
        user
    };
    let mut doc = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    let Some(object) = doc.as_object_mut() else {
        return Err(crate::Error::io(
            path,
            std::io::Error::other("the settings file is not an object"),
        ));
    };
    for (key, value) in [
        ("emu", &state.emu),
        ("recomp", &state.recomp),
        ("rekarina", &state.rekarina),
        ("recomp_builder", &state.builder),
        ("target", &state.target),
        ("profile", &state.profile),
        ("lang", &state.lang),
    ] {
        object.insert(key.to_string(), serde_json::Value::String(value.clone()));
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| crate::Error::io(parent, e))?;
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&doc).unwrap_or_default(),
    )
    .map_err(|e| crate::Error::io(&path, e))
}
