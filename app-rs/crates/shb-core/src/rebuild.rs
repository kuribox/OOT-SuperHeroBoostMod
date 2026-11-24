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

/// What the install page can do with a recompiled game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Copy the mod and compile it: the whole thing.
    Install,
    /// Only run the builder, on what is already in the game folder (hand edits included). Needs the mod installed.
    Recompile,
    /// Write the module, its settings block, `image.bin` and the romfs — nothing has to be compiled.
    Settings,
    /// Uninstall first, then install: the mod files are copied again from scratch.
    Reinstall,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Mode> {
        match text {
            "install" => Some(Mode::Install),
            "recompile" | "rebuild" => Some(Mode::Recompile),
            "settings" | "apply" => Some(Mode::Settings),
            "reinstall" => Some(Mode::Reinstall),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Install => "install",
            Mode::Recompile => "recompile",
            Mode::Settings => "settings",
            Mode::Reinstall => "reinstall",
        }
    }

    /// Whether this mode runs the compiler.
    pub fn compiles(self) -> bool {
        matches!(self, Mode::Install | Mode::Recompile | Mode::Reinstall)
    }
}

/// What to rebuild.
#[derive(Debug, Clone)]
pub struct RebuildRequest {
    pub game: PathBuf,
    /// A profile name (`recommended`, `custom`, …).
    pub profile: String,
    /// The app folder (the one that holds `payload/`).
    pub app: PathBuf,
    /// What to do (a recompiled game).
    pub mode: Mode,
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

/// Whether a folder is a builder this port can drive (`tools/nx2/make_game.py` inside it).
pub fn is_builder(root: &Path) -> bool {
    root.join("tools")
        .join("nx2")
        .join("make_game.py")
        .is_file()
}

/// The builder this port would use for a game folder: the launcher's newest toolchain, or one beside the game.
pub fn detected_builder(game: &Path) -> Option<Builder> {
    let found = game::find(game)
        .ok_or_else(|| Error::NotAGame(game.to_path_buf()))
        .ok()?;
    find_builders(&found.root, found.flavor).into_iter().next()
}

/// Do it: write the settings, rebuild when asked, and report as the build goes.
///
/// `stop` is the window's cancel flag; the recompiler is killed when it is set.
pub fn run(
    req: &RebuildRequest,
    stop: Arc<AtomicBool>,
    mut report: impl FnMut(BuildEvent),
) -> Result<()> {
    let result = run_steps(req, stop, &mut report);
    if let Err(err) = &result
        && matches!(req.mode, Mode::Install | Mode::Reinstall)
        && req.rebuild
    {
        // A failed install must not leave the mod half in: the folder goes back to what it was before.
        match roll_back(&req.game) {
            Ok(true) => report(BuildEvent::Note(format!(
                "the install failed ({err}); the mod was removed again, nothing is left half installed"
            ))),
            Ok(false) => {}
            Err(why) => report(BuildEvent::Note(format!(
                "the install failed and the mod could not be removed again: {why}"
            ))),
        }
    }
    result
}

/// Undo what a failed install put in a game folder (a recompiled game or a Rekarina install). `true` when there was
/// something to undo.
fn roll_back(folder: &Path) -> Result<bool> {
    if let Some(rk) = crate::rekarina::find(folder) {
        return crate::rekarina::uninstall(&rk);
    }
    match game::find(folder) {
        Some(found) => game::uninstall(&found),
        None => Ok(false),
    }
}

fn run_steps(
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
    match req.mode {
        Mode::Settings => {
            // The settings and the romfs only: nothing here needs the compiler.
            game::apply_settings(&game, &settings, &payload)?;
            report(BuildEvent::Note(
                "settings and romfs written; the code was not rebuilt".to_string(),
            ));
            return Ok(());
        }
        Mode::Recompile => {
            // Only the builder: the mod is not copied, so whatever is in the game folder is what gets compiled.
            if !game::is_installed(&game) {
                return Err(Error::Build {
                    key: "recomp_needs_install".to_string(),
                    arg: game.root.display().to_string(),
                });
            }
        }
        Mode::Reinstall => {
            if game::is_installed(&game) {
                game::uninstall(&game)?;
            }
            game::stage(&game, &settings, &payload)?;
        }
        Mode::Install => {
            // The module has to be in exefs/ before the lift, and written again after the build (which replaces it).
            game::stage(&game, &settings, &payload)?;
        }
    }
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
    if req.mode == Mode::Reinstall || req.mode == Mode::Install {
        // The build rewrites the executable and may have rewritten the module: write the settings into it again so the
        // compiled code and the file agree.
        game::stage(&game, &settings, &payload)?;
    }
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
    if req.mode == Mode::Settings || !req.rebuild {
        crate::rekarina::apply_settings(&rk, settings, payload)?;
        report(BuildEvent::Note(
            "Rekarina: settings written; the game was not rebuilt".to_string(),
        ));
        return Ok(());
    }
    if req.mode == Mode::Reinstall {
        crate::rekarina::uninstall(&rk)?;
    }
    // The mod's folder has to be in `mods/` before the converter runs: it reads our IPS from there.
    crate::rekarina::stage(&rk, settings, payload)?;
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
            mode: Mode::Install,
            rebuild: true,
            builder: None,
        };
        assert!(req.payload().ends_with("payload"));
    }

    #[test]
    fn the_modes_say_which_ones_compile() {
        assert!(Mode::Install.compiles());
        assert!(Mode::Recompile.compiles());
        assert!(Mode::Reinstall.compiles());
        assert!(!Mode::Settings.compiles());
        assert_eq!(Mode::parse("recompile"), Some(Mode::Recompile));
        assert_eq!(Mode::parse("settings"), Some(Mode::Settings));
        assert_eq!(Mode::parse("nonsense"), None);
        assert_eq!(Mode::Install.name(), "install");
    }

    #[test]
    fn a_failed_install_leaves_the_game_as_it_was() {
        let root = std::env::temp_dir().join(format!("shb-rollback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let game_dir = root.join("game");
        let exefs = game_dir.join(".longshot_exefs");
        std::fs::create_dir_all(game_dir.join("bin")).unwrap();
        std::fs::create_dir_all(&exefs).unwrap();
        std::fs::write(game_dir.join("bin/longshot.exe"), b"exe").unwrap();
        let mut main = vec![0u8; 0x100];
        main[0x40..0x44].copy_from_slice(&[0xE0, 0xF3, 0xCC, 0xFA]);
        std::fs::write(exefs.join("main"), &main).unwrap();
        let magic = b"SHBCFG01";
        let mut stock = vec![0x22u8; 0x400];
        stock[0x100..0x108].copy_from_slice(magic);
        stock[0x108..0x10C].copy_from_slice(&1u32.to_le_bytes());
        stock[0x10C..0x110].copy_from_slice(&0x61u32.to_le_bytes());
        // A game that was never given the mod has no module in its ExeFS.
        std::fs::write(game_dir.join("image.bin"), &stock).unwrap();
        let app = root.join("app");
        let variant = app.join("payload").join("standalone");
        std::fs::create_dir_all(&variant).unwrap();
        let mut ours = stock.clone();
        ours.iter_mut().skip(0x200).for_each(|b| *b = 0x33);
        std::fs::write(variant.join("subsdk9"), &ours).unwrap();
        std::fs::write(variant.join("ours.ips"), b"IPS32EEOF").unwrap();

        let req = RebuildRequest {
            game: game_dir.clone(),
            profile: "recommended".to_string(),
            app,
            mode: Mode::Install,
            rebuild: true,
            // A builder that is not there: the compile step fails after the mod was copied in.
            builder: Some(root.join("no-builder")),
        };
        let mut notes: Vec<String> = Vec::new();
        let result = run(&req, Arc::new(AtomicBool::new(false)), |event| {
            if let BuildEvent::Note(text) = event {
                notes.push(text);
            }
        });
        assert!(result.is_err(), "the build cannot run without a builder");
        let found = game::find(&game_dir).unwrap();
        assert!(!game::is_installed(&found), "left installed: {notes:?}");
        assert!(!exefs.join("subsdk9").exists());
        assert_eq!(std::fs::read(game_dir.join("image.bin")).unwrap(), stock);
        assert!(notes.iter().any(|n| n.contains("removed again")), "{notes:?}");
        let _ = std::fs::remove_dir_all(&root);
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
