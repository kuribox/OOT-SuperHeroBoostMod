//! A Rekarina install (`app-legacy/rekarina.py`): a static recompilation that keeps the game's ExeFS modules and runs the
//! code it lifted to C.
//!
//! Rekarina rejects code mods ("the game has no loader for it"), so the module is not installed into an `exefs` mod
//! folder: the app has to lift and compile it with Rekarina's own tools. What this port does is the settings half,
//! exactly like the recompiled games: write the config block into `exefs/subsdk9`, patch the SHA-256 that
//! `Rekarina.exe` stores for that module, refresh the romfs files, and keep our mod first in `mods_order`. The lift
//! and the compile are Rekarina's, and are not started here.
//!
//! The state lives in `.shb-rekarina/`: `state.json` (what was built and from which files) and `backup/` (the
//! originals, put back by `uninstall`).

use crate::config::{MAGIC, patch_config, read_config};
use crate::error::{Error, Result, read, write};
use crate::find::MOD_NAME;
use crate::install::BUILD_ID;
use crate::settings::Settings;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const EXE_NAME: &str = "Rekarina.exe";
pub const INI: &str = "rekarina.ini";
pub const MODULE: &str = "subsdk9";
pub const WORK: &str = ".shb-rekarina";
pub const BACKUP: &str = ".shb-rekarina/backup";
/// The game version the module is built for.
pub const GAME: &str = "1.1.1";
pub const MODULE_BUILD: &str = "e0f3ccfa";

/// A Rekarina game folder: `Rekarina.exe`, `rekarina.ini` and `exefs/main`.
#[derive(Debug, Clone)]
pub struct Rekarina {
    pub root: PathBuf,
}

fn sha256(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The game folder, from the install root or its `game` subfolder.
pub fn find(path: &Path) -> Option<Rekarina> {
    if !path.exists() {
        return None;
    }
    let mut base = path.to_path_buf();
    if base.is_file() {
        base = base.parent()?.to_path_buf();
    }
    let mut candidates = vec![base.clone()];
    if base
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("game"))
        == Some(true)
        && let Some(parent) = base.parent()
    {
        candidates.push(parent.to_path_buf());
    } else {
        candidates.push(base.join("game"));
    }
    for candidate in &candidates {
        if candidate.join(INI).is_file() && candidate.join("exefs").join("main").is_file() {
            return Some(Rekarina {
                root: candidate.clone(),
            });
        }
    }
    None
}

fn state_path(game: &Rekarina) -> PathBuf {
    game.root.join(WORK).join("state.json")
}

/// What the last build wrote: enough to know whether the module in the exe is ours.
pub fn state(game: &Rekarina) -> Value {
    std::fs::read_to_string(state_path(game))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null)
}

fn write_state(
    game: &Rekarina,
    exe_sha: &str,
    module_sha: &str,
    rebuilt: bool,
    ips_sha: Option<&str>,
) -> Result<()> {
    let mut doc = state(game);
    if !doc.is_object() {
        doc = serde_json::json!({});
    }
    let object = doc.as_object_mut().expect("an object");
    object.insert("built".into(), Value::Bool(true));
    object.insert("game".into(), Value::String(GAME.into()));
    object.insert("exe_sha256".into(), Value::String(exe_sha.into()));
    object.insert("module_sha256".into(), Value::String(module_sha.into()));
    object.insert("rebuilt".into(), Value::Bool(rebuilt));
    if let Some(ips) = ips_sha {
        object.insert("ips_sha256".into(), Value::String(ips.into()));
    }
    let path = state_path(game);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    write(&path, doc.to_string().as_bytes())
}

fn exe_path(game: &Rekarina) -> PathBuf {
    game.root.join(EXE_NAME)
}

fn module_path(game: &Rekarina) -> PathBuf {
    game.root.join("exefs").join(MODULE)
}

/// The game was built with our module: the exe's hash and the module's match the state, and the exe knows that
/// module's SHA-256 (a Rekarina rebuild would not).
pub fn is_built(game: &Rekarina) -> bool {
    let exe = exe_path(game);
    let module = module_path(game);
    if !exe.is_file() || !module.is_file() {
        return false;
    }
    let state = state(game);
    let Some(want_exe) = state.get("exe_sha256").and_then(|v| v.as_str()) else {
        return false;
    };
    let Some(want_module) = state.get("module_sha256").and_then(|v| v.as_str()) else {
        return false;
    };
    if state.get("built").and_then(|v| v.as_bool()) != Some(true) {
        return false;
    }
    let Ok(exe_data) = read(&exe) else {
        return false;
    };
    let Ok(module_data) = read(&module) else {
        return false;
    };
    sha256(&exe_data) == want_exe
        && sha256(&module_data) == want_module
        && exe_data
            .windows(want_module.len())
            .any(|w| w == want_module.as_bytes())
}

