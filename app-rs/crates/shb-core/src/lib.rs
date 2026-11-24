//! Super Hero Boost companion, in Rust: the same job as `app/` in the Python tree.
//!
//! The mod ships precompiled (`app/payload/standalone/subsdk9` + `ours.ips`); this crate patches the configuration
//! block inside that module, copies the button art and the optional romfs files into the emulator's mod folder,
//! and does nothing that needs a compiler or the game dump.
//!
//! Layout of the crate:
//! - [`settings`]: everything the user can choose, with the JSON of the Python app
//! - [`roles`]: the button remap table and what the module accepts
//! - [`config`]: the `ShbConfig` block, written field by field at documented offsets
//! - [`presets`]: the game profiles
//! - [`install`]: the install / uninstall of the mod folder
//! - [`ips`], [`find`], [`paths`]: the small parts around it

pub mod config;
pub mod enemy;
pub mod error;
pub mod find;
pub mod game;
pub mod gamecube;
pub mod install;
pub mod ips;
pub mod layouts;
pub mod paths;
pub mod presets;
pub mod rebuild;
pub mod recomp_build;
pub mod rekarina;
pub mod rekarina_build;
pub mod roles;
pub mod romfs;
pub mod settings;
pub mod update;
pub mod zstd;

pub use error::{Error, Result};
pub use install::{InstallRequest, InstallResult};
pub use settings::Settings;

/// Version of this app (its own: the mod's version lives in `app/version.py`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The payload folder (`app/payload` of the repository): where the prebuilt module, the button art and the romfs
/// options live. It is not committed, so the app looks next to itself and, when run from a checkout, next to the
/// `app-rs` folder.
pub fn payload_root(explicit: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    if let Some(path) = explicit {
        return Some(path.to_path_buf());
    }
    if let Some(path) = std::env::var_os("SHB_PAYLOAD") {
        return Some(std::path::PathBuf::from(path));
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(dir) = &exe_dir {
        candidates.push(dir.join("payload"));
        // `cargo run` puts the binary in target/<profile>: walk up to the checkout.
        let mut dir = Some(dir.as_path());
        for _ in 0..4 {
            let Some(d) = dir else { break };
            candidates.push(d.join("app/payload"));
            candidates.push(d.join("payload"));
            dir = d.parent();
        }
    }
    candidates.push(std::path::PathBuf::from("app/payload"));
    candidates
        .into_iter()
        .find(|p| p.join("standalone").is_dir())
}
