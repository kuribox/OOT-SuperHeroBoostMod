//! The GameCube artwork (`app/gamecube.py`): a local copy of the game's controller atlas with the Basic Controls
//! diagram replaced.
//!
//! The game's UI archive is 244 MB once decompressed, so this reads it whole, checks that each region it is about to
//! overwrite still holds the textures the artwork was cut for (a SHA-256 per region), swaps them, and compresses the
//! result the way the emulator's log wants it. A different archive is refused, never patched blind.
//!
//! The artwork itself lives in `gamecube_textures.rs` / `.bin`, generated from the Python app by
//! `tools/gen_gamecube.py`.

use crate::error::{Error, Result, read, write};
use crate::layouts::ATLAS as ATLAS_REL;
use crate::zstd;
use std::path::{Path, PathBuf};

include!("gamecube_textures.rs");

/// The mod folder the old standalone diagram shipped in.
pub const LEGACY_MOD: &str = "GameCube-System-Controller";

/// Where the game's original archive is: `SHB_ROMFS` when set (an extracted romfs elsewhere), else `<root>/romfs`.
pub fn source_of(install_root: &Path) -> PathBuf {
    match std::env::var_os("SHB_ROMFS") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join(ATLAS_REL),
        _ => install_root.join("romfs").join(ATLAS_REL),
    }
}

/// The same, with an explicit romfs folder: what the tests use, so they never touch the process environment.
fn source_with(install_root: &Path, romfs: Option<&Path>) -> PathBuf {
    match romfs {
        Some(dir) => dir.join(ATLAS_REL),
        None => install_root.join("romfs").join(ATLAS_REL),
    }
}

fn digest(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Read the game's atlas, put the GameCube diagram in, and compress it back.
pub fn make_atlas(install_root: &Path) -> Result<Vec<u8>> {
    let override_dir = std::env::var_os("SHB_ROMFS")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from);
    make_atlas_with(install_root, override_dir.as_deref())
}

fn make_atlas_with(install_root: &Path, romfs: Option<&Path>) -> Result<Vec<u8>> {
    let source = source_with(install_root, romfs);
    if !source.is_file() {
        return Err(Error::GameCube(format!(
            "this needs the game's original UI archive at {}: extract the romfs and point SHB_ROMFS at it",
            source.display()
        )));
    }
    let packed = read(&source)?;
    let mut atlas = zstd::decompress(&packed).map_err(|_| {
        Error::GameCube(format!(
            "could not read {} as a zstd archive",
            source.display()
        ))
    })?;
    if atlas.len() != ATLAS_SIZE || atlas.get(..4) != Some(b"BNTX") {
        return Err(Error::GameCube(format!(
            "{} is not the UI archive this artwork was cut for",
            source.display()
        )));
    }
    for (offset, size, expected, start) in TEXTURES {
        let end = offset + size;
        let region = atlas
            .get_mut(*offset..end)
            .ok_or_else(|| Error::GameCube("a region falls outside the archive".into()))?;
        if &digest(region) != expected {
            return Err(Error::GameCube(format!(
                "{} has different controller textures at {offset}: use the original archive",
                source.display()
            )));
        }
        let replacement = &REPLACEMENTS[*start..*start + *size];
        region.copy_from_slice(replacement);
    }
    zstd::compress(&atlas, 6)
}

/// Write it into a mod's romfs.
pub fn write_atlas(romfs: &Path, atlas: &[u8]) -> Result<()> {
    let path = romfs.join(ATLAS_REL);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    write(&path, atlas)
}

/// Move a recognized standalone diagram out of the game's resource lookup (`.disabled` next to it), so only ours is
/// read. Returns what happened, for the note the window shows.
pub fn disable_legacy_atlas(mods: &Path) -> Result<Option<String>> {
    let path = mods.join(LEGACY_MOD).join("romfs").join(ATLAS_REL);
    if !path.is_file() {
        return Ok(None);
    }
    let ours = digest(&read(&path)?);
    if !LEGACY_ATLAS_HASHES.contains(&ours.as_str()) {
        return Ok(None);
    }
    let backup = path.with_file_name(format!(
        "{}.disabled",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    if backup.exists() {
        if digest(&read(&backup)?) != ours {
            return Err(Error::GameCube(format!(
                "move the standalone {LEGACY_MOD} mod out of {}: its backup already exists",
                mods.display()
            )));
        }
        std::fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
    } else {
        std::fs::rename(&path, &backup).map_err(|e| Error::io(&path, e))?;
    }
    Ok(Some(format!(
        "the standalone {LEGACY_MOD} diagram was switched off"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generated_table_is_consistent() {
        assert!(!TEXTURES.is_empty());
        for (offset, size, expected, start) in TEXTURES {
            assert_eq!(expected.len(), 64);
            assert!(expected.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(
                *offset + *size <= ATLAS_SIZE,
                "a region overflows the archive"
            );
            assert!(
                *start + *size <= REPLACEMENTS.len(),
                "a replacement is out of the blob"
            );
        }
        assert!(LEGACY_ATLAS_HASHES.iter().all(|h| h.len() == 64));
    }

    #[test]
    fn a_missing_archive_is_reported() {
        let root = std::env::temp_dir().join(format!("shb-rs-gc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let err = make_atlas_with(&root, None).unwrap_err();
        assert!(matches!(err, Error::GameCube(_)), "{err}");
        assert!(format!("{err}").contains("SHB_ROMFS"), "{err}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_different_archive_is_refused_instead_of_patched() {
        let root = std::env::temp_dir().join(format!("shb-rs-gc-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source = root.join(ATLAS_REL);
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        // A BNTX of the right size is not enough: every region must still hold the textures we know.
        let mut atlas = vec![0u8; ATLAS_SIZE];
        atlas[..4].copy_from_slice(b"BNTX");
        std::fs::write(&source, zstd::compress(&atlas, 3).unwrap()).unwrap();
        let err = make_atlas_with(&root, Some(&root)).unwrap_err();
        assert!(matches!(err, Error::GameCube(_)));
        assert!(
            format!("{err}").contains("different controller textures"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
