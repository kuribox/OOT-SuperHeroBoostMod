//! The install itself: patch the prebuilt module, copy the button art and the optional romfs files into the
//! emulator's mod folder.
//!
//! No compiler involved. The folder is named `zz-Super-Hero-Boost` on purpose: the loaders let the last mod win
//! each file, and ours must win while a costume pack still keeps its own files.

use crate::config::patch_config;
use crate::error::{Error, Result, read, write};
use crate::find::{self, MARKER, MOD_NAME, TITLE_ID};
use crate::settings::{BmLayout, ButtonStyle, GameLayout, Settings};
use std::fs;
use std::path::{Path, PathBuf};

/// Build id the IPS patch of the supported update applies to.
pub const BUILD_ID: &str = "E0F3CCFAF50065AE4EA01EB5AEB13B1A45DA3348";

#[derive(Debug, Clone)]
pub struct InstallRequest {
    /// the emulator folder (or its `user` / title folder)
    pub emu: PathBuf,
    pub settings: Settings,
    pub name: String,
    /// `app/payload` of the repository, or the payload next to the program
    pub payload: PathBuf,
}

impl InstallRequest {
    pub fn new(emu: impl Into<PathBuf>, settings: Settings, payload: impl Into<PathBuf>) -> Self {
        InstallRequest {
            emu: emu.into(),
            settings,
            name: MOD_NAME.to_string(),
            payload: payload.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallResult {
    pub target: PathBuf,
    pub variant: String,
    /// Things the user should know, already phrased (conflicts, optional files that were not built...).
    pub notes: Vec<String>,
}

/// The button art folder of a style.
fn style_dir(style: ButtonStyle) -> &'static str {
    match style {
        ButtonStyle::Switch => "switch",
        ButtonStyle::Xbox => "xbox",
        ButtonStyle::PlayStation => "playstation",
    }
}

pub fn install(req: &InstallRequest) -> Result<InstallResult> {
    let s = &req.settings;
    let mods = find::find_mods_dir(&req.emu).ok_or_else(|| Error::NoModsDir(req.emu.clone()))?;
    let pay = req.payload.join("standalone");
    if !pay.join("subsdk9").is_file() {
        return Err(Error::PayloadMissing(pay.join("subsdk9")));
    }

    fs::create_dir_all(&mods).map_err(|e| Error::io(&mods, e))?;
    // Older versions used other folder names (00-Super-Hero-Boost...): ours go away, other mods stay.
    for old in find::our_mod_folders(&mods) {
        if old.file_name().and_then(|n| n.to_str()) != Some(req.name.as_str()) {
            fs::remove_dir_all(&old).map_err(|e| Error::io(&old, e))?;
        }
    }

    let target = mods.join(&req.name);
    if target.exists() {
        if !target.join(MARKER).exists() {
            return Err(Error::NotOurs(target));
        }
        fs::remove_dir_all(&target).map_err(|e| Error::io(&target, e))?;
    }
    let exefs = target.join("exefs");
    fs::create_dir_all(&exefs).map_err(|e| Error::io(&exefs, e))?;
    write(&target.join(MARKER), marker_json(req).as_bytes())?;

    let module = read(&pay.join("subsdk9"))?;
    write(&exefs.join("subsdk9"), &patch_config(&module, s)?)?;
    write(
        &exefs.join(format!("{BUILD_ID}.ips")),
        &read(&pay.join("ours.ips"))?,
    )?;

    let mut notes: Vec<String> = Vec::new();
    let romfs = target.join("romfs");

    // buttonMapping: the button textures of the chosen style plus the shared UI budget.
    if s.button_mapping {
        let art = req.payload.join("button_romfs");
        let plat = style_dir(s.button_style);
        if !art.join(plat).is_dir() {
            return Err(Error::PayloadMissing(art.join(plat)));
        }
        copy_tree(&art.join(plat), &romfs)?;
        if art.join("common").is_dir() {
            copy_tree(&art.join("common"), &romfs)?;
        }
    }

    // The drops are two changes to the same game table: when both are on, one combined file is installed.
    let drops: Vec<&str> = match (s.no_heart_drop, s.no_magic_drop) {
        (true, true) => vec!["no_heart_magic_drop"],
        (true, false) => vec!["no_heart_drop"],
        (false, true) => vec!["no_magic_drop"],
        (false, false) => vec![],
    };
    for opt in drops {
        let src = req.payload.join("romfs_opts").join(opt);
        if src.is_dir() {
            copy_tree(&src, &romfs)?;
        } else {
            // The file is generated from the user's own dump: never block the install because of it.
            let key = if opt == "no_heart_magic_drop" {
                "no heart drops and no magic drops".to_string()
            } else {
                opt.replace('_', " ")
            };
            notes.push(format!("{key}: not built (needs the game dump), skipped"));
        }
    }

    for (opt, enabled) in [
        ("enemy_double", s.enemy_double),
        ("farore_outdoor", s.farore_outdoor),
        ("great_fairy", s.great_fairy),
    ] {
        if !enabled {
            continue;
        }
        let src = req.payload.join("romfs_opts").join(opt);
        if src.is_dir() {
            copy_tree(&src, &romfs)?;
        } else {
            notes.push(format!("{}: not built, skipped", opt.replace('_', " ")));
        }
    }

    // Parts of the Python app that this port does not carry yet: say it instead of installing half of it.
    if s.enemy_move != 0
        || s.enemy_anim != 0
        || s.enemy_attack != 0
        || s.enemy_proj != 0
        || s.enemy_alert != 0
        || s.enemy_aggro != 0
        || s.boss_move != 0
        || s.boss_anim != 0
    {
        notes.push(
            "enemy speed / aggression: not ported to the Rust app yet, the packs were not installed"
                .to_string(),
        );
    }
    if s.bm_layout == BmLayout::Gamecube {
        notes.push(
            "GameCube layout: the patched UI diagram is not ported to the Rust app yet; the HUD and the button art were installed"
                .to_string(),
        );
    }
    if s.game_layout != GameLayout::None {
        notes.push(
            "game-wide prompts: the download is not ported to the Rust app yet, only the mod's own art was installed"
                .to_string(),
        );
    }

    // Conflicts: other mods that patch the executable, and the ones that fight over the button font.
    let skip = [req.name.as_str()];
    let conflicts = find::other_exefs_mods(&mods, &skip);
    if !conflicts.is_empty() {
        notes.push(format!(
            "other mods patch the executable too: {}",
            conflicts.join(", ")
        ));
    }
    Ok(InstallResult {
        target,
        variant: "standalone".to_string(),
        notes,
    })
}

/// The marker file: what we installed and with which settings, so the app can tell its own folders apart.
fn marker_json(req: &InstallRequest) -> String {
    let mut value = serde_json::to_value(&req.settings).unwrap_or_default();
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "variant".to_string(),
            serde_json::Value::String("standalone".to_string()),
        );
        obj.insert(
            "name".to_string(),
            serde_json::Value::String(req.name.clone()),
        );
        obj.insert(
            "payload".to_string(),
            serde_json::Value::String("standalone".to_string()),
        );
    }
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// Remove every folder this app installed.
pub fn uninstall(emu: &Path) -> Result<bool> {
    let mods = find::find_mods_dir(emu).ok_or_else(|| Error::NoModsDir(emu.to_path_buf()))?;
    let mut gone = false;
    for folder in find::our_mod_folders(&mods) {
        fs::remove_dir_all(&folder).map_err(|e| Error::io(&folder, e))?;
        gone = true;
    }
    Ok(gone)
}

/// Copy a directory tree over another one (files are replaced, extra files are kept).
pub fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).map_err(|e| Error::io(dst, e))?;
    let entries = fs::read_dir(src).map_err(|e| Error::io(src, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::io(src, e))?;
        let path = entry.path();
        let to = dst.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &to)?;
        } else {
            fs::copy(&path, &to).map_err(|e| Error::io(&to, e))?;
        }
    }
    Ok(())
}