/// Patch the expected module hash inside the executable: the file is copied, patched and swapped in.
fn patch_exe_hash(game: &Rekarina, old: &str, new: &str) -> Result<bool> {
    let exe = exe_path(game);
    let data = read(&exe)?;
    let old_bytes = old.as_bytes();
    let new_bytes = new.as_bytes();
    if old_bytes.len() != new_bytes.len() {
        return Err(Error::Rekarina(
            "the module hashes have different lengths".into(),
        ));
    }
    let mut patched = data.clone();
    let mut found = 0;
    let mut at = 0;
    while let Some(hit) = data[at..]
        .windows(old_bytes.len())
        .position(|w| w == old_bytes)
    {
        let start = at + hit;
        patched[start..start + new_bytes.len()].copy_from_slice(new_bytes);
        found += 1;
        at = start + new_bytes.len();
    }
    if found == 0 {
        return Ok(false);
    }
    let temp = exe.with_extension("exe.shb-tmp");
    write(&temp, &patched)?;
    std::fs::rename(&temp, &exe).map_err(|e| Error::io(&exe, e))?;
    Ok(true)
}

/// Put our mod first in `mods_order` and take it out of `mods_disabled`, without touching the player's settings.
pub fn set_mod_order(game: &Rekarina) -> Result<()> {
    let ini = game.root.join(INI);
    let Ok(text) = std::fs::read_to_string(&ini) else {
        return Ok(());
    };
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut have = false;
    for line in lines.iter_mut() {
        let low = line.trim().to_ascii_lowercase();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let names: Vec<&str> = value
            .split('|')
            .map(str::trim)
            .filter(|n| !n.is_empty() && *n != MOD_NAME)
            .collect();
        if low.starts_with("mods_order") {
            *line = format!(
                "{} = {}",
                key.trim_end(),
                std::iter::once(MOD_NAME)
                    .chain(names)
                    .collect::<Vec<_>>()
                    .join("|")
            );
            have = true;
        } else if low.starts_with("mods_disabled") {
            *line = format!("{} = {}", key.trim_end(), names.join("|"));
        }
    }
    if !have {
        lines.push(String::new());
        lines.push("[mods]".to_string());
        lines.push(format!("mods_order = {MOD_NAME}"));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    write(&ini, out.as_bytes())
}

/// `build-cache/mods-built.txt`: the code files the build took, as `<path under mods/>\t<sha256>`.
fn write_built(game: &Rekarina) -> Result<()> {
    let mods = game.root.join("mods");
    let mut rows: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&mods) {
        let mut dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            for rel in ["exefs/subsdk9", "exefs/main"] {
                let file = dir.join(rel);
                if let Ok(data) = read(&file) {
                    let name = format!(
                        "{}/{}",
                        dir.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        rel
                    );
                    rows.push(format!("{name}\t{}", sha256(&data)));
                }
            }
        }
    }
    let path = game.root.join("build-cache").join("mods-built.txt");
    if rows.is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    write(&path, format!("{}\n", rows.join("\n")).as_bytes())
}

