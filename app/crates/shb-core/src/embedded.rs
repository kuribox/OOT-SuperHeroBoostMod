//! The payload, embedded in this executable.
//!
//! `build.rs` turns `app/payload` into one `include_bytes!` per file when the folder is there at build time, so the app
//! installs the mod with nothing beside it: the module of every variant, the IPS files, the config block, the romfs
//! overlays and the enemy packs are inside the binary. They are unpacked once into the user's data folder, under a
//! folder named after the payload's stamp, and every part of the app keeps reading files from a path.
//!
//! A `payload/` folder beside the executable (or `SHB_PAYLOAD`) still wins: that is how the app is developed and how
//! a payload can be swapped without rebuilding the binary.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::paths;

include!(concat!(env!("OUT_DIR"), "/payload_embed.rs"));

/// Whether this binary carries the payload.
pub fn embedded() -> bool {
    !FILES.is_empty()
}

/// How many files it carries (for the About page and the logs).
pub fn count() -> usize {
    FILES.len()
}

/// The payload, unpacked into the user's data folder on first use.
///
/// The folder is keyed by the payload's stamp: a new payload unpacks into a new folder instead of mixing with the old
/// one, and a half-written unpack (an interrupted first run) is never mistaken for a complete one.
pub fn ensure() -> Result<PathBuf> {
    if !embedded() {
        return Err(Error::PayloadMissing(PathBuf::from(
            "no payload beside the executable and none embedded in it",
        )));
    }
    let root = paths::user_data_dir().join("payload").join(STAMP);
    let stamp = root.join(".complete");
    if stamp.is_file() {
        return Ok(root);
    }
    for (name, data) in FILES {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        // Skip a file that is already there with the right size: a resumed unpack only writes what is missing.
        if let Ok(meta) = std::fs::metadata(&path)
            && meta.len() == data.len() as u64
        {
            continue;
        }
        std::fs::write(&path, data).map_err(|e| Error::io(&path, e))?;
    }
    std::fs::write(&stamp, STAMP.as_bytes()).map_err(|e| Error::io(&stamp, e))?;
    Ok(root)
}

/// The payload when it is embedded and unpacked, without creating anything else.
pub fn unpacked() -> Option<PathBuf> {
    if !embedded() {
        return None;
    }
    let root = paths::user_data_dir().join("payload").join(STAMP);
    root.join(".complete").is_file().then_some(root)
}

/// Where a file of the payload would land, for the messages.
pub fn location() -> Option<PathBuf> {
    embedded().then(|| paths::user_data_dir().join("payload").join(STAMP))
}

/// The same check the rest of the app does: does this folder look like a payload?
pub fn looks_like_payload(dir: &Path) -> bool {
    crate::find::looks_like_payload(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_payload_is_whole_or_absent() {
        if !embedded() {
            assert!(FILES.is_empty());
            assert_eq!(count(), 0);
            return;
        }
        // Every file has a relative name and bytes, and the module the app installs is among them.
        assert!(
            FILES
                .iter()
                .all(|(name, data)| !name.starts_with('/') && !data.is_empty())
        );
        assert!(
            FILES
                .iter()
                .any(|(name, _)| name.starts_with("standalone/")),
            "the standalone module is part of the payload"
        );
        assert!(!STAMP.is_empty());
    }
}
