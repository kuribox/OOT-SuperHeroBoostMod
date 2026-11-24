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