/// Snapshot, once, every file the install can change.
fn backup_once(game: &Rekarina) -> Result<()> {
    let backup = game.root.join(BACKUP);
    let manifest = backup.join("manifest.json");
    if manifest.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(backup.join("files")).map_err(|e| Error::io(&backup, e))?;
    let mut entries: Vec<Value> = Vec::new();
    for rel in [
        EXE_NAME.to_string(),
        format!("exefs/{MODULE}"),
        INI.to_string(),
        format!("mods/{MOD_NAME}"),
    ] {
        let src = game.root.join(&rel);
        let entry = if src.is_file() {
            let dst = backup.join("files").join(&rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            std::fs::copy(&src, &dst).map_err(|e| Error::io(&dst, e))?;
            serde_json::json!({ "path": rel, "kind": "file" })
        } else if src.is_dir() {
            let dst = backup.join("files").join(&rel);
            if dst.exists() {
                let _ = std::fs::remove_dir_all(&dst);
            }
            crate::install::copy_tree(&src, &dst)?;
            serde_json::json!({ "path": rel, "kind": "dir" })
        } else {
            serde_json::json!({ "path": rel, "kind": "created" })
        };
        entries.push(entry);
    }
    let doc = serde_json::json!({ "version": 1, "kind": "rekarina", "files": entries });
    write(&manifest, doc.to_string().as_bytes())
}

/// Restore the originals (`rekarina.ini` keeps the player's settings: only our line is removed).
pub fn uninstall(game: &Rekarina) -> Result<bool> {
    let backup = game.root.join(BACKUP);
    let mut restored = false;
    if let Ok(text) = std::fs::read_to_string(backup.join("manifest.json"))
        && let Ok(doc) = serde_json::from_str::<Value>(&text)
    {
        for entry in doc
            .get("files")
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default()
        {
            let Some(rel) = entry.get("path").and_then(|p| p.as_str()) else {
                continue;
            };
            if rel == INI {
                continue;
            }
            let dst = game.root.join(rel);
            let src = backup.join("files").join(rel);
            match entry
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("created")
            {
                "file" if src.is_file() => {
                    if let Some(parent) = dst.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::copy(&src, &dst).map_err(|e| Error::io(&dst, e))?;
                    restored = true;
                }
                "dir" if src.is_dir() => {
                    if dst.exists() {
                        let _ = std::fs::remove_dir_all(&dst);
                    }
                    crate::install::copy_tree(&src, &dst)?;
                    restored = true;
                }
                _ => {
                    if dst.is_dir() {
                        let _ = std::fs::remove_dir_all(&dst);
                        restored = true;
                    } else if dst.is_file() {
                        std::fs::remove_file(&dst).map_err(|e| Error::io(&dst, e))?;
                        restored = true;
                    }
                }
            }
        }
    }
    // Our line in mods_order goes away; the rest of the file is the player's.
    if let Ok(text) = std::fs::read_to_string(game.root.join(INI)) {
        let lines: Vec<String> = text
            .lines()
            .map(|line| {
                let low = line.trim().to_ascii_lowercase();
                let Some((key, value)) = line.split_once('=') else {
                    return line.to_string();
                };
                if !(low.starts_with("mods_order") || low.starts_with("mods_disabled")) {
                    return line.to_string();
                }
                let names: Vec<&str> = value
                    .split('|')
                    .map(str::trim)
                    .filter(|n| !n.is_empty() && *n != MOD_NAME)
                    .collect();
                format!("{} = {}", key.trim_end(), names.join("|"))
            })
            .collect();
        let mut out = lines.join("\n");
        out.push('\n');
        let _ = write(&game.root.join(INI), out.as_bytes());
    }
    Ok(restored)
}

/// What a build needs in the game folder before it starts: the backup to come back to, the mod's own folder (its
/// ExeFS patch file is what Rekarina's converter turns into the hooks) and the first place in `mods_order`.
pub fn stage(game: &Rekarina, s: &Settings, payload: &Path) -> Result<()> {
    backup_once(game)?;
    install_romfs(game, s, payload)?;
    set_mod_order(game)
}

/// The settings half of an install: the module's config block, the exe's expected hash, the romfs and the mod order.
pub fn apply_settings(game: &Rekarina, s: &Settings, payload: &Path) -> Result<()> {
    if !is_built(game) {
        return Err(Error::Rekarina(
            "this Rekarina game was not built with the mod yet: that lift and compile is Rekarina's, not the app's"
                .into(),
        ));
    }
    let module = module_path(game);
    let old_data = read(&module)?;
    let old_sha = sha256(&old_data);
    let patched = patch_config(&old_data, s)?;
    let new_sha = sha256(&patched);

    backup_once(game)?;
    if new_sha != old_sha {
        if !patch_exe_hash(game, &old_sha, &new_sha)? {
            return Err(Error::Rekarina(
                "the executable does not carry the module's hash: Rekarina has to rebuild the game"
                    .into(),
            ));
        }
        write(&module, &patched)?;
        let exe_sha = sha256(&read(&exe_path(game))?);
        let ips = crate::ips::read(&read(&payload.join("rekarina").join("ours.ips"))?)?;
        let _ = ips;
        write_state(game, &exe_sha, &new_sha, false, None)?;
    }

    install_romfs(game, s, payload)?;
    set_mod_order(game)?;
    write_built(game)?;
    Ok(())
}

/// Take a freshly built executable as this install: copy it over `Rekarina.exe`, write the settings into the module,
/// put the module's new hash inside the executable, and record what was built.
///
/// The order matters: the build's expected-hash table was made from the *raw* module, so the settings change its hash
/// and the executable has to learn the new one as it is swapped in — exactly what the Python install does.
pub fn adopt_build(game: &Rekarina, s: &Settings, payload: &Path, built: &Path) -> Result<()> {
    let source = payload.join("rekarina").join(MODULE);
    let raw = read(&source)?;
    let patched = crate::config::patch_config(&raw, s)?;
    let old_sha = sha256(&raw);
    let new_sha = sha256(&patched);
    let dest = exe_path(game);
    let data = read(built)?;
    let mut data = data;
    if new_sha != old_sha {
        if old_sha.len() != new_sha.len() {
            return Err(Error::Rekarina(
                "the module hashes have different lengths".into(),
            ));
        }
        let mut found = 0;
        let mut at = 0;
        while let Some(hit) = data[at..]
            .windows(old_sha.len())
            .position(|w| w == old_sha.as_bytes())
        {
            let start = at + hit;
            data[start..start + new_sha.len()].copy_from_slice(new_sha.as_bytes());
            found += 1;
            at = start + new_sha.len();
        }
        if found == 0 {
            return Err(Error::Rekarina(
                "the built executable does not carry the module's hash: it cannot be used".into(),
            ));
        }
    }
    let temp = dest.with_extension("exe.shb-tmp");
    write(&temp, &data)?;
    std::fs::rename(&temp, &dest).map_err(|e| Error::io(&dest, e))?;
    write(&module_path(game), &patched)?;
    install_romfs(game, s, payload)?;
    set_mod_order(game)?;
    write_built(game)?;
    write_state(game, &sha256(&data), &new_sha, true, None)
}

/// What opens a Rekarina game: its own executable, under either spelling.
pub fn launcher_of(game: &Rekarina) -> Option<PathBuf> {
    [EXE_NAME, "rekarina"]
        .iter()
        .map(|name| game.root.join(name))
        .find(|path| path.is_file())
}

/// Say that the game was rebuilt with the module (the state's `rebuilt` flag), from what is on disk now.
pub fn mark_rebuilt(game: &Rekarina) -> Result<()> {
    let exe_sha = sha256(&read(&exe_path(game))?);
    let module_sha = sha256(&read(&module_path(game))?);
    write_state(game, &exe_sha, &module_sha, true, None)
}

/// The romfs files, plus the two things Rekarina needs beside them: the ExeFS patch set it switches its generated
/// code on with, and a marker saying which mod folder this is.
fn install_romfs(game: &Rekarina, s: &Settings, payload: &Path) -> Result<()> {
    let mods = game.root.join("mods");
    let target = mods.join(MOD_NAME);
    std::fs::create_dir_all(&mods).map_err(|e| Error::io(&mods, e))?;
    for old in crate::find::our_mod_folders(&mods) {
        if old.file_name().map(|n| n.to_string_lossy() != MOD_NAME) == Some(true) {
            let _ = std::fs::remove_dir_all(&old);
        }
    }
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|e| Error::io(&target, e))?;
    }
    crate::romfs::populate(
        &target.join("romfs"),
        s,
        payload,
        &game.root,
        crate::romfs::Variant::Rekarina,
    )?;
    let exefs = target.join("exefs");
    std::fs::create_dir_all(&exefs).map_err(|e| Error::io(&exefs, e))?;
    let ips = payload.join("rekarina").join("ours.ips");
    if !ips.is_file() {
        return Err(Error::PayloadMissing(ips));
    }
    std::fs::copy(&ips, exefs.join(format!("{BUILD_ID}.ips"))).map_err(|e| Error::io(&exefs, e))?;
    // The marker: the variant plus every setting, so the app can tell this folder is ours and was installed here.
    let mut doc = serde_json::json!({ "variant": "rekarina", "module": MODULE });
    if let Ok(serde_json::Value::Object(fields)) = serde_json::to_value(s) {
        for (key, value) in fields {
            doc[key] = value;
        }
    }
    write(
        &target.join(crate::find::MARKER),
        doc.to_string().as_bytes(),
    )
}

