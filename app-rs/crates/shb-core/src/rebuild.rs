//! Installing a recompiled game *and* rebuilding it — in Rust, so the app is one executable.
//!
//! A static recompilation (Longshot, Hookshot, ReLink) only runs the mod when the module is compiled into the game.
//! This module does what `app/recomp.py`'s `install()` does for those folders: stage the module and its settings, find
//! the builder the launcher installed, and drive it (`recomp_build`) — the recompiler's `make_game.py` and the
//! toolchain it uses are external programs, like a C project's compiler, and nothing of *this* app runs in Python.
//!
//! Rekarina is the other kind: its three stages (`gen.py`, `build_game.py`, its patch converters) are Rekarina's own
//! toolchain, shipped with its `setup/` folder and a private interpreter. `rekarina_build` runs that pipeline and does
//! this app's part of it in Rust (the ExeFS, the patch tables, the extra seeds, the expected hashes). Either way, no
//! part of *this* app runs in Python.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Error, Result};
use crate::game::{self, Flavor};
use crate::presets;
use crate::recomp_build::{self, BuildEvent};
use crate::settings::Settings;

/// What to rebuild.
#[derive(Debug, Clone)]
pub struct RebuildRequest {
    pub game: PathBuf,
    /// A profile name (`recommended`, `custom`, …).
    pub profile: String,
    /// The app folder (the one that holds `payload/`).
    pub app: PathBuf,
    /// `false` patches the settings only: the code stays as the last build left it.
    pub rebuild: bool,
    /// A builder folder to use instead of the one the launcher installed.
    pub builder: Option<PathBuf>,
}

impl RebuildRequest {
    /// The payload: the folder beside the app when there is one, else the one this executable carries.
    pub fn payload(&self) -> PathBuf {
        let beside = self.app.join("payload");
        if crate::find::looks_like_payload(&beside) || beside.join("standalone").is_dir() {
            return beside;
        }
        crate::payload_root(None).unwrap_or(beside)
    }
}

/// A builder folder, found the way the launcher keeps them.
#[derive(Debug, Clone, PartialEq)]
pub struct Builder {
    pub root: PathBuf,
    /// Found under `%LOCALAPPDATA%\<Launcher>\toolchains\<version>\`.
    pub from_launcher: bool,
}

/// Every folder that could hold a builder for this flavor.
///
/// Longshot's launcher keeps `toolchains/<version>/LongshotBuilder`; a portable package is `LongshotBuilder` beside
/// the game or the app. A folder counts when it has `tools/nx2/make_game.py` — the Python builder. (v3's .NET
/// `LongshotBuilder` does not have it, and is reported as missing rather than run with the wrong arguments.)
pub fn find_builders(game: &Path, flavor: Flavor) -> Vec<Builder> {
    let mut out: Vec<Builder> = Vec::new();
    let has_script = |root: &Path| {
        root.join("tools")
            .join("nx2")
            .join("make_game.py")
            .is_file()
    };
    let names: &[&str] = match flavor {
        Flavor::ReLink => &["ReLinkLauncher", "ReLink"],
        Flavor::Hookshot => &["HookshotLauncher", "Hookshot"],
        Flavor::Longshot => &["LongshotLauncher", "Longshot"],
    };
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        for name in names {
            let base = Path::new(&local).join(name).join("toolchains");
            let Ok(entries) = std::fs::read_dir(&base) else {
                continue;
            };
            let mut versions: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
            versions.sort();
            for version in versions.into_iter().rev() {
                for candidate in [version.join("LongshotBuilder"), version.clone()] {
                    if has_script(&candidate) {
                        out.push(Builder {
                            root: candidate,
                            from_launcher: true,
                        });
                        break;
                    }
                }
            }
        }
    }
    for candidate in [
        game.join("LongshotBuilder"),
        game.join("builder"),
        game.join("tools").join("longshot-builder"),
    ] {
        if has_script(&candidate) {
            out.push(Builder {
                root: candidate,
                from_launcher: false,
            });
        }
    }
    out
}

