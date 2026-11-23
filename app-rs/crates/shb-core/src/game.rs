//! A statically recompiled game (Longshot, Hookshot, ReLink).
//!
//! Those ports compile the game's code ahead of time (`bin/longshot.exe` …), so an `exefs` mod is ignored: the
//! module has to be inside the game. The Python app writes the module into `<game>/.longshot_exefs/subsdk9`, patches
//! `exefs/main` with our IPS and the config block into `image.bin`, copies the romfs files to
//! `<game>/mods/zz-Super-Hero-Boost/romfs`, and then runs the builder to compile it.
//!
//! This module does the first half: everything except the build. `apply` refuses to invent a working executable,
//! and says when a rebuild is needed; `uninstall` puts the original files back.

use crate::config::{MAGIC, patch_config};
use crate::error::{Error, Result, read, write};
use crate::find::MOD_NAME;
use crate::install::copy_tree;
use crate::settings::Settings;
use std::path::{Path, PathBuf};

pub const EXEFS_NAMES: [&str; 3] = [".longshot_exefs", ".hookshot_exefs", ".relink_exefs"];
pub const EXES: [&str; 6] = [
    "bin/longshot.exe",
    "bin/longshot",
    "bin/hookshot.exe",
    "bin/hookshot",
    "bin/relink-game.exe",
    "bin/relink-game",
];
pub const BACKUP: &str = ".shb-backup";
/// The game build the payload's IPS and module are made for (the first half of `main`'s module id).
pub const MODULE_BUILD: &str = "e0f3ccfa";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Longshot,
    Hookshot,
    ReLink,
}

impl Flavor {
    pub fn name(self) -> &'static str {
        match self {
            Flavor::Longshot => "longshot",
            Flavor::Hookshot => "hookshot",
            Flavor::ReLink => "relink",
        }
    }

    /// ReLink needs its own module build (its translator mis-handles the standalone one), so the payload has a
    /// second variant for it.
    pub fn payload_variant(self) -> &'static str {
        match self {
            Flavor::ReLink => "relink",
            _ => "standalone",
        }
    }
}

/// A recompiled game folder.
#[derive(Debug, Clone)]
pub struct Game {
    pub root: PathBuf,
    pub exefs: PathBuf,
    pub flavor: Flavor,
}

/// Whether the folder looks like a Rekarina install (`rekarina.ini` + `exefs/main`, or it under `game/`).
///
/// Rekarina is a different model (it patches the module in place, updates the executable's module hash and keeps
/// its own state), so this port only reports what it found.
pub fn looks_like_rekarina(path: &Path) -> bool {
    let mut base = path.to_path_buf();
    if base.is_file() {
        base = match base.parent() {
            Some(p) => p.to_path_buf(),
            None => return false,
        };
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
    }
    candidates.push(base.join("game"));
    candidates.iter().any(|dir| {
        (dir.join("rekarina.ini").is_file() || dir.join("Rekarina.exe").is_file())
            && dir.join("exefs").join("main").is_file()
    })
}

/// The game folder (the one with an exefs folder and `bin/longshot.exe` or the like).
pub fn find(path: &Path) -> Option<Game> {
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
        .map(|n| n.eq_ignore_ascii_case("bin"))
        == Some(true)
        && let Some(parent) = base.parent()
    {
        candidates.push(parent.to_path_buf());
    }
    for root in &candidates {
        if let (Some(exefs), Some(flavor)) = (exefs_of(root), flavor_of(root))
            && EXES.iter().any(|exe| root.join(exe).is_file())
        {
            return Some(Game {
                root: root.clone(),
                exefs,
                flavor,
            });
        }
    }
    // Prepared but never run: accept it when the exefs is there.
    for root in &candidates {
        if let (Some(exefs), Some(flavor)) = (exefs_of(root), flavor_of(root)) {
            return Some(Game {
                root: root.clone(),
                exefs,
                flavor,
            });
        }
    }
    None
}

