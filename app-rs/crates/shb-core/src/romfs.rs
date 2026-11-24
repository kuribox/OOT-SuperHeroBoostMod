//! The romfs half of an install: the button art, the GameCube diagram, the drops, the world options, the enemy packs
//! and the chosen controller layout, written into a mod's `romfs` folder.
//!
//! The three targets differ in details that matter, so the differences are named instead of guessed:
//!
//! - the emulator and a recompiled game take the button art whole, the downloadable prompt pack included;
//! - Rekarina draws its own PlayStation/Xbox prompts at run time, so its mod folder must not carry that font or the
//!   game-wide pack (a file would shadow Rekarina's own switching), and it merges resource size tables at run time,
//!   so no table surgery either.

use crate::error::{Error, Result};
use crate::install::copy_tree;
use crate::settings::{ButtonStyle, GameLayout, Settings};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// The emulator's `user/load/<title id>/<mod>`.
    Emulator,
    /// A static recompilation's `mods/<mod>`.
    Recompiled,
    /// Rekarina's `mods/<mod>`.
    Rekarina,
}

fn style_dir(style: ButtonStyle) -> &'static str {
    match style {
        ButtonStyle::Switch => "switch",
        ButtonStyle::Xbox => "xbox",
        ButtonStyle::PlayStation => "playstation",
    }
}

/// Copy a tree, leaving out a top-level folder (`Font2` for Rekarina).
fn copy_tree_skipping(src: &Path, dst: &Path, skip: &[&str]) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(src).map_err(|e| Error::io(src, e))? {
        let entry = entry.map_err(|e| Error::io(src, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if skip.contains(&name.as_str()) {
            continue;
        }
        if path.is_dir() {
            copy_tree_skipping(&path, &dst.join(&name), skip)?;
        } else {
            let out = dst.join(&name);
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            std::fs::copy(&path, &out).map_err(|e| Error::io(&out, e))?;
        }
    }
    Ok(())
}

/// Write every romfs file `s` asks for into `romfs`, which belongs to a mod installed under `install_root` (that is
/// where the game's own UI archive is looked for when the GameCube diagram is on).
pub fn populate(
    romfs: &Path,
    s: &Settings,
    payload: &Path,
    install_root: &Path,
    variant: Variant,
) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    std::fs::create_dir_all(romfs).map_err(|e| Error::io(romfs, e))?;
    let gamecube = s.bm_layout == crate::settings::BmLayout::Gamecube;

    if s.button_mapping {
        let art = payload.join("button_romfs");
        let style = style_dir(s.button_style);
        // Rekarina: not the style's External font, which would shadow the font it switches while the game runs.
        let skip: &[&str] = if variant == Variant::Rekarina {
            &["Font2"]
        } else {
            &[]
        };
        copy_tree_skipping(&art.join(style), romfs, skip)?;
        copy_tree_skipping(&art.join("common"), romfs, &[])?;
    }

    // The GameCube diagram: its button art and the game's own UI atlas, patched in place.
    if gamecube
        && s.button_mapping
        && !s.bm_simple
        && !matches!(s.button_style, ButtonStyle::Xbox | ButtonStyle::PlayStation)
    {
        let art = payload.join("button_romfs").join("gamecube");
        if !art.is_dir() {
            return Err(Error::PayloadMissing(art));
        }
        copy_tree_skipping(&art, romfs, &[])?;
    }
    if gamecube {
        match crate::gamecube::make_atlas(install_root) {
            Ok(atlas) => crate::gamecube::write_atlas(romfs, &atlas)?,
            // The diagram needs the game's own archive, which an emulator install does not have on disk: the rest of
            // the install does not depend on it, so this is a note and not a failure.
            Err(err) => notes.push(format!("{err}")),
        }
    }

    // The drops are two changes to the same game table: when both are on, one combined file is installed.
    let drops: &[&str] = match (s.no_heart_drop, s.no_magic_drop) {
        (true, true) => &["no_heart_magic_drop"],
        (true, false) => &["no_heart_drop"],
        (false, true) => &["no_magic_drop"],
        (false, false) => &[],
    };
    for opt in drops {
        let src = payload.join("romfs_opts").join(opt);
        if src.is_dir() {
            copy_tree(&src, romfs)?;
        } else {
            notes.push(format!(
                "{}: not built (needs the game dump), skipped",
                opt.replace('_', " ")
            ));
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
        let src = payload.join("romfs_opts").join(opt);
        if src.is_dir() {
            copy_tree(&src, romfs)?;
        } else {
            notes.push(format!("{}: not built, skipped", opt.replace('_', " ")));
        }
    }

    notes.extend(crate::enemy::install(romfs, s, payload)?);

    // The game-wide prompts: a download, and not for Rekarina (it ships its own) nor when our own diagram is on.
    if s.game_layout != GameLayout::None && variant != Variant::Rekarina && !gamecube {
        let style = s.game_layout.as_str();
        crate::layouts::apply(romfs, style, payload)?;
        notes.push(format!("game prompts: {style} layout installed"));
    }
    Ok(notes)
}
