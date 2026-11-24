//! Finding things on disk: the emulator's mods folder, the emulator program, the game's language.
//!
//! The same rules as the Python app, because the users' folders are what they are: an emulator path may point at
//! the program's folder, at its `user` folder or at the title folder, and a custom `load_directory` in
//! `qt-config.ini` wins over everything.

use crate::settings::Lang;
use std::fs;
use std::path::{Path, PathBuf};

pub const TITLE_ID: &str = "0400486024DEC000";
pub const MOD_NAME: &str = "zz-Super-Hero-Boost"; // must sort last: the loader lets the last mod win each file
pub const MARKER: &str = ".super-hero-boost";

const EMU_EXES: [&str; 9] = [
    "eden", "citron", "sudachi", "suyu", "torzu", "yuzu", "ryujinx", "kenji-nx", "kenjinx",
];
const EMU_SKIP: [&str; 7] = [
    "maintenance",
    "update",
    "uninst",
    "crash",
    "cmd",
    "room",
    "tester",
];

fn read_text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().or_else(|| {
        // Emulator configs are not always valid UTF-8: lose the bad bytes rather than the file.
        fs::read(path)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    })
}

/// `load_directory` from the emulator's `qt-config.ini`, when it points at a folder that exists.
pub fn load_directory(emu: &Path) -> Option<PathBuf> {
    for cfg in [
        emu.join("user/config/qt-config.ini"),
        emu.join("config/qt-config.ini"),
    ] {
        let Some(text) = read_text(&cfg) else {
            continue;
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() != "load_directory" {
                continue;
            }
            let value = value.trim().trim_matches('"');
            if value.is_empty() {
                continue;
            }
            let path = PathBuf::from(value);
            let path = if path.is_absolute() {
                path
            } else {
                emu.join(path)
            };
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

/// The folder that holds the per-title mod folders.
/// A payload folder: the module it installs is there (what every install starts from).
pub fn looks_like_payload(dir: &Path) -> bool {
    dir.join("standalone").join("subsdk9").is_file()
}

pub fn find_mods_dir(emu: &Path) -> Option<PathBuf> {
    if !emu.exists() {
        return None;
    }
    if emu.file_name().and_then(|n| n.to_str()) == Some(TITLE_ID) {
        return Some(emu.to_path_buf());
    }
    if emu.join(TITLE_ID).is_dir() {
        return Some(emu.join(TITLE_ID));
    }
    if let Some(load) = load_directory(emu) {
        return Some(load.join(TITLE_ID));
    }
    for candidate in [
        emu.join(format!("user/load/{TITLE_ID}")),
        emu.join(format!("load/{TITLE_ID}")),
        emu.join(format!("mods/contents/{TITLE_ID}")),
        emu.join(format!("mods/{TITLE_ID}")),
    ] {
        if candidate.exists() {
            return Some(candidate);
        }
    }
    if emu.join("user").exists() {
        // The folder does not exist yet: the caller creates it with the first install.
        return Some(emu.join(format!("user/load/{TITLE_ID}")));
    }
    None
}

/// The emulator program in (or a few folders above) the chosen path.
pub fn emulator_exe(emu: &Path) -> Option<PathBuf> {
    if emu.is_file() {
        return Some(emu.to_path_buf());
    }
    let mut base = Some(emu);
    for _ in 0..4 {
        let Some(dir) = base else { break };
        if dir.is_dir() {
            let mut progs: Vec<PathBuf> = fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .filter(|p| is_program(p))
                .collect();
            progs.sort();
            for name in EMU_EXES {
                if let Some(found) = progs
                    .iter()
                    .find(|p| stem(p).starts_with(name) && !skipped(p))
                {
                    return Some(found.clone());
                }
            }
            // Any single other program in the folder (a renamed build).
            let others: Vec<&PathBuf> = progs.iter().filter(|p| !skipped(p)).collect();
            if others.len() == 1 {
                return Some(others[0].clone());
            }
        }
        base = dir.parent();
    }
    None
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn skipped(path: &Path) -> bool {
    let s = stem(path);
    EMU_SKIP.iter().any(|k| s.contains(k))
}

fn is_program(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    #[cfg(windows)]
    {
        ext.eq_ignore_ascii_case("exe")
    }
    #[cfg(not(windows))]
    {
        ext.is_empty() || ext == "AppImage"
    }
}

/// The language the game is played in, so the mod's own HUD labels match the game's text.
///
/// ReLink / Longshot keep it in `language.txt` next to the game; Rekarina in `rekarina.ini`; the emulators in
/// `language_index` (yuzu family) or `system_language` (Ryujinx). `None` when it cannot be read: the caller
/// falls back to the app's own language.
pub fn detect_lang(emu: Option<&Path>, game: Option<&Path>) -> Option<Lang> {
    if let Some(game) = game {
        for path in [game.join("language.txt"), game.join("rc/language.txt")] {
            let code = read_text(&path).and_then(|t| {
                t.lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty() && !l.starts_with('#'))
                    .map(str::to_string)
            });
            if let Some(lang) = code.as_deref().and_then(Lang::from_tag) {
                return Some(lang);
            }
        }
        for name in ["rekarina.ini", "totk.ini"] {
            let Some(text) = read_text(&game.join(name)) else {
                continue;
            };
            for line in text.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                if key.trim() == "lang"
                    && let Some(lang) = Lang::from_tag(value.split('#').next().unwrap_or("").trim())
                {
                    return Some(lang);
                }
            }
            break;
        }
    }

    let mut bases: Vec<&Path> = Vec::new();
    if let Some(emu) = emu {
        // The app may be pointed at the emulator, its user folder or the title folder.
        let mut dir = Some(emu);
        for _ in 0..5 {
            let Some(d) = dir else { break };
            bases.push(d);
            dir = d.parent();
        }
    }
    for base in bases {
        for rel in ["user/config/qt-config.ini", "config/qt-config.ini"] {
            let Some(text) = read_text(&base.join(rel)) else {
                continue;
            };
            for line in text.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let key = key.trim();
                let value = value.trim();
                if key == "language_index"
                    && let Some(lang) = crate::settings::lang_from_index(value.parse().unwrap_or(0))
                {
                    return Some(lang);
                }
                if key == "Paths\\language"
                    && let Some(lang) = Lang::from_tag(value)
                {
                    return Some(lang);
                }
            }
        }
        for rel in ["portable/Config.json", "Config.json"] {
            let Some(text) = read_text(&base.join(rel)) else {
                continue;
            };
            let json = serde_json::from_str::<serde_json::Value>(&text);
            if let Some(lang) = json
                .ok()
                .as_ref()
                .and_then(|json| json.get("system_language"))
                .and_then(|v| v.as_str())
                .and_then(Lang::from_tag)
            {
                return Some(lang);
            }
        }
    }
    None
}

/// The folders this app installed (the marker file is ours), including older names.
pub fn our_mod_folders(mods: &Path) -> Vec<PathBuf> {
    folders(mods, |d| d.join(MARKER).exists())
}

/// Mods that patch the executable too: only one of them can win, and it is not always ours.
pub fn other_exefs_mods(mods: &Path, skip: &[&str]) -> Vec<String> {
    folders(mods, |d| {
        let exefs = d.join("exefs");
        exefs.is_dir()
            && fs::read_dir(&exefs)
                .into_iter()
                .flatten()
                .flatten()
                .any(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .ends_with(".ips")
                })
    })
    .into_iter()
    .filter(|p| !skip.contains(&name_of(p).as_str()))
    .map(|p| name_of(&p))
    .collect()
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn folders(mods: &Path, accept: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(mods)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && accept(p))
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("shb-rs-{}-{}", std::process::id(), tag));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn mods_dir_from_the_emulator_folder() {
        let root = temp_dir("mods");
        fs::create_dir_all(root.join("user/load")).unwrap();
        let found = find_mods_dir(&root).unwrap();
        assert_eq!(found, root.join(format!("user/load/{TITLE_ID}")));
        assert!(
            !found.exists(),
            "the folder is created by the first install"
        );

        // Pointed at the title folder itself: that is the answer.
        fs::create_dir_all(&found).unwrap();
        assert_eq!(find_mods_dir(&found).unwrap(), found);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn custom_load_directory_wins() {
        let root = temp_dir("loaddir");
        let custom = root.join("custom-mods");
        fs::create_dir_all(&custom).unwrap();
        fs::create_dir_all(root.join("user/config")).unwrap();
        fs::write(
            root.join("user/config/qt-config.ini"),
            format!("load_directory={}\n", custom.display()),
        )
        .unwrap();
        assert_eq!(find_mods_dir(&root).unwrap(), custom.join(TITLE_ID));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn language_from_the_emulator_config() {
        let root = temp_dir("lang");
        fs::create_dir_all(root.join("user/config")).unwrap();
        fs::write(root.join("user/config/qt-config.ini"), "language_index=5\n").unwrap();
        assert_eq!(detect_lang(Some(&root), None), Some(Lang::Es));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn language_from_the_game_folder() {
        let root = temp_dir("gamelang");
        fs::write(root.join("language.txt"), "# set by the player\nes-419\n").unwrap();
        assert_eq!(detect_lang(None, Some(&root)), Some(Lang::Es));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn our_folders_are_marked() {
        let mods = temp_dir("marker");
        let ours = mods.join("zz-Super-Hero-Boost");
        fs::create_dir_all(&ours).unwrap();
        fs::write(ours.join(MARKER), "{}").unwrap();
        fs::create_dir_all(mods.join("00-Other/exefs")).unwrap();
        fs::write(mods.join("00-Other/exefs/x.ips"), b"IPS32").unwrap();
        let mine = our_mod_folders(&mods);
        assert_eq!(mine, vec![ours]);
        assert_eq!(other_exefs_mods(&mods, &[]), vec!["00-Other".to_string()]);
        assert!(other_exefs_mods(&mods, &["00-Other"]).is_empty());
        let _ = fs::remove_dir_all(mods);
    }
}