/// What the module currently says, for the CLI's `show`.
pub fn installed_config(game: &Rekarina) -> Result<crate::config::InstalledConfig> {
    let data = read(&module_path(game))?;
    if !data.windows(MAGIC.len()).any(|w| w == MAGIC) {
        return Err(Error::Rekarina("the module has no config block".into()));
    }
    read_config(&data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("shb-rs-rekarina-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// A game as Rekarina leaves it after a build: the module with its config block, an exe that stores the module's
    /// SHA-256, and the state file.
    fn game(root: &Path) {
        std::fs::create_dir_all(root.join("exefs")).unwrap();
        let mut module = vec![0x44u8; 0x400];
        module[0x100..0x108].copy_from_slice(MAGIC);
        module[0x108..0x10C].copy_from_slice(&1u32.to_le_bytes());
        module[0x10C..0x110].copy_from_slice(&0x61u32.to_le_bytes());
        std::fs::write(root.join("exefs/subsdk9"), &module).unwrap();
        std::fs::write(root.join("exefs/main"), vec![0u8; 0x100]).unwrap();
        let module_sha = sha256(&module);
        let mut exe = b"REKARINA".to_vec();
        exe.extend_from_slice(module_sha.as_bytes());
        exe.extend_from_slice(&[0u8; 64]);
        std::fs::write(root.join(EXE_NAME), &exe).unwrap();
        std::fs::write(root.join(INI), "[general]\nlang = es-419\n").unwrap();
        let exe_sha = sha256(&exe);
        std::fs::create_dir_all(root.join(WORK)).unwrap();
        std::fs::write(
            root.join(WORK).join("state.json"),
            serde_json::json!({
                "built": true, "game": GAME, "exe_sha256": exe_sha, "module_sha256": module_sha, "rebuilt": true
            })
            .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn detection_and_built_state() {
        let root = temp_dir("built");
        game(&root);
        let found = find(&root).expect("the game folder");
        assert!(is_built(&found));
        // A changed module invalidates the state.
        let mut module = std::fs::read(root.join("exefs/subsdk9")).unwrap();
        module[0x200] = 0x99;
        std::fs::write(root.join("exefs/subsdk9"), &module).unwrap();
        assert!(!is_built(&found));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn apply_patches_the_module_the_exe_hash_and_the_mod_order() {
        let root = temp_dir("apply");
        game(&root);
        let payload = root.join("payload");
        std::fs::create_dir_all(payload.join("rekarina")).unwrap();
        std::fs::write(payload.join("rekarina/ours.ips"), b"IPS32EEOF").unwrap();
        let art = payload.join("button_romfs/switch/Layouts");
        std::fs::create_dir_all(&art).unwrap();
        std::fs::write(art.join("a.bntx"), b"x").unwrap();

        let found = find(&root).unwrap();
        let s = Settings {
            enemy_hp: 250,
            ..Settings::default()
        };
        apply_settings(&found, &s, &payload).unwrap();

        // The module carries the settings and the exe knows its new hash again.
        assert_eq!(installed_config(&found).unwrap().enemy_hp, 250);
        assert!(is_built(&found), "the exe was re-hashed");
        let ini = std::fs::read_to_string(root.join(INI)).unwrap();
        assert!(ini.contains(&format!("mods_order = {MOD_NAME}")), "{ini}");
        assert!(
            root.join("mods")
                .join(MOD_NAME)
                .join("romfs/Layouts/a.bntx")
                .is_file()
        );

        // Uninstall puts the module and the exe back and takes our line out.
        assert!(uninstall(&found).unwrap());
        let ini = std::fs::read_to_string(root.join(INI)).unwrap();
        assert!(!ini.contains(MOD_NAME), "{ini}");
        assert!(!root.join("mods").join(MOD_NAME).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_game_without_the_build_is_refused() {
        let root = temp_dir("notbuilt");
        game(&root);
        std::fs::remove_file(root.join(WORK).join("state.json")).unwrap();
        let found = find(&root).unwrap();
        let err = apply_settings(&found, &Settings::default(), &root).unwrap_err();
        assert!(matches!(err, Error::Rekarina(_)));
        let _ = std::fs::remove_dir_all(root);
    }
}