fn exefs_of(root: &Path) -> Option<PathBuf> {
    EXEFS_NAMES
        .iter()
        .map(|name| root.join(name))
        .find(|dir| dir.join("main").is_file())
}

fn flavor_of(root: &Path) -> Option<Flavor> {
    EXEFS_NAMES.iter().find_map(|name| {
        root.join(name)
            .join("main")
            .is_file()
            .then_some(match *name {
                ".hookshot_exefs" => Flavor::Hookshot,
                ".relink_exefs" => Flavor::ReLink,
                _ => Flavor::Longshot,
            })
    })
}

/// The NSO module id of `main` (hex); the builders call its first half "the build".
pub fn main_build_id(main: &Path) -> String {
    let Ok(data) = read(main) else {
        return String::new();
    };
    data.get(0x40..0x60).map(hex).unwrap_or_default()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The game holds our module (installed, whether or not it was compiled).
pub fn is_installed(game: &Game) -> bool {
    game.exefs.join("subsdk9").is_file()
}

/// Patch the module's config block and the copy in `image.bin`, and refresh the romfs files. This is what the
/// Python app's `apply_settings` does, and it needs the game to have been built with the module already (the code
/// lives in the executable).
pub fn apply_settings(game: &Game, s: &Settings, payload: &Path) -> Result<bool> {
    if !is_installed(game) {
        return Err(Error::NeedsBuild(game.root.clone()));
    }
    let bid = main_build_id(&game.exefs.join("main"));
    if !bid.is_empty() && !bid.starts_with(MODULE_BUILD) {
        return Err(Error::WrongBuild(bid));
    }
    backup_once(game)?;

    // exefs/subsdk9: the payload's module of this flavor, with the settings written into it.
    let variant = game.flavor.payload_variant();
    let source = payload.join(variant).join("subsdk9");
    if !source.is_file() {
        return Err(Error::PayloadMissing(source));
    }
    let module = read(&source)?;
    write(&game.exefs.join("subsdk9"), &patch_config(&module, s)?)?;

    // image.bin carries the same block for the compiled executable.
    let image = game.root.join("image.bin");
    let mut image_patched = false;
    if image.is_file() {
        let data = read(&image)?;
        if data.windows(MAGIC.len()).any(|w| w == MAGIC) {
            write(&image, &patch_config(&data, s)?)?;
            image_patched = true;
        }
    }

    install_romfs(game, s, payload)?;
    Ok(image_patched)
}

/// The recompilers read their mods from `<game>/mods/<name>/romfs`.
fn install_romfs(game: &Game, s: &Settings, payload: &Path) -> Result<()> {
    let mods = game.root.join("mods");
    let target = mods.join(MOD_NAME);
    std::fs::create_dir_all(&mods).map_err(|e| Error::io(&mods, e))?;
    // Older versions used other folder names: ours go away, other mods stay.
    if let Ok(entries) = std::fs::read_dir(&mods) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join(crate::find::MARKER).is_file() && path != target {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|e| Error::io(&target, e))?;
    }
    let romfs = target.join("romfs");
    std::fs::create_dir_all(&romfs).map_err(|e| Error::io(&romfs, e))?;
    write(&target.join(crate::find::MARKER), b"{}")?;

    if s.button_mapping {
        let art = payload.join("button_romfs");
        let style = match s.button_style {
            crate::settings::ButtonStyle::Switch => "switch",
            crate::settings::ButtonStyle::Xbox => "xbox",
            crate::settings::ButtonStyle::PlayStation => "playstation",
        };
        if art.join(style).is_dir() {
            copy_tree(&art.join(style), &romfs)?;
        }
        if art.join("common").is_dir() {
            copy_tree(&art.join("common"), &romfs)?;
        }
    }
    let drops: &[&str] = match (s.no_heart_drop, s.no_magic_drop) {
        (true, true) => &["no_heart_magic_drop"],
        (true, false) => &["no_heart_drop"],
        (false, true) => &["no_magic_drop"],
        (false, false) => &[],
    };
    for opt in drops {
        let src = payload.join("romfs_opts").join(opt);
        if src.is_dir() {
            copy_tree(&src, &romfs)?;
        }
    }
    for (opt, enabled) in [
        ("enemy_double", s.enemy_double),
        ("farore_outdoor", s.farore_outdoor),
        ("great_fairy", s.great_fairy),
    ] {
        let src = payload.join("romfs_opts").join(opt);
        if enabled && src.is_dir() {
            copy_tree(&src, &romfs)?;
        }
    }
    crate::enemy::install(&romfs, s, payload)?;
    if s.game_layout != crate::settings::GameLayout::None {
        // Last: the layout's resource table is merged over the mod's own.
        crate::layouts::apply(&romfs, s.game_layout.as_str(), payload)?;
    }
    Ok(())
}