/// Do it: write the settings, rebuild when asked, and report as the build goes.
///
/// `stop` is the window's cancel flag; the recompiler is killed when it is set.
pub fn run(
    req: &RebuildRequest,
    stop: Arc<AtomicBool>,
    mut report: impl FnMut(BuildEvent),
) -> Result<()> {
    let settings = presets::settings_for(&req.profile);
    let payload = req.payload();
    if crate::rekarina::find(&req.game).is_some() {
        return run_rekarina(req, &settings, &payload, stop, report);
    }
    let game = game::find(&req.game).ok_or_else(|| Error::NotAGame(req.game.clone()))?;
    // The module has to be in exefs/ before the lift, and written again after the build (which replaces the file).
    game::stage(&game, &settings, &payload)?;
    if !req.rebuild {
        report(BuildEvent::Note(
            "settings written; the code was not rebuilt".to_string(),
        ));
        return Ok(());
    }
    let builder = match &req.builder {
        Some(root) => Builder {
            root: root.clone(),
            from_launcher: false,
        },
        None => find_builders(&game.root, game.flavor)
            .into_iter()
            .next()
            .ok_or_else(|| Error::Build {
                key: "recomp_no_builder".to_string(),
                arg: game.root.display().to_string(),
            })?,
    };
    report(BuildEvent::Note(format!(
        "builder: {}",
        builder.root.display()
    )));
    recomp_build::build(&game.root, &builder.root, None, stop, &mut report)?;
    // The build rewrites the executable and may have rewritten the module: write the settings into it again so the
    // compiled code and the file agree.
    game::stage(&game, &settings, &payload)?;
    report(BuildEvent::Note(format!(
        "the rebuild finished: the module is now part of {}",
        game.root.join("image.bin").display()
    )));
    Ok(())
}

/// Rekarina: the same driver, with Rekarina's own tools doing the lift and the link.
///
/// The steps: the settings go into a copy of the module, the mods' patches are converted by Rekarina's converters, the
/// lifter translates `main, subsdk0, sdk, subsdk9` with the extra seeds our hooks need, the compiler builds it, the
/// new executable replaces the old one, and the module's hash is written into it.
fn run_rekarina(
    req: &RebuildRequest,
    settings: &Settings,
    payload: &Path,
    stop: Arc<AtomicBool>,
    mut report: impl FnMut(BuildEvent),
) -> Result<()> {
    let rk = crate::rekarina::find(&req.game).ok_or_else(|| Error::NotAGame(req.game.clone()))?;
    if !req.rebuild {
        crate::rekarina::apply_settings(&rk, settings, payload)?;
        report(BuildEvent::Note(
            "Rekarina: settings written; the game was not rebuilt".to_string(),
        ));
        return Ok(());
    }
    let exe = crate::rekarina_build::build(&rk, settings, payload, stop.clone(), &mut report)?;
    if stop.load(Ordering::Relaxed) {
        return Err(Error::Build {
            key: "recomp_cancelled".to_string(),
            arg: String::new(),
        });
    }
    // The executable, the settings into the module and the module's hash into the executable.
    crate::rekarina::adopt_build(&rk, settings, payload, &exe)?;
    report(BuildEvent::Note(format!(
        "Rekarina: rebuilt {}",
        rk.root.join(crate::rekarina::EXE_NAME).display()
    )));
    Ok(())
}

/// The app folder (the one that holds `payload/`) from the payload path.
pub fn app_dir(payload: &Path) -> Result<PathBuf> {
    let app = payload
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| payload.to_path_buf());
    if !app.join("payload").is_dir() {
        return Err(Error::Build {
            key: "no_payload".to_string(),
            arg: app.display().to_string(),
        });
    }
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_payload_folder_is_under_the_app() {
        let req = RebuildRequest {
            game: PathBuf::from("g"),
            profile: "recommended".to_string(),
            app: PathBuf::from("F:/project/super-hero-boost/app"),
            rebuild: true,
            builder: None,
        };
        assert!(req.payload().ends_with("payload"));
    }

    #[test]
    fn a_folder_without_a_payload_is_reported() {
        let root = std::env::temp_dir().join(format!("shb-appdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let err = app_dir(&root).unwrap_err();
        assert!(format!("{err}").contains("no_payload"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_builders_are_the_ones_with_make_game() {
        let root = std::env::temp_dir().join(format!("shb-builders-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let builder = root.join("LongshotBuilder");
        std::fs::create_dir_all(builder.join("tools").join("nx2")).unwrap();
        std::fs::write(builder.join("tools").join("nx2").join("make_game.py"), "").unwrap();
        let found = find_builders(&root, Flavor::Longshot);
        assert!(found.iter().any(|b| b.root == builder), "{found:?}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