/// What a reader of the emulator folder needs to know before installing into it.
#[derive(Debug, Clone)]
pub struct EmulatorInfo {
    pub mods: Option<PathBuf>,
    pub exe: Option<PathBuf>,
    pub installed: Option<PathBuf>,
}

pub fn inspect(emu: &Path) -> EmulatorInfo {
    let mods = find::find_mods_dir(emu);
    let installed = mods.as_ref().and_then(|m| {
        find::our_mod_folders(m)
            .into_iter()
            .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(MOD_NAME))
    });
    EmulatorInfo {
        mods,
        exe: find::emulator_exe(emu),
        installed,
    }
}

/// The title id of the game, for messages.
pub fn title_id() -> &'static str {
    TITLE_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("shb-rs-install-{}-{}", std::process::id(), tag));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// A payload with a fake module (a real config block) and one file per optional folder.
    fn payload(root: &Path, block_size: usize) {
        let standalone = root.join("standalone");
        fs::create_dir_all(&standalone).unwrap();
        let mut module = vec![0x11u8; 0x800];
        module[0x400..0x408].copy_from_slice(crate::config::MAGIC);
        module[0x408..0x40C].copy_from_slice(&1u32.to_le_bytes());
        module[0x40C..0x410].copy_from_slice(&(block_size as u32).to_le_bytes());
        fs::write(standalone.join("subsdk9"), &module).unwrap();
        fs::write(standalone.join("ours.ips"), b"IPS32EEOF").unwrap();
        for (dir, rel) in [
            ("button_romfs/switch", "Layouts/a.bntx"),
            ("button_romfs/common", "Fonts/f.bfarc"),
            ("romfs_opts/no_heart_drop", "GameData/x.bin"),
        ] {
            let p = root.join(dir).join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"x").unwrap();
        }
    }

    #[test]
    fn install_writes_the_mod_folder() {
        let root = temp_dir("ok");
        let emu = root.join("emu");
        fs::create_dir_all(emu.join("user")).unwrap();
        let pay = root.join("payload");
        payload(&pay, 0x61);

        let settings = Settings {
            no_heart_drop: true,
            hud_ctrl_scale: 110,
            ..Settings::default()
        };
        let req = InstallRequest::new(&emu, settings.clone(), &pay);
        let res = install(&req).unwrap();

        let mods = emu.join(format!("user/load/{TITLE_ID}"));
        assert_eq!(res.target, mods.join(MOD_NAME));
        assert_eq!(res.variant, "standalone");
        assert!(res.target.join(MARKER).is_file());
        assert!(res.target.join("exefs/subsdk9").is_file());
        assert!(res.target.join(format!("exefs/{BUILD_ID}.ips")).is_file());
        assert!(
            res.target.join("romfs/Layouts/a.bntx").is_file(),
            "button art"
        );
        assert!(
            res.target.join("romfs/Fonts/f.bfarc").is_file(),
            "shared UI budget"
        );
        assert!(
            res.target.join("romfs/GameData/x.bin").is_file(),
            "no heart drops"
        );

        // The installed module carries the settings.
        let module = fs::read(res.target.join("exefs/subsdk9")).unwrap();
        let cfg = crate::config::read_config(&module).unwrap();
        assert_eq!(cfg.enemy_hp, settings.enemy_hp);
        let block = module
            .windows(8)
            .position(|w| w == crate::config::MAGIC)
            .unwrap();
        assert_eq!(module[block + crate::config::off::HUD_CTRL_SCALE], 110);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_second_install_replaces_ours_and_uninstall_removes_it() {
        let root = temp_dir("twice");
        let emu = root.join("emu");
        fs::create_dir_all(emu.join("user")).unwrap();
        let pay = root.join("payload");
        payload(&pay, 0x61);
        let req = InstallRequest::new(&emu, Settings::default(), &pay);
        install(&req).unwrap();
        install(&req).unwrap();
        assert!(uninstall(&emu).unwrap());
        assert!(!uninstall(&emu).unwrap(), "nothing left to remove");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_foreign_folder_with_our_name_is_not_deleted() {
        let root = temp_dir("foreign");
        let emu = root.join("emu");
        let mods = emu.join(format!("user/load/{TITLE_ID}"));
        fs::create_dir_all(mods.join(MOD_NAME)).unwrap();
        let pay = root.join("payload");
        payload(&pay, 0x61);
        let err = install(&InstallRequest::new(&emu, Settings::default(), &pay)).unwrap_err();
        assert!(matches!(err, Error::NotOurs(_)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_payload_is_reported() {
        let root = temp_dir("nopayload");
        let emu = root.join("emu");
        fs::create_dir_all(emu.join("user")).unwrap();
        let err = install(&InstallRequest::new(
            &emu,
            Settings::default(),
            root.join("payload"),
        ))
        .unwrap_err();
        assert!(matches!(err, Error::PayloadMissing(_)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_old_module_only_gets_the_fields_it_has() {
        let root = temp_dir("old");
        let emu = root.join("emu");
        fs::create_dir_all(emu.join("user")).unwrap();
        let pay = root.join("payload");
        payload(&pay, 0x48);
        let res = install(&InstallRequest::new(&emu, Settings::default(), &pay)).unwrap();
        assert!(res.target.join("exefs/subsdk9").is_file());
        let _ = fs::remove_dir_all(root);
    }
}