/// Copy, once, every file this app writes: the uninstall puts them back.
fn backup_once(game: &Game) -> Result<()> {
    let backup = game.root.join(BACKUP);
    let files = backup.join("files");
    let manifest = backup.join("manifest.json");
    if manifest.is_file() {
        return Ok(()); // the first snapshot is the original game: never overwrite it
    }
    std::fs::create_dir_all(&files).map_err(|e| Error::io(&files, e))?;
    let exefs_name = game
        .exefs
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut entries: Vec<serde_json::Value> = Vec::new();
    for rel in [
        format!("{exefs_name}/main"),
        format!("{exefs_name}/subsdk9"),
        "image.bin".to_string(),
    ] {
        let src = game.root.join(&rel);
        let entry = if src.is_file() {
            let dst = files.join(&rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            std::fs::copy(&src, &dst).map_err(|e| Error::io(&dst, e))?;
            serde_json::json!({ "path": rel, "kind": "file" })
        } else {
            serde_json::json!({ "path": rel, "kind": "created" })
        };
        entries.push(entry);
    }
    let doc = serde_json::json!({ "version": 1, "exefs": exefs_name, "files": entries });
    write(&manifest, doc.to_string().as_bytes())?;
    Ok(())
}

/// Put the original files back and drop what the install created.
pub fn uninstall(game: &Game) -> Result<bool> {
    let backup = game.root.join(BACKUP);
    let manifest = backup.join("manifest.json");
    let mut restored = false;
    if let Ok(text) = std::fs::read_to_string(&manifest)
        && let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text)
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
            let dst = game.root.join(rel);
            match entry
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("created")
            {
                "file" => {
                    let src = backup.join("files").join(rel);
                    if src.is_file() {
                        if let Some(parent) = dst.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        std::fs::copy(&src, &dst).map_err(|e| Error::io(&dst, e))?;
                        restored = true;
                    }
                }
                _ => {
                    if dst.is_file() {
                        std::fs::remove_file(&dst).map_err(|e| Error::io(&dst, e))?;
                        restored = true;
                    }
                }
            }
        }
    }
    let mods = game.root.join("mods");
    if let Ok(entries) = std::fs::read_dir(&mods) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join(crate::find::MARKER).is_file() {
                std::fs::remove_dir_all(&path).map_err(|e| Error::io(&path, e))?;
                restored = true;
            }
        }
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("shb-rs-game-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// A Longshot-shaped game: an exefs with `main` (build id e0f3ccfa…) and the module, plus `image.bin`.
    fn game(root: &Path) {
        let exefs = root.join(".longshot_exefs");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::create_dir_all(&exefs).unwrap();
        std::fs::write(root.join("bin/longshot.exe"), b"exe").unwrap();
        let mut main = vec![0u8; 0x100];
        // The module id is raw bytes; `main_build_id` prints them as hex.
        main[0x40..0x44].copy_from_slice(&[0xE0, 0xF3, 0xCC, 0xFA]);
        std::fs::write(exefs.join("main"), &main).unwrap();
        let mut module = vec![0x22u8; 0x400];
        module[0x100..0x108].copy_from_slice(MAGIC);
        module[0x108..0x10C].copy_from_slice(&1u32.to_le_bytes());
        module[0x10C..0x110].copy_from_slice(&0x61u32.to_le_bytes());
        std::fs::write(exefs.join("subsdk9"), &module).unwrap();
        std::fs::write(root.join("image.bin"), &module).unwrap();
    }

    fn payload(root: &Path) {
        for variant in ["standalone", "relink"] {
            let dir = root.join(variant);
            std::fs::create_dir_all(&dir).unwrap();
            let mut module = vec![0x33u8; 0x400];
            module[0x100..0x108].copy_from_slice(MAGIC);
            module[0x108..0x10C].copy_from_slice(&1u32.to_le_bytes());
            module[0x10C..0x110].copy_from_slice(&0x61u32.to_le_bytes());
            std::fs::write(dir.join("subsdk9"), &module).unwrap();
            std::fs::write(dir.join("ours.ips"), b"IPS32EEOF").unwrap();
        }
        let art = root.join("button_romfs/switch/Layouts");
        std::fs::create_dir_all(&art).unwrap();
        std::fs::write(art.join("a.bntx"), b"x").unwrap();
    }

    #[test]
    fn detection() {
        let root = temp_dir("find");
        game(&root);
        let found = find(&root).expect("the game folder");
        assert_eq!(found.flavor, Flavor::Longshot);
        assert!(main_build_id(&found.exefs.join("main")).starts_with(MODULE_BUILD));
        // The `bin` folder is accepted too.
        assert_eq!(find(&root.join("bin")).map(|g| g.root), Some(root.clone()));
        assert!(find(&root.join("nowhere")).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn apply_writes_the_module_image_and_romfs_then_uninstall_restores() {
        let root = temp_dir("apply");
        game(&root);
        let pay = root.join("payload");
        payload(&pay);
        let found = find(&root).unwrap();
        let original = std::fs::read(root.join(".longshot_exefs/subsdk9")).unwrap();

        let mut s = Settings {
            enemy_hp: 200,
            ..Settings::default()
        };
        s.button_mapping = true;
        apply_settings(&found, &s, &pay).unwrap();

        let module = std::fs::read(root.join(".longshot_exefs/subsdk9")).unwrap();
        let cfg = crate::config::read_config(&module).unwrap();
        assert_eq!(cfg.enemy_hp, 200);
        let image = std::fs::read(root.join("image.bin")).unwrap();
        assert_eq!(crate::config::read_config(&image).unwrap().enemy_hp, 200);
        assert!(
            root.join("mods/zz-Super-Hero-Boost/romfs/Layouts/a.bntx")
                .is_file()
        );
        assert!(root.join(BACKUP).join("manifest.json").is_file());

        assert!(uninstall(&found).unwrap());
        assert_eq!(
            std::fs::read(root.join(".longshot_exefs/subsdk9")).unwrap(),
            original
        );
        assert!(!root.join("mods/zz-Super-Hero-Boost").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_game_without_the_module_needs_a_build() {
        let root = temp_dir("nobuild");
        game(&root);
        std::fs::remove_file(root.join(".longshot_exefs/subsdk9")).unwrap();
        let pay = root.join("payload");
        payload(&pay);
        let found = find(&root).unwrap();
        let err = apply_settings(&found, &Settings::default(), &pay).unwrap_err();
        assert!(matches!(err, Error::NeedsBuild(_)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_different_game_build_is_refused() {
        let root = temp_dir("build");
        game(&root);
        let mut main = std::fs::read(root.join(".longshot_exefs/main")).unwrap();
        main[0x40..0x44].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        std::fs::write(root.join(".longshot_exefs/main"), &main).unwrap();
        let pay = root.join("payload");
        payload(&pay);
        let err = apply_settings(&find(&root).unwrap(), &Settings::default(), &pay).unwrap_err();
        assert!(matches!(err, Error::WrongBuild(_)));
        let _ = std::fs::remove_dir_all(root);
    }
}
