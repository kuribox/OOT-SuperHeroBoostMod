//! The window. Same pages, same palette, same two fonts and the same texts as the Python app (`app-legacy/ui_kit.py`,
//! `app-legacy/i18n.py`).
//!
//! Everything goes through `shb-core`, so the window and `shb` (the CLI) cannot drift apart: the same settings
//! produce the same bytes. The window adds no logic of its own beyond remembering the last path.

// A windowed program, like the PyInstaller build of the Python app: no console in release. `--version` still
// prints, which is what the build scripts use.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod i18n;
mod preview;
mod theme;
mod widgets;

use eframe::egui;
use i18n::{fill, lang_name, tr};
use shb_core::find;
use shb_core::install::{self, InstallRequest};
use shb_core::presets::{self, PROFILE_KEYS};
use shb_core::roles::{Button, Role, Roles};
use shb_core::settings::{BmLayout, ButtonStyle, GameLayout, HudMode, Lang, Settings};
use shb_core::update::{self, Update};
use shb_core::{paths, payload_root, rebuild};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc as channel;
use std::sync::mpsc::{Receiver, channel};
use theme::{ghost_button, primary_button, section};

/// What the update check is doing, so the page can paint it.
enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(Update),
    Downloading { done: Arc<AtomicU64>, total: u64 },
    Ready(PathBuf),
    Failed(String),
}

/// The answers of the background threads.
enum UpdateMsg {
    Checked(Result<Option<Update>, String>),
    Downloaded(Result<PathBuf, String>),
}

/// A page of the sidebar, in the Python app's order (`nav` in `main.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Install,
    Presets,
    Gameplay,
    Difficulty,
    Controls,
    Hud,
    Misc,
    About,
}

impl Page {
    const ALL: [Page; 8] = [
        Page::Install,
        Page::Presets,
        Page::Gameplay,
        Page::Difficulty,
        Page::Controls,
        Page::Hud,
        Page::Misc,
        Page::About,
    ];

    /// The page named by `SHB_PAGE` (a development aid: screenshots, and the docs' captures).
    fn from_env() -> Page {
        let name = std::env::var("SHB_PAGE")
            .unwrap_or_default()
            .to_ascii_lowercase();
        for page in Page::ALL {
            if page.name() == name {
                return page;
            }
        }
        Page::Install
    }

    fn name(self) -> &'static str {
        match self {
            Page::Install => "install",
            Page::Presets => "presets",
            Page::Gameplay => "gameplay",
            Page::Difficulty => "difficulty",
            Page::Controls => "controls",
            Page::Hud => "hud",
            Page::Misc => "misc",
            Page::About => "about",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Page::Install => "nav_install",
            Page::Presets => "nav_home",
            Page::Gameplay => "nav_stamina",
            Page::Difficulty => "nav_difficulty",
            Page::Controls => "nav_controls",
            Page::Hud => "nav_hud",
            Page::Misc => "nav_misc",
            Page::About => "nav_about",
        }
    }
}

/// Where the mod goes: the emulator's mod folder, a recompiled game, or a Rekarina install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Emulator,
    Recomp,
    Rekarina,
}

impl Target {
    const ALL: [Target; 3] = [Target::Emulator, Target::Recomp, Target::Rekarina];

    /// Where this target's folder sits in `App::paths`.
    fn slot(self) -> usize {
        match self {
            Target::Emulator => 0,
            Target::Recomp => 1,
            Target::Rekarina => 2,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Target::Emulator => "target_emu",
            Target::Recomp => "target_recomp",
            Target::Rekarina => "target_rekarina",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Target::Emulator => "emu_hint",
            Target::Recomp => "recomp_hint",
            Target::Rekarina => "rekarina_hint",
        }
    }
}

/// Why a folder cannot be used: the message the page shows and the actions it disables.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Problem {
    /// Not an emulator folder: no program in it.
    Emulator,
    /// Not a recompiled game: no ExeFS or no launcher.
    Game,
    /// Not a builder: no `tools/nx2/make_game.py`.
    Builder,
    /// Not a Rekarina install.
    Rekarina,
    /// A Rekarina install without its `setup/<version>/` (its own compiler).
    Setup,
}

impl Problem {
    fn key(self) -> &'static str {
        match self {
            Problem::Emulator => "e_path_emulator",
            Problem::Game => "e_path_game",
            Problem::Builder => "e_path_builder",
            Problem::Rekarina => "rekarina_missing",
            Problem::Setup => "rekarina_setup_missing",
        }
    }
}

type StatusKey = (Target, String, String);

/// What looking at the folders on the install page found.
#[derive(Clone, Debug)]
struct PathStatus {
    /// The program the folder would open, or what is wrong with it.
    valid: Result<std::path::PathBuf, Problem>,
    builder: Result<(), Problem>,
    /// The mod is in the folder (what the actions depend on).
    installed: bool,
    /// Still being looked at: nothing can be done yet.
    pending: bool,
}

impl PathStatus {
    fn pending() -> Self {
        PathStatus {
            valid: Err(Problem::Emulator),
            builder: Ok(()),
            installed: false,
            pending: true,
        }
    }
}

/// Look at the folders: the slow part of what the install page shows. Runs in a thread.
fn inspect_paths(target: Target, emu: &str, builder: &str) -> PathStatus {
    let path = PathBuf::from(emu);
    let valid = if emu.is_empty() {
        Err(match target {
            Target::Emulator => Problem::Emulator,
            Target::Recomp => Problem::Game,
            Target::Rekarina => Problem::Rekarina,
        })
    } else {
        match target {
            Target::Emulator => shb_core::find::emulator_exe(&path).ok_or(Problem::Emulator),
            Target::Recomp => shb_core::find::launcher_of(&path)
                .filter(|_| shb_core::game::find(&path).is_some())
                .ok_or(Problem::Game),
            Target::Rekarina => rekarina_launcher(&path),
        }
    };
    // The builder field only applies to a recompiled game (Rekarina brings its own toolchain).
    let builder_ok = if target != Target::Recomp
        || builder.is_empty()
        || shb_core::rebuild::is_builder(std::path::Path::new(builder))
    {
        Ok(())
    } else {
        Err(Problem::Builder)
    };
    let installed = match target {
        Target::Emulator => install::inspect(&path).installed.is_some(),
        Target::Recomp => shb_core::game::find(&path)
            .map(|game| shb_core::game::is_installed(&game))
            .unwrap_or(false),
        Target::Rekarina => shb_core::rekarina::find(&path)
            .map(|rk| shb_core::rekarina::is_built(&rk))
            .unwrap_or(false),
    };
    PathStatus {
        valid,
        builder: builder_ok,
        installed,
        pending: false,
    }
}

/// A Rekarina folder with its `setup/` (its compiler ships there: without it nothing can be built) and the program
/// it opens.
fn rekarina_launcher(path: &std::path::Path) -> Result<std::path::PathBuf, Problem> {
    let rk = shb_core::rekarina::find(path).ok_or(Problem::Rekarina)?;
    if shb_core::rekarina_build::find_setup(&rk.root).is_none() {
        return Err(Problem::Setup);
    }
    shb_core::rekarina::launcher_of(&rk).ok_or(Problem::Rekarina)
}

/// How an action ended, for the window that reports it.
#[derive(Clone, Debug)]
struct Outcome {
    ok: bool,
    headline: String,
    detail: Vec<String>,
}

/// Which of the two folder fields asked for a folder: the page's path, or the builder of a recompiled game.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Browse {
    Target,
    Builder,
}

/// The stamina modes of the config block, with the Python app's names.
const PRESET_KEYS: [&str; 4] = ["m_hardcore", "m_combat", "m_sprint", "m_advanced"];

struct App {
    page: Page,
    target: Target,
    emu: String,
    /// The folder last used for each target (emulator, recompiled game, Rekarina): switching between them must not
    /// throw the path of the one you leave away.
    paths: [String; 3],
    settings: Settings,
    profile: String,
    ui_lang: Lang,
    payload: Option<PathBuf>,
    log: Vec<String>,
    notes: Vec<String>,
    failed: bool,
    logo: Option<egui::TextureHandle>,
    status: Option<String>,
    update: UpdateState,
    /// The key of the text a "?" opened, painted as a window over the page.
    help: Option<&'static str>,
    /// How the last action ended, shown in a window that has to be dismissed: success or failure, never silence.
    outcome: Option<Outcome>,
    /// What the folders on the install page were last found to be, and for which folders (see `path_status`).
    status_cache: Option<(StatusKey, PathStatus)>,
    status_rx: Option<(StatusKey, channel::Receiver<PathStatus>)>,
    /// An action ended: look at the folders again.
    status_dirty: bool,
    /// The frame it opened on: the click that opens it is outside the window it just created, and closing on that
    /// click made every "?" look dead.
    help_frame: u64,
    /// The lines of a rebuild in progress: the Python app's own build, run in a thread.
    rebuild_rx: Option<channel::Receiver<Busy>>,
    /// Set to stop a running build.
    rebuild_stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The folder dialog, while the player is choosing one: the OS dialog blocks, so it runs in a thread.
    pick_rx: Option<channel::Receiver<Option<std::path::PathBuf>>>,
    /// The builder of a recompiled game: detected when the folder is chosen, and editable (some people keep several).
    builder: String,
    /// What a running action is doing, for the bar.
    busy: Option<Busy>,
    /// Whether the next folder to come back is the builder rather than the game folder.
    pick_builder: bool,
    update_rx: Option<Receiver<UpdateMsg>>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let (settings, _) = paths::load_settings();
        // The folders, the builder and the target are remembered in the same settings file the Python app uses, so a
        // user keeps them when switching between the two (and `SHB_EMU` still overrides for a test run).
        let state = paths::load_ui_state();
        // The target decides which of the three folders the page shows; `SHB_EMU` overrides for a test run.
        let target = match state.target.as_str() {
            "recomp" => Target::Recomp,
            "rekarina" => Target::Rekarina,
            _ => Target::Emulator,
        };
        let remembered = match target {
            Target::Emulator => state.emu.clone(),
            Target::Recomp => state.recomp.clone(),
            Target::Rekarina => state.rekarina.clone(),
        };
        let emu = match std::env::var("SHB_EMU") {
            Ok(from_env) if !from_env.is_empty() => from_env,
            _ => remembered,
        };
        let paths = match target {
            Target::Emulator => [emu.clone(), state.recomp.clone(), state.rekarina.clone()],
            Target::Recomp => [state.emu.clone(), emu.clone(), state.rekarina.clone()],
            Target::Rekarina => [state.emu.clone(), state.recomp.clone(), emu.clone()],
        };
        // The window speaks the system language when it is one of ours, like the Python app.
        let ui_lang = shb_core::settings::Lang::parse(&state.lang).unwrap_or_else(system_lang);
        let profile = if state.profile.is_empty() {
            presets::DEFAULT_PROFILE.to_string()
        } else {
            state.profile.clone()
        };
        let mut app = App {
            // `SHB_PAGE=difficulty` opens straight on that page: how the screenshots of this window are taken.
            page: Page::from_env(),
            target,
            emu,
            paths,
            settings,
            profile,
            ui_lang,
            payload: payload_root(None),
            log: Vec::new(),
            notes: Vec::new(),
            failed: false,
            logo: theme::logo(&cc.egui_ctx),
            status: None,
            update: UpdateState::Idle,
            help_frame: 0,
            // `SHB_OUTCOME=ok|fail` opens the result window at start: how its screenshots are taken.
            status_cache: None,
            status_rx: None,
            status_dirty: false,
            outcome: std::env::var("SHB_OUTCOME").ok().and_then(|how| {
                let ok = how == "ok";
                (ok || how == "fail").then(|| Outcome {
                    ok,
                    headline: if ok { "Mod installed." } else { "The action failed." }.to_string(),
                    detail: if ok {
                        Vec::new()
                    } else {
                        vec!["recomp_build_failed: the builder's own compiler could not build a test program".to_string()]
                    },
                })
            }),
            // `SHB_HELP=variant_d` opens that text at start: how the docs' screenshots are taken.
            help: std::env::var("SHB_HELP")
                .ok()
                .and_then(|key| HELP_KEYS.iter().copied().find(|known| *known == key)),
            rebuild_rx: None,
            rebuild_stop: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            pick_rx: None,
            // A remembered builder that is not one any more (the launcher was updated, the folder moved) must not
            // stay in the field: it is what the rebuild would run.
            builder: if shb_core::rebuild::is_builder(std::path::Path::new(&state.builder)) {
                state.builder.clone()
            } else {
                String::new()
            },
            busy: None,
            pick_builder: false,
            update_rx: None,
        };
        match app.payload.clone() {
            Some(path) => app.say(format!("payload: {}", path.display())),
            None => app.say(tr(app.ui_lang, "e_no_payload")),
        }
        app.refresh_emulator();
        // `SHB_BROWSE=builder|game` opens that folder dialog at start: how a headless check sees the dialog the
        // buttons open (the window keeps painting while the dialog runs in its own thread).
        match std::env::var("SHB_BROWSE").as_deref() {
            Ok("builder") => app.request_folder(Browse::Builder),
            Ok("game") | Ok("target") => app.request_folder(Browse::Target),
            _ => {}
        }
        app
    }

    fn t(&self, key: &'static str) -> &'static str {
        tr(self.ui_lang, key)
    }

    /// The window that closes an action: green and short on success, red with the reason on failure.
    fn show_outcome(&mut self, ui: &mut egui::Ui) {
        let Some(outcome) = &self.outcome else {
            return;
        };
        let lang = self.ui_lang;
        let (title_key, colour) = if outcome.ok {
            ("result_ok_title", theme::TEAL)
        } else {
            ("result_fail_title", theme::RED)
        };
        let frame = egui::Frame::window(ui.style()).inner_margin(egui::Margin::symmetric(22, 18));
        let modal = egui::Modal::new(ui.make_persistent_id("shb-outcome"))
            .frame(frame)
            .show(ui.ctx(), |ui| {
                ui.set_width(460.0);
                ui.label(
                    egui::RichText::new(tr(lang, title_key))
                        .size(18.0)
                        .strong()
                        .color(colour),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(&outcome.headline)
                        .size(13.5)
                        .color(theme::TEXT),
                );
                if !outcome.detail.is_empty() {
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for line in &outcome.detail {
                                ui.label(egui::RichText::new(line).size(12.0).color(theme::DIM));
                            }
                        });
                }
                ui.add_space(14.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.button(egui::RichText::new(tr(lang, "result_close")).size(13.0))
                        .clicked()
                })
                .inner
            });
        if modal.inner || modal.should_close() {
            self.outcome = None;
        }
    }

    /// An action ended: say so in the status line and in a window the player has to close.
    fn conclude(&mut self, ok: bool, headline: impl Into<String>, detail: Vec<String>) {
        let headline = headline.into();
        self.status_dirty = true;
        self.failed = !ok;
        self.status = Some(headline.clone());
        self.outcome = Some(Outcome {
            ok,
            headline,
            detail,
        });
    }

    fn say(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    fn refresh_emulator(&mut self) {
        if self.emu.trim().is_empty() {
            return;
        }
        let path = PathBuf::from(self.emu.trim());

        // Rekarina is its own kind of install: the game is `Rekarina.exe` beside the ExeFS and the mod is the module
        // the lift compiled in. There is no mods folder to look for, and no builder of ours — its `setup/` is the
        // toolchain, found next to the game.
        if self.target == Target::Rekarina {
            match shb_core::rekarina::find(&path) {
                Some(rk) => {
                    let built = shb_core::rekarina::is_built(&rk);
                    let settings = if shb_core::rekarina_build::find_setup(&rk.root).is_some() {
                        ""
                    } else {
                        tr(self.ui_lang, "rekarina_setup_missing")
                    };
                    let state = if built {
                        tr(self.ui_lang, "rekarina_built")
                    } else {
                        tr(self.ui_lang, "e_rekarina_needs_build")
                    };
                    self.status = Some(state.to_string());
                    self.say(format!("rekarina: {}", rk.root.display()));
                    self.say(state.to_string());
                    if !settings.is_empty() {
                        self.say(settings.to_string());
                    }
                }
                None => {
                    let missing = tr(self.ui_lang, "rekarina_missing");
                    self.status = Some(missing.to_string());
                    self.say(format!("{} {}", missing, path.display()));
                }
            }
            return;
        }

        // A recompiled game keeps its mods where its recompiler looks for them — `<game>/mods/<name>/romfs` — and not
        // in the emulator's `user/load/<title>`; the module in its exefs is what says the mod is there.
        if self.target == Target::Recomp {
            match shb_core::game::find(&path) {
                Some(game) => {
                    let mods = shb_core::game::mods_dir(&game);
                    let module = game.exefs.join("subsdk9");
                    let installed = shb_core::game::is_installed(&game);
                    self.status = Some(
                        tr(
                            self.ui_lang,
                            if installed {
                                "state_installed"
                            } else {
                                "state_not_installed"
                            },
                        )
                        .to_string(),
                    );
                    let line = if mods.is_dir() {
                        fill(self.t("mods_found"), &[("a", &mods.display().to_string())])
                    } else {
                        tr(self.ui_lang, "mods_missing").to_string()
                    };
                    self.say(line);
                    // The module is the mod: the recompiler folds `mods/<name>` into its own merged folder during a
                    // build, so that folder being gone afterwards says nothing about the install.
                    self.say(format!(
                        "{}: {}",
                        module.display(),
                        tr(
                            self.ui_lang,
                            if installed {
                                "installed"
                            } else {
                                "state_not_installed"
                            }
                        )
                    ));
                    if let Some(exe) = shb_core::find::launcher_of(&game.root) {
                        self.say(format!("{}: {}", tr(self.ui_lang, "launch"), exe.display()));
                    }
                    if !rebuild::is_builder(std::path::Path::new(self.builder.trim()))
                        && let Some(found) = rebuild::detected_builder(&path)
                    {
                        self.builder = found.root.display().to_string();
                        self.say(format!("builder: {}", self.builder));
                    }
                }
                None => {
                    let missing = tr(self.ui_lang, "mods_missing");
                    self.status = Some(missing.to_string());
                    self.say(format!("{} {}", missing, path.display()));
                }
            }
            return;
        }

        let info = install::inspect(&path);
        let installed = info.installed.as_ref().map(|p| {
            format!(
                "{} {}",
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                self.t("installed")
            )
        });
        let missing = tr(self.ui_lang, "mods_missing");
        let (status, line) = match (&info.mods, installed) {
            (Some(mods), Some(state)) => (
                fill(self.t("mods_found"), &[("a", &mods.display().to_string())]),
                format!(
                    "{} ({state})",
                    fill(self.t("mods_found"), &[("a", &mods.display().to_string())])
                ),
            ),
            (Some(mods), None) => (
                fill(self.t("mods_found"), &[("a", &mods.display().to_string())]),
                fill(self.t("mods_found"), &[("a", &mods.display().to_string())]),
            ),
            (None, _) => (missing.to_string(), missing.to_string()),
        };
        self.status = Some(status);
        self.say(line);
        if let Some(exe) = info.exe {
            self.say(format!("{}: {}", tr(self.ui_lang, "launch"), exe.display()));
        }
        // A recompiled game: the builder this port would run, so the player sees it and can change it.
        if self.target == Target::Recomp
            && !rebuild::is_builder(std::path::Path::new(self.builder.trim()))
            && let Some(found) = rebuild::detected_builder(&path)
        {
            self.builder = found.root.display().to_string();
            self.say(format!("builder: {}", self.builder));
        }
    }

    /// Open what the page points at: the emulator, the launcher of a recompiled game, or Rekarina's own executable.
    fn play(&mut self) {
        let exe = match self.check_paths() {
            Ok(exe) => exe,
            Err(problem) => {
                self.failed = true;
                self.status = Some(self.t(problem.key()).to_string());
                return;
            }
        };
        match shb_core::find::launch(&exe) {
            Ok(()) => {
                self.failed = false;
                let name = exe
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.status = Some(fill(self.t("launched"), &[("a", &name)]));
                self.say(format!("launched {name}"));
            }
            Err(err) => {
                self.failed = true;
                self.status = Some(format!("{err}"));
            }
        }
    }

    /// Keep what the window knows about where things are: the three folders, the builder, the target, the profile and
    /// the language. It is the Python app's own set of keys, in its own file.
    fn remember(&mut self) {
        // The folder of the target on screen is the one in the text field: `paths` only gets it when the target is left.
        self.paths[self.target.slot()] = self.emu.clone();
        let state = paths::UiState {
            emu: self.paths[0].clone(),
            recomp: self.paths[1].clone(),
            rekarina: self.paths[2].clone(),
            builder: self.builder.clone(),
            target: match self.target {
                Target::Emulator => "emulator",
                Target::Recomp => "recomp",
                Target::Rekarina => "rekarina",
            }
            .to_string(),
            profile: self.profile.clone(),
            lang: self.ui_lang.as_str().to_string(),
        };
        if let Err(err) = paths::save_ui_state(&state) {
            self.say(format!("could not remember the folders: {err}"));
        }
    }

    /// Whether the folder on the page is the kind of folder the target needs: the emulator's program, a
    /// recompiled game's ExeFS and launcher, a Rekarina install with its `setup/`. Gives the program it would open.
    fn check_paths(&self) -> Result<std::path::PathBuf, Problem> {
        inspect_paths(self.target, self.emu.trim(), self.builder.trim()).valid
    }

    /// What the install page knows about the folders on it, from the last time they were looked at. Looking is not
    /// free (a Rekarina install hashes a 350 MB program to know whether it is built), so it happens in a thread when
    /// the folders change or an action ends, not on every frame the page is drawn.
    fn path_status(&mut self, ctx: &egui::Context) -> PathStatus {
        let key = (
            self.target,
            self.emu.trim().to_string(),
            self.builder.trim().to_string(),
        );
        if let Some((asked, rx)) = &self.status_rx {
            match rx.try_recv() {
                Ok(found) => {
                    let done = asked.clone();
                    self.status_rx = None;
                    self.status_cache = Some((done, found));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.status_rx = None,
            }
        }
        let fresh = matches!(&self.status_cache, Some((k, _)) if *k == key) && !self.status_dirty;
        let pending = matches!(&self.status_rx, Some((k, _)) if *k == key);
        if !fresh && !pending && self.busy.is_none() {
            let (tx, rx) = channel();
            let (target, emu, builder) = key.clone();
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(inspect_paths(target, &emu, &builder));
                repaint.request_repaint();
            });
            self.status_rx = Some((key.clone(), rx));
            self.status_dirty = false;
        }
        match &self.status_cache {
            Some((_, status)) => status.clone(),
            None => PathStatus::pending(),
        }
    }

    /// Choosing a profile: its values go onto the settings, the choices it does not make stay the player's (the Python
    /// window's `apply_profile`, which `presets::apply_profile` follows).
    fn apply_profile(&mut self) {
        presets::apply_profile(&mut self.settings, &self.profile.clone());
        let line = format!("{} {}", self.t("profile"), self.profile);
        self.say(line);
    }

    /// Remember the folder of the target we leave and bring back the one of the target we go to.
    fn switch_target(&mut self, target: Target) {
        if target == self.target {
            return;
        }
        self.paths[self.target.slot()] = self.emu.clone();
        self.target = target;
        self.emu = self.paths[target.slot()].clone();
        self.refresh_emulator();
        self.remember();
    }

    /// Ask the system for a folder. The dialog blocks, so it runs in its own thread and the answer arrives through a
    /// channel — the window keeps drawing while it is open (and a dialog opened from the paint code would deadlock).
    /// One "Examinar…" for the two fields: it says which one asked (that is where the answer goes) and opens the
    /// dialog at the folder that is in it.
    fn request_folder(&mut self, which: Browse) {
        self.pick_builder = which == Browse::Builder;
        let start = match which {
            Browse::Target => PathBuf::from(self.emu.trim()),
            Browse::Builder => PathBuf::from(self.builder.trim()),
        };
        self.pick_folder_from(start);
    }

    fn pick_folder_from(&mut self, start: PathBuf) {
        if self.pick_rx.is_some() {
            return;
        }
        // A test or a headless run says where the answer would come from instead of opening a window.
        if std::env::var_os("SHB_NO_DIALOG").is_some() {
            let (tx, rx) = channel();
            let _ = tx.send(None);
            self.pick_rx = Some(rx);
            return;
        }
        let title = tr(self.ui_lang, "pick_folder").to_string();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new();
            if let Some(dir) = start.parent().filter(|dir| dir.is_dir()) {
                dialog = dialog.set_directory(dir);
            }
            let chosen = dialog.set_title(&title).pick_folder();
            let _ = tx.send(chosen);
        });
        self.pick_rx = Some(rx);
    }

    /// Take the folder the player chose, if the dialog has answered.
    fn poll_folder_choice(&mut self) {
        let Some(rx) = self.pick_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Some(path)) => {
                if self.pick_builder {
                    self.builder = path.display().to_string();
                    self.pick_builder = false;
                } else {
                    self.emu = path.display().to_string();
                    self.refresh_emulator();
                }
                self.remember();
            }
            Ok(None) => self.pick_builder = false,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // Still open: put it back.
                self.pick_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.pick_builder = false,
        }
    }

    /// Run one of the actions of a recompiled game (or of Rekarina): install, recompile, settings, reinstall.
    fn start_mode(&mut self, mode: shb_core::rebuild::Mode) {
        if self.busy.is_some() {
            return;
        }
        let Some(payload) = self.payload.clone() else {
            self.failed = true;
            self.notes = vec![self.t("e_no_payload").to_string()];
            return;
        };
        let app = payload
            .parent()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| payload.clone());
        let builder = self.builder.trim();
        let request = rebuild::RebuildRequest {
            game: PathBuf::from(self.emu.trim()),
            profile: self.profile.clone(),
            settings: Some(self.settings.clone()),
            mode,
            rebuild: true,
            app,
            builder: (!builder.is_empty()).then(|| PathBuf::from(builder)),
        };
        let (tx, rx) = channel();
        let started = format!(
            "{} {}",
            tr(self.ui_lang, action_name(mode)),
            request.game.display()
        );
        let stop = self.rebuild_stop.clone();
        stop.store(false, std::sync::atomic::Ordering::Relaxed);
        std::thread::spawn(move || {
            let sender = tx.clone();
            let result = rebuild::run(&request, stop, |event| {
                let _ = sender.send(match event {
                    shb_core::recomp_build::BuildEvent::Progress {
                        fraction,
                        text,
                        stage,
                    } => Busy::Progress {
                        fraction,
                        stage,
                        text,
                    },
                    shb_core::recomp_build::BuildEvent::Line(text) => Busy::Line(text),
                    shb_core::recomp_build::BuildEvent::Note(text) => Busy::Note(text),
                });
            });
            let _ = tx.send(Busy::Done(result.err().map(|err| format!("{err}")), mode));
        });
        self.rebuild_rx = Some(rx);
        self.busy = Some(Busy::Progress {
            fraction: 0.0,
            stage: "read".to_string(),
            text: tr(self.ui_lang, "starting").to_string(),
        });
        self.log.clear();
        self.log.push(started);
        self.failed = false;
        self.status = None;
    }

    /// Take what a running action printed since the last frame, and finish it.
    fn poll_rebuild(&mut self) {
        let Some(rx) = self.rebuild_rx.take() else {
            return;
        };
        let mut finished = false;
        loop {
            match rx.try_recv() {
                Ok(Busy::Line(line)) => self.log.push(line),
                Ok(Busy::Note(note)) => self.log.push(format!("· {note}")),
                Ok(Busy::Progress {
                    fraction,
                    stage,
                    text,
                }) => {
                    self.busy = Some(Busy::Progress {
                        fraction,
                        stage,
                        text,
                    });
                }
                Ok(Busy::Done(error, mode)) => {
                    finished = true;
                    self.busy = None;
                    self.refresh_emulator();
                    match error {
                        None => {
                            self.notes.clear();
                            let done = tr(self.ui_lang, action_done(mode)).to_string();
                            self.conclude(true, done, Vec::new());
                        }
                        Some(why) => {
                            self.notes = vec![why.clone()];
                            let failed = self.t("install_failed").to_string();
                            self.conclude(false, failed, vec![why]);
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    finished = true;
                    self.busy = None;
                    break;
                }
            }
        }
        if !finished {
            self.rebuild_rx = Some(rx);
        }
        if self.log.len() > 500 {
            let extra = self.log.len() - 500;
            self.log.drain(0..extra);
        }
    }

    fn install(&mut self) {
        match self.target {
            // An emulator takes the mod as files: nothing to compile.
            Target::Emulator => self.install_emulator(),
            // A recompiled game or a Rekarina install: the mod files *and* the build, which is what "install" means
            // there (the settings-only path is its own button).
            _ => self.start_mode(shb_core::rebuild::Mode::Install),
        }
    }

    fn install_emulator(&mut self) {
        let Some(payload) = self.payload.clone() else {
            self.failed = true;
            self.notes = vec![self.t("e_no_payload").to_string()];
            return;
        };
        if self.emu.trim().is_empty() {
            self.failed = true;
            self.notes = vec![self.t("mods_missing").to_string()];
            return;
        }
        let req = InstallRequest::new(
            PathBuf::from(self.emu.trim()),
            self.settings.clone(),
            payload,
        );
        match install::install(&req) {
            Ok(result) => {
                self.notes = result.notes;
                let line = format!("{} {}", self.t("installed"), result.target.display());
                self.say(line.clone());
                self.refresh_emulator();
                let notes = self.notes.clone();
                self.conclude(true, line, notes);
            }
            Err(err) => {
                let why = format!("error: {err}");
                self.notes = vec![why.clone()];
                let failed = self.t("install_failed").to_string();
                self.conclude(false, failed, vec![why]);
            }
        }
    }

    fn uninstall(&mut self) {
        if self.emu.trim().is_empty() {
            return;
        }
        let emu = PathBuf::from(self.emu.trim());
        let result = if self.target == Target::Rekarina
            && let Some(found) = shb_core::rekarina::find(&emu)
        {
            shb_core::rekarina::uninstall(&found)
        } else if self.target == Target::Recomp
            && let Some(found) = shb_core::game::find(&emu)
        {
            shb_core::game::uninstall(&found)
        } else {
            install::uninstall(&emu)
        };
        match result {
            Ok(true) => {
                self.notes.clear();
                let done = self.t("uninstalled").to_string();
                self.say(done.clone());
                self.refresh_emulator();
                self.conclude(true, done, Vec::new());
            }
            Ok(false) => {
                let nothing = self.t("nothing").to_string();
                self.say(nothing.clone());
                self.conclude(true, nothing, Vec::new());
            }
            Err(err) => {
                let why = format!("error: {err}");
                self.say(why.clone());
                let failed = self.t("install_failed").to_string();
                self.conclude(false, failed, vec![why]);
            }
        }
    }

    /// Ask GitHub in a thread: the window keeps painting while the request is on its way.
    fn start_check(&mut self) {
        let current = shb_core::VERSION.to_string();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = update::check(&current).map_err(|e| e.to_string());
            let _ = tx.send(UpdateMsg::Checked(result));
        });
        self.update_rx = Some(rx);
        self.update = UpdateState::Checking;
    }

    /// Download the release asset in a thread, with a counter the page reads every frame.
    fn start_download(&mut self, found: Update) {
        let (dest, _) = update::update_path(&found.version);
        let done = Arc::new(AtomicU64::new(0));
        let total = found.asset_size;
        let counter = Arc::clone(&done);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = update::download(&found, &dest, |bytes, _| {
                counter.store(bytes, Ordering::Relaxed);
            })
            .map_err(|e| e.to_string());
            let _ = tx.send(UpdateMsg::Downloaded(result));
        });
        self.update_rx = Some(rx);
        self.update = UpdateState::Downloading { done, total };
    }

    /// Take whatever the background thread produced, if anything.
    fn poll_update(&mut self) {
        let Some(rx) = &self.update_rx else { return };
        let Ok(message) = rx.try_recv() else { return };
        self.update_rx = None;
        match message {
            UpdateMsg::Checked(Ok(Some(found))) => self.update = UpdateState::Available(found),
            UpdateMsg::Checked(Ok(None)) => self.update = UpdateState::UpToDate,
            UpdateMsg::Checked(Err(err)) => self.update = UpdateState::Failed(err),
            UpdateMsg::Downloaded(Ok(path)) => self.update = UpdateState::Ready(path),
            UpdateMsg::Downloaded(Err(err)) => self.update = UpdateState::Failed(err),
        }
    }

    fn save(&mut self) {
        self.remember();
        match paths::save_settings(&self.settings) {
            Ok(path) => {
                let line = format!("{}: {}", self.t("status"), path.display());
                self.say(line);
            }
            Err(err) => self.say(format!("error: {err}")),
        }
    }
}

/// The name of a profile (`profile_custom` for the custom one).
fn profile_name(lang: Lang, key: &str) -> String {
    if key == "custom" {
        return tr(lang, "profile_custom").to_string();
    }
    // The keys are static in the table, so the list is spelled out instead of built at run time.
    let name_key = match key {
        "recommended" => "p_recommended",
        "recommended-lite" => "p_recommended_lite",
        "souls" => "p_souls",
        "souls-hero" => "p_souls_hero",
        "base" => "p_base",
        "manual-shield" => "p_manual_shield",
        "hero" => "p_hero",
        "hero-stamina" => "p_hero_stamina",
        "hero-explorer" => "p_hero_explorer",
        "super-hero" => "p_super_hero",
        "ultra-hero" => "p_ultra_hero",
        "survival" => "p_survival",
        "world" => "p_world",
        "beta-enemies" => "p_beta_enemies",
        "beta-all" => "p_beta_all",
        // The custom profile has no `p_custom`: its name is "Personalizado".
        _ => "profile_custom",
    };
    tr(lang, name_key).to_string()
}

/// The description key of a profile.
fn profile_desc_key(key: &str) -> &'static str {
    match key {
        "recommended" => "p_recommended_d",
        "recommended-lite" => "p_recommended_lite_d",
        "souls" => "p_souls_d",
        "souls-hero" => "p_souls_hero_d",
        "base" => "p_base_d",
        "manual-shield" => "p_manual_shield_d",
        "hero" => "p_hero_d",
        "hero-stamina" => "p_hero_stamina_d",
        "hero-explorer" => "p_hero_explorer_d",
        "super-hero" => "p_super_hero_d",
        "ultra-hero" => "p_ultra_hero_d",
        "survival" => "p_survival_d",
        "world" => "p_world_d",
        "beta-enemies" => "p_beta_enemies_d",
        "beta-all" => "p_beta_all_d",
        _ => "p_custom_d",
    }
}

/// The `*_d` key of a stamina style, for its "?".
fn preset_desc_key(value: usize) -> &'static str {
    const KEYS: [&str; 4] = ["m_hardcore_d", "m_combat_d", "m_sprint_d", "m_advanced_d"];
    KEYS.get(value).copied().unwrap_or("m_hardcore_d")
}

/// The one-sentence description of a stamina style.
fn preset_desc(lang: Lang, value: usize) -> &'static str {
    widgets::short(tr(lang, preset_desc_key(value))).leak()
}

/// The presets in the order the Python app lists them, the GameCube one last.
const PRESETS: [&str; 6] = [
    "classic",
    "action_down",
    "shield",
    "souls",
    "triggers",
    "gamecube",
];

/// The name key of a remap preset.
fn preset_name_key(key: &str) -> &'static str {
    match key {
        "classic" => "preset_classic",
        "action_down" => "preset_action_down",
        "shield" => "preset_shield",
        "souls" => "preset_souls",
        "triggers" => "preset_triggers",
        _ => "preset_gamecube",
    }
}

/// The description key of a remap preset.
fn preset_desc_key_for(key: &str) -> &'static str {
    match key {
        "classic" => "ctrl_preset_classic_d",
        "action_down" => "ctrl_preset_action_down_d",
        "shield" => "ctrl_preset_shield_d",
        "souls" => "ctrl_preset_souls_d",
        "triggers" => "ctrl_preset_triggers_d",
        _ => "ctrl_preset_gamecube_d",
    }
}

/// A role in the window's language.
fn into_role_name(lang: Lang, role: Role) -> String {
    tr(lang, role_key(role)).to_string()
}

/// The text key of a role.
fn role_key(role: Role) -> &'static str {
    match role {
        Role::None => "role_none",
        Role::Jump => "role_jump",
        Role::Sword => "role_sword",
        Role::Action => "role_action",
        Role::Target => "role_target",
        Role::Item1 => "role_item1",
        Role::Item2 => "role_item2",
        Role::Item3 => "role_item3",
        Role::ItemMain => "role_item_main",
        Role::Camera => "role_camera",
        Role::Sprint => "role_sprint",
        Role::Shield => "role_shield",
        Role::Selector => "role_selector",
        Role::Armory => "role_armory",
        Role::Back => "role_back",
    }
}

/// The extended explanation of a role, for the ones that have one.
fn role_help_key(role: Role) -> Option<&'static str> {
    match role {
        Role::Action => Some("role_action_help"),
        Role::ItemMain => Some("role_item_main_help"),
        Role::Armory => Some("role_armory_help"),
        Role::Back => Some("role_back_help"),
        Role::Target => Some("opt_switch_help"),
        Role::Sprint => Some("opt_toggle_help"),
        _ => None,
    }
}

/// The choices of one row of the table. They follow the switches, as the Python editor's do: the item roles only
/// with the custom item HUD, the shield only with the manual shield, and nothing of the remap itself with the button
/// mapping off. The item selector is a role of the remap (it presses the game's own selector key), not an item, so
/// the item HUD does not take it away. The role a button already has always stays in its list: a table that was made
/// before is not silently rewritten. `no_hold` is a button whose own role acts held or at once: it has nothing to
/// hold for, so only "none" is left.
fn role_choices(
    button: Button,
    current: Role,
    mapping_on: bool,
    items_on: bool,
    shield_on: bool,
    no_hold: bool,
) -> Vec<Role> {
    Role::ALL
        .iter()
        .copied()
        .filter(|role| shb_core::roles::role_allowed(*role, button))
        .filter(|role| !no_hold || *role == Role::None)
        .filter(|role| {
            let item = shb_core::roles::ITEM_ROLES.contains(role);
            let hidden = (item && !items_on)
                || (*role == Role::Selector && !mapping_on)
                || (*role == Role::Shield && !shield_on);
            !hidden || *role == current
        })
        .collect()
}

/// One role on one button: the role that was there moves to the button that had the new one, so every role stays on
/// a single button (the module's rule, `core/remap.h`).
fn take_role(current: &Roles, button: Button, picked: Role) -> Roles {
    let mut table = current.clone();
    let old = table.get(&button).copied().unwrap_or(Role::None);
    if picked != Role::None && picked != old && !shb_core::roles::DUP_ROLES.contains(&picked) {
        let holder = table
            .iter()
            .find(|(other, role)| **other != button && **role == picked)
            .map(|(other, _)| *other);
        if let Some(other) = holder {
            let replacement = if shb_core::roles::role_allowed(old, other) {
                old
            } else {
                Role::None
            };
            table.insert(other, replacement);
        }
    }
    table.insert(button, picked);
    shb_core::roles::normalize(&table)
}

/// Every text a "?" can open, spelled out so `SHB_HELP` can name one (a development aid).
const HELP_KEYS: &[&str] = &[
    "rm_help",
    "rm_hud_help",
    "variant_d",
    "bm_ext_d",
    "bm_simple_d",
    "bm_place_d",
    "hud_ctrl_scale_desc",
    "hud_count_scale_desc",
    "enemy_alert_help",
    "enemy_aggro_help",
    "opt_shield_help",
    "opt_switch_help",
    "opt_auto_help",
    "opt_toggle_help",
    "role_action_help",
    "role_item_main_help",
    "role_armory_help",
    "role_back_help",
    "stamina_ring_d",
    "stamina_bar_d",
    "game_layout_d",
];

/// The language of the window: the system's when it is one of ours, else English.
fn system_lang() -> Lang {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var)
            && let Some(lang) = Lang::from_tag(&value)
        {
            return lang;
        }
    }
    // Windows: GetUserDefaultUILanguage via the language index the emulators read is not available here, so the
    // app starts in English and the user picks one from the header.
    Lang::En
}

/// The small gold pill the Python app uses for the active profile (`Chip`).
fn chip(ui: &mut egui::Ui, text: &str) {
    egui::Frame::NONE
        .fill(theme::PANEL2)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .color(theme::ACCENT)
                    .size(12.0)
                    .strong(),
            );
        });
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let lang = self.ui_lang;
        let mut changed = false;
        self.poll_update();
        self.poll_rebuild();
        self.poll_folder_choice();

        egui::Panel::top("header")
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(14, 7)),
            )
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if let Some(logo) = &self.logo {
                        let height = 30.0;
                        let size = logo.size_vec2();
                        let width = height * size.x / size.y.max(1.0);
                        ui.image((logo.id(), egui::vec2(width, height)));
                        ui.add_space(6.0);
                    }
                    // The two lines, drawn as one block of *measured* height and centred against the logo.
                    //
                    // Laid out as widgets, the block either took the whole row's height (the title at the top, the
                    // subtitle at the bottom) or overflowed a guessed height and made the header taller. The galleys
                    // say exactly how tall the text is, so the block is allocated at that size and the two lines are
                    // placed inside it: the row's `Align::Center` then centres the block against the icon.
                    let title_text = tr(lang, "app_title").to_string();
                    let sub_text = tr(lang, "subtitle").to_string();
                    let title_font =
                        egui::FontId::new(19.0, egui::FontFamily::Name(theme::DISPLAY.into()));
                    let sub_font = egui::FontId::new(11.0, egui::FontFamily::Proportional);
                    let title_galley =
                        ui.painter()
                            .layout_no_wrap(title_text, title_font, theme::TEXT);
                    let sub_galley = ui.painter().layout_no_wrap(sub_text, sub_font, theme::DIM);
                    let gap = 1.0;
                    let block = egui::vec2(
                        title_galley.size().x.max(sub_galley.size().x),
                        title_galley.size().y + gap + sub_galley.size().y,
                    );
                    let (rect, _) = ui.allocate_exact_size(block, egui::Sense::hover());
                    let painter = ui.painter();
                    painter.galley(rect.left_top(), title_galley.clone(), theme::TEXT);
                    painter.galley(
                        egui::pos2(rect.left(), rect.top() + title_galley.size().y + gap),
                        sub_galley,
                        theme::DIM,
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // The flag beside the name: the fonts carry no flag emoji, so it is drawn.
                        ui.horizontal(|ui| {
                            widgets::flag(ui, self.ui_lang);
                            egui::ComboBox::from_id_salt("ui-lang")
                                .selected_text(lang_name(self.ui_lang))
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    for code in Lang::ALL {
                                        let response = ui
                                            .horizontal(|ui| {
                                                widgets::flag(ui, *code);
                                                ui.selectable_value(
                                                    &mut self.ui_lang,
                                                    *code,
                                                    lang_name(*code),
                                                )
                                            })
                                            .inner;
                                        let _ = response;
                                    }
                                });
                        });
                        ui.label(
                            egui::RichText::new(tr(lang, "language"))
                                .size(12.0)
                                .color(theme::DIM),
                        );
                        let name = if self.profile == "custom" {
                            tr(lang, "profile_custom").to_string()
                        } else {
                            self.profile.clone()
                        };
                        // Inside a `horizontal`, like the language box: a frame placed straight into this layout sits at the
                        // top of the header instead of on the line of the other controls.
                        ui.horizontal(|ui| {
                            chip(ui, &fill(tr(lang, "profile_current"), &[("a", &name)]));
                        });
                    });
                });
            });

        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(170.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(10, 12)),
            )
            .show(ui, |ui| {
                for page in Page::ALL {
                    let selected = self.page == page;
                    let text = if selected {
                        egui::RichText::new(tr(lang, page.key()))
                            .color(theme::ACCENT)
                            .strong()
                    } else {
                        egui::RichText::new(tr(lang, page.key())).color(theme::TEXT)
                    };
                    let fill = if selected {
                        theme::ROW_HOVER
                    } else {
                        theme::SIDEBAR
                    };
                    let button = egui::Button::new(text)
                        .fill(fill)
                        .stroke(egui::Stroke::NONE)
                        .corner_radius(egui::CornerRadius::same(10));
                    if ui.add_sized([ui.available_width(), 29.0], button).clicked() {
                        self.page = page;
                    }
                }
            });

        egui::Panel::bottom("footer")
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Installing and uninstalling live in the install section, next to the folder they act on; the
                    // footer keeps what is not about a target.
                    if primary_button(ui, tr(lang, "launch")).clicked() {
                        self.play();
                    }
                    if ghost_button(ui, tr(lang, "save_settings")).clicked() {
                        self.save();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(tr(lang, "install_hint"))
                                .size(11.0)
                                .color(theme::DIM),
                        );
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.page {
                        Page::Install => self.page_install(ui),
                        Page::Presets => changed |= self.page_presets(ui),
                        Page::Gameplay => changed |= self.page_gameplay(ui),
                        Page::Difficulty => changed |= self.page_difficulty(ui),
                        Page::Controls => changed |= self.page_controls(ui),
                        Page::Hud => changed |= self.page_hud(ui),
                        Page::Misc => changed |= self.page_misc(ui),
                        Page::About => self.page_about(ui),
                    });
            });

        self.show_outcome(ui);

        if let Some(key) = self.help {
            let mut open = true;
            let text = tr(lang, key).to_string();
            let title = text.split('.').next().unwrap_or(&text).to_string();
            // Always in the same place: opening one "?" and then another must not move the text around.
            let frame =
                egui::Frame::window(ui.style()).inner_margin(egui::Margin::symmetric(18, 14));
            let window = egui::Window::new(egui::RichText::new(title).size(14.0).strong())
                .anchor(egui::Align2::CENTER_TOP, [0.0, 96.0])
                .collapsible(false)
                .resizable(false)
                .title_bar(true)
                .frame(frame)
                .open(&mut open)
                .default_width(520.0);
            let shown = window.show(ui.ctx(), |ui| {
                ui.set_max_width(500.0);
                ui.add_space(2.0);
                ui.label(egui::RichText::new(text).size(13.0).color(theme::TEXT));
            });
            // A click anywhere else closes it, the way a dialog does — but not the click that opened it.
            let frame = ui.ctx().cumulative_frame_nr();
            if self.help_frame == 0 {
                self.help_frame = frame;
            }
            if let Some(shown) = &shown {
                let clicked_outside = ui.input(|i| {
                    i.pointer.any_click()
                        && i.pointer
                            .interact_pos()
                            .map(|pos| !shown.response.rect.contains(pos))
                            .unwrap_or(false)
                });
                if closes_help(frame, self.help_frame, clicked_outside) {
                    self.help = None;
                }
            }
            if !open {
                self.help = None;
            }
            if self.help.is_none() {
                self.help_frame = 0;
            }
        }

        if changed && self.profile != "custom" {
            // Any option the user touches turns the profile into Custom, like the Python window.
            self.profile = "custom".to_string();
        }
    }
}

impl App {
    fn page_install(&mut self, ui: &mut egui::Ui) {
        let lang = self.ui_lang;
        section(
            ui,
            tr(lang, "install_title"),
            Some(tr(lang, "install_desc")),
            |ui| {
                ui.label(
                    egui::RichText::new(tr(lang, "target"))
                        .size(12.0)
                        .color(theme::DIM),
                );
                ui.horizontal_wrapped(|ui| {
                    for target in Target::ALL {
                        let selected = self.target == target;
                        let button = egui::Button::new(
                            egui::RichText::new(tr(lang, target.key())).color(if selected {
                                theme::ON_ACCENT
                            } else {
                                theme::TEXT
                            }),
                        )
                        .fill(if selected {
                            theme::ACCENT
                        } else {
                            theme::PANEL2
                        })
                        .stroke(egui::Stroke::new(1.0, theme::BORDER))
                        .corner_radius(egui::CornerRadius::same(10))
                        .min_size(egui::vec2(0.0, 34.0));
                        if ui.add(button).clicked() {
                            self.switch_target(target);
                        }
                    }
                });
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(tr(lang, self.target.hint()))
                        .size(12.0)
                        .color(theme::DIM),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.emu)
                            .desired_width(ui.available_width() - 110.0)
                            .hint_text("E:/Switch2/Emu/…"),
                    );
                    // A typed or pasted folder is kept as soon as the field is left, not only the one from "Browse".
                    if field.lost_focus() {
                        self.remember();
                    }
                    if ghost_button(ui, tr(lang, "browse")).clicked() {
                        self.request_folder(Browse::Target);
                    }
                });

                // A recompiled game: which builder will run, and the way to change it. It is detected when the folder
                // is chosen, and the player can point at another one (a second install, a newer version). Rekarina is
                // not here: its lift and its compiler come with its own `setup/`, it has no builder of ours.
                if self.target == Target::Recomp {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr(lang, "builder_label"))
                                .size(12.0)
                                .color(theme::DIM),
                        );
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut self.builder)
                                .desired_width(ui.available_width() - 110.0)
                                .hint_text(tr(lang, "recomp_builder_btn")),
                        );
                        if field.lost_focus() {
                            self.remember();
                        }
                        if ghost_button(ui, tr(lang, "browse")).clicked() {
                            self.request_folder(Browse::Builder);
                        }
                    });
                }

                // The folder has to be the right one before anything can be done with it: the page says so and the
                // actions stay off until it is (a wrong path is the most common way to get a bare error).
                let status = self.path_status(ui.ctx());
                let pending = status.pending;
                let (valid, builder_ok, installed) =
                    (status.valid, status.builder, status.installed);
                let busy = self.busy.is_some();
                let ready = !busy && !pending && valid.is_ok() && builder_ok.is_ok();
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        tr(lang, "state"),
                        tr(
                            lang,
                            if installed {
                                "state_installed"
                            } else {
                                "state_not_installed"
                            }
                        )
                    ))
                    .size(12.0)
                    .color(if installed { theme::TEAL } else { theme::DIM }),
                );

                // The verdict on the folders: what is wrong, or what it will open.
                match (&valid, &builder_ok) {
                    _ if pending => {}
                    (Ok(exe), Ok(())) => {
                        let label = fill(self.t("path_ok"), &[("a", &exe.display().to_string())]);
                        ui.label(egui::RichText::new(label).size(11.5).color(theme::TEAL));
                    }
                    (Ok(_), Err(problem)) | (Err(problem), _) => {
                        let label = tr(lang, problem.key());
                        ui.label(egui::RichText::new(label).size(11.5).color(theme::AMBER));
                    }
                }

                // What this target can do. An emulator only installs; a recompiled game can compile without copying
                // the mod, copy the settings without compiling, or do both from scratch.
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if self.target == Target::Emulator {
                        if theme::primary_button_enabled(ui, tr(lang, "install"), ready).clicked() {
                            self.install();
                        }
                        if theme::ghost_button_enabled(
                            ui,
                            tr(lang, "uninstall"),
                            ready && installed,
                        )
                        .clicked()
                        {
                            self.uninstall();
                        }
                    } else {
                        // "Install" is the first-time path. On a game that already has the mod it would lay the files
                        // over the installed ones and break it, so it stays off: the other three buttons are for that.
                        let modes: [(shb_core::rebuild::Mode, bool); 4] = [
                            (shb_core::rebuild::Mode::Install, !installed),
                            (shb_core::rebuild::Mode::Settings, installed),
                            (shb_core::rebuild::Mode::Recompile, installed),
                            (shb_core::rebuild::Mode::Reinstall, installed),
                        ];
                        for (mode, enabled) in modes {
                            let label = tr(lang, action_name(mode));
                            let clicked = if mode == shb_core::rebuild::Mode::Install {
                                theme::primary_button_enabled(ui, label, ready).clicked()
                            } else {
                                theme::ghost_button_enabled(ui, label, ready && enabled).clicked()
                            };
                            if clicked {
                                self.start_mode(mode);
                            }
                        }
                        if theme::ghost_button_enabled(
                            ui,
                            tr(lang, "uninstall"),
                            !busy && installed,
                        )
                        .clicked()
                        {
                            self.uninstall();
                        }
                    }
                });

                // The bar: the builder's own progress, in the stage it is in.
                if let Some(Busy::Progress {
                    fraction,
                    stage,
                    text,
                }) = &self.busy
                {
                    ui.add_space(4.0);
                    let label = if text.is_empty() {
                        format!("{stage}  {:.0} %", fraction * 100.0)
                    } else {
                        format!("{stage}  {:.0} %  {text}", fraction * 100.0)
                    };
                    ui.add(egui::ProgressBar::new(*fraction).text(label));
                    if ghost_button(ui, tr(lang, "recomp_cancel")).clicked() {
                        self.rebuild_stop
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                }

                if let Some(status) = &self.status {
                    ui.label(
                        egui::RichText::new(status)
                            .size(12.0)
                            .color(if self.failed {
                                theme::AMBER
                            } else {
                                theme::TEAL
                            }),
                    );
                }
            },
        );
        ui.add_space(10.0);
        if !self.notes.is_empty() {
            section(ui, tr(lang, "status"), None, |ui| {
                for note in &self.notes {
                    let color = if self.failed {
                        theme::RED
                    } else {
                        theme::AMBER
                    };
                    ui.label(egui::RichText::new(note).color(color));
                }
            });
            ui.add_space(10.0);
        }
        section(ui, tr(lang, "recomp_log"), None, |ui| {
            egui::ScrollArea::vertical()
                .max_height(190.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log {
                        ui.label(
                            egui::RichText::new(line)
                                .monospace()
                                .size(12.0)
                                .color(theme::DIM),
                        );
                    }
                });
        });
    }

    fn page_presets(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut help = self.help;
        theme::section(
            ui,
            tr(lang, "home_title"),
            Some(tr(lang, "home_desc")),
            |ui| {
                // One card per profile: its name, the first sentence of what it changes, and the rest behind the "?".
                // Two columns when the window has room for them, one when it does not.
                let wide = ui.available_width() >= 700.0;
                if wide {
                    ui.columns(2, |cols| {
                        for (i, key) in PROFILE_KEYS.iter().enumerate() {
                            self.profile_card(&mut cols[i % 2], key, &mut help);
                            cols[i % 2].add_space(4.0);
                        }
                    });
                } else {
                    for key in PROFILE_KEYS {
                        self.profile_card(ui, key, &mut help);
                        ui.add_space(4.0);
                    }
                }
                ui.add_space(4.0);
                if primary_button(ui, tr(lang, "reapply")).clicked() {
                    self.apply_profile();
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(tr(lang, "profile_shield_note"))
                        .size(11.5)
                        .color(theme::DIM),
                );
            },
        );
        self.help = help;
        false
    }

    /// A profile: its name, the first sentence of what it changes and the rest behind the "?".
    ///
    /// The whole card takes the click, hit-tested by hand against its own rectangle: an interaction registered before
    /// the contents ends up under them (only a corner stayed clickable) and one registered after them eats the "?".
    /// The "?" keeps its own button, so a click there opens the text and does not select the card.
    fn profile_card(
        &mut self,
        ui: &mut egui::Ui,
        key: &'static str,
        help: &mut Option<&'static str>,
    ) {
        let lang = self.ui_lang;
        let selected = self.profile == key;
        let key_desc = profile_desc_key(key);
        let full = tr(lang, key_desc);
        let shown = widgets::short(full);
        let width = ui.available_width();
        let frame = egui::Frame::NONE
            .fill(if selected {
                theme::ACCENT
            } else {
                theme::PANEL2
            })
            .stroke(egui::Stroke::new(
                1.0,
                if selected {
                    theme::ACCENT
                } else {
                    theme::BORDER
                },
            ))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(10, 6));
        let mut help_rect = None;
        let drawn = frame.show(ui, |ui| {
            ui.set_width(width - 22.0);
            ui.horizontal(|ui| {
                // The name, and the star drawn instead of trusting a font that may not carry "★".
                let name = profile_name(lang, key);
                let starred = name.contains('★');
                let text = name.replace('★', "").trim_end().to_string();
                ui.label(
                    egui::RichText::new(&text)
                        .size(12.5)
                        .strong()
                        .color(if selected {
                            theme::ON_ACCENT
                        } else {
                            theme::TEXT
                        }),
                );
                if starred {
                    let color = if selected {
                        theme::ON_ACCENT
                    } else {
                        theme::ACCENT
                    };
                    widgets::star(ui, 11.0, color);
                }
                if shown != full {
                    help_rect = Some(widgets::help_icon(ui, key_desc, lang, help).rect);
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&shown).size(11.0).color(if selected {
                    theme::ON_ACCENT
                } else {
                    theme::DIM
                }));
            });
        });
        if card_pressed(ui, drawn.response.rect, help_rect) {
            self.profile = key.to_string();
            self.apply_profile();
        }
    }
    fn page_gameplay(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        let mut help = self.help;
        theme::section(
            ui,
            tr(lang, "stamina_title"),
            Some(tr(lang, "stamina_desc")),
            |ui| {
                if widgets::check(
                    ui,
                    &mut self.settings.stamina,
                    tr(lang, "stamina_on"),
                    tr(lang, "stamina_on_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                ui.horizontal_top(|ui| {
                    // Left: the display and the styles, each with its own sentence. Right: the preview.
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(tr(lang, "stamina_display"))
                                .size(13.0)
                                .strong(),
                        );
                        // Options, the way the Python app draws them: a radio, its name, its sentence and its "?".
                        for (bar, name_key, desc_key) in [
                            (false, "stamina_ring", "stamina_ring_d"),
                            (true, "stamina_bar", "stamina_bar_d"),
                        ] {
                            ui.horizontal(|ui| {
                                if ui
                                    .radio_value(
                                        &mut self.settings.stamina_bar,
                                        bar,
                                        tr(lang, name_key),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                widgets::help_icon(ui, desc_key, lang, &mut help);
                            });
                            let sentence = widgets::short(tr(lang, desc_key));
                            widgets::desc_under(ui, &sentence);
                        }
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(tr(lang, "stamina_mode"))
                                .size(13.0)
                                .strong(),
                        );
                        for (value, key) in PRESET_KEYS.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui
                                    .radio_value(
                                        &mut self.settings.preset,
                                        value as u8,
                                        tr(lang, key),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                widgets::help_icon(ui, preset_desc_key(value), lang, &mut help);
                            });
                            widgets::desc_under(ui, preset_desc(lang, value));
                        }
                    });
                    ui.add_space(24.0);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(tr(lang, "preview"))
                                .size(12.0)
                                .color(theme::DIM)
                                .strong(),
                        );
                        preview::stamina(ui, self.settings.stamina, self.settings.stamina_bar);
                    });
                });
                if !self.settings.stamina {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(tr(lang, "stamina_off_note"))
                            .size(12.0)
                            .color(theme::AMBER),
                    );
                }
            },
        );
        ui.add_space(10.0);
        theme::section(
            ui,
            tr(lang, "costs_title"),
            Some(tr(lang, "costs_desc")),
            |ui| {
                egui::Grid::new("costs")
                    .num_columns(2)
                    .spacing([28.0, 2.0])
                    .show(ui, |ui| {
                        for (i, (field, key)) in [
                            (&mut self.settings.jump, "a_jump"),
                            (&mut self.settings.attack, "a_attack"),
                            (&mut self.settings.sprint, "a_sprint"),
                            (&mut self.settings.block, "a_block"),
                            (&mut self.settings.shoot, "a_shoot"),
                            (&mut self.settings.roll, "a_roll"),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            if ui.checkbox(field, tr(lang, key)).changed() {
                                changed = true;
                            }
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                ui.add_space(4.0);
                if widgets::check(
                    ui,
                    &mut self.settings.combat_only,
                    tr(lang, "combat_only"),
                    "",
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
            },
        );
        ui.add_space(10.0);
        theme::section(ui, tr(lang, "sprint_speed"), None, |ui| {
            if widgets::combo_row(
                ui,
                tr(lang, "sprint_speed"),
                "sprint-speed",
                &mut self.settings.sprint_speed,
                &widgets::speed_options(lang),
                150.0,
            ) {
                changed = true;
            }
        });
        self.help = help;
        changed
    }
    fn page_difficulty(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        let mut help = self.help;
        theme::section(
            ui,
            tr(lang, "difficulty_title"),
            Some(tr(lang, "difficulty_desc")),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    if widgets::combo_row(
                        ui,
                        tr(lang, "enemy_hp"),
                        "mult-hp",
                        &mut self.settings.enemy_hp,
                        &widgets::mult_options(lang),
                        140.0,
                    ) {
                        changed = true;
                    }
                    ui.add_space(18.0);
                    if widgets::combo_row(
                        ui,
                        tr(lang, "damage"),
                        "mult-damage",
                        &mut self.settings.damage,
                        &widgets::mult_options(lang),
                        140.0,
                    ) {
                        changed = true;
                    }
                });
                ui.add_space(6.0);
                if widgets::check(
                    ui,
                    &mut self.settings.no_heart_drop,
                    tr(lang, "no_heart_drop"),
                    tr(lang, "no_heart_drop_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                if widgets::check(
                    ui,
                    &mut self.settings.no_magic_drop,
                    tr(lang, "no_magic_drop"),
                    tr(lang, "no_magic_drop_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                ui.label(
                    egui::RichText::new(tr(lang, "difficulty_note"))
                        .size(12.0)
                        .color(theme::DIM),
                );
            },
        );
        ui.add_space(10.0);
        theme::section(
            ui,
            tr(lang, "enemies_title"),
            Some(tr(lang, "enemies_desc")),
            |ui| {
                if widgets::check(
                    ui,
                    &mut self.settings.enemy_double,
                    tr(lang, "enemy_double_on"),
                    tr(lang, "enemy_double_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                ui.label(
                    egui::RichText::new(tr(lang, "enemy_beta_note"))
                        .size(12.0)
                        .color(theme::DIM),
                );
                ui.label(
                    egui::RichText::new(tr(lang, "enemy_speed"))
                        .size(13.0)
                        .strong(),
                );
                widgets::desc(ui, tr(lang, "enemy_speed_desc"));
                ui.add_space(2.0);
                let options = widgets::enemy_options(lang);
                egui::Grid::new("enemy-levels")
                    .num_columns(2)
                    .spacing([24.0, 4.0])
                    .show(ui, |ui| {
                        for (i, (field, key, help_key)) in [
                            (&mut self.settings.enemy_move, "enemy_move", None),
                            (&mut self.settings.enemy_anim, "enemy_anim", None),
                            (&mut self.settings.enemy_attack, "enemy_attack", None),
                            (&mut self.settings.enemy_proj, "enemy_proj", None),
                            (
                                &mut self.settings.enemy_alert,
                                "enemy_alert",
                                Some("enemy_alert_help"),
                            ),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            ui.horizontal(|ui| {
                                if widgets::combo_row(
                                    ui,
                                    tr(lang, key),
                                    key,
                                    field,
                                    &options,
                                    110.0,
                                ) {
                                    changed = true;
                                }
                                if let Some(help_key) = help_key {
                                    widgets::help_icon(ui, help_key, lang, &mut help);
                                }
                            });
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(tr(lang, "boss_speed"))
                        .size(13.0)
                        .strong(),
                );
                widgets::desc(ui, tr(lang, "boss_speed_desc"));
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    if widgets::combo_row(
                        ui,
                        tr(lang, "enemy_move"),
                        "boss-move",
                        &mut self.settings.boss_move,
                        &options,
                        110.0,
                    ) {
                        changed = true;
                    }
                    ui.add_space(18.0);
                    if widgets::combo_row(
                        ui,
                        tr(lang, "enemy_anim"),
                        "boss-anim",
                        &mut self.settings.boss_anim,
                        &options,
                        110.0,
                    ) {
                        changed = true;
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tr(lang, "enemy_aggro"))
                            .size(13.0)
                            .strong(),
                    );
                    widgets::help_icon(ui, "enemy_aggro_help", lang, &mut help);
                });
                ui.horizontal(|ui| {
                    for (value, key) in [(0u8, "aggro_0"), (1, "aggro_1"), (2, "aggro_2")] {
                        if ui
                            .selectable_value(&mut self.settings.enemy_aggro, value, tr(lang, key))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            },
        );
        self.help = help;
        changed
    }
    fn page_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        let mut help = self.help;
        // Everything below the switch is the mapping: with the mapping off the card is shown greyed, the way the
        // Python app disables it. The switch itself (and the item side) stay reachable.
        let on = self.settings.button_mapping;

        // With the mapping off the card below is greyed, so the switch that turns it back on is drawn here, where it
        // is still reachable (inside the greyed card it could not be clicked).
        if !on {
            ui.add_space(2.0);
            if widgets::check(
                ui,
                &mut self.settings.button_mapping,
                tr(lang, "bm_on"),
                tr(lang, "bm_on_desc"),
                None,
                &mut help,
            ) {
                changed = true;
            }
            ui.add_space(6.0);
        }

        ui.add_enabled_ui(on, |ui| {
            // The presets: one card per table, the way the Python app opens the page.
            theme::section(
                ui,
                tr(lang, "ctrl_preset_title"),
                Some(tr(lang, "ctrl_preset_desc")),
                |ui| {
                    let wide = ui.available_width() >= 620.0;
                    if wide {
                        ui.columns(2, |cols| {
                            for (i, key) in PRESETS.iter().enumerate() {
                                self.preset_card(&mut cols[i % 2], key, &mut help, &mut changed);
                                cols[i % 2].add_space(4.0);
                            }
                        });
                    } else {
                        for key in PRESETS {
                            self.preset_card(ui, key, &mut help, &mut changed);
                            ui.add_space(4.0);
                        }
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        widgets::help_icon(ui, "rm_help", lang, &mut help);
                        ui.label(
                            egui::RichText::new(widgets::short(tr(lang, "rm_help")))
                                .size(11.5)
                                .color(theme::DIM),
                        );
                    });
                },
            );

            // The buttons themselves: on/off, the pictures, the face-button positions.
            ui.add_space(8.0);
            theme::section(
                ui,
                tr(lang, "buttons_title"),
                Some(tr(lang, "ctrl_advanced_desc")),
                |ui| {
                    if widgets::check(
                        ui,
                        &mut self.settings.button_mapping,
                        tr(lang, "bm_on"),
                        tr(lang, "bm_on_desc"),
                        None,
                        &mut help,
                    ) {
                        changed = true;
                    }
                    // The item side of the mapping: our own item buttons and items 1-3 on any button. It rides on the
                    // mapping, so it is only offered while the mapping is on.
                    if self.settings.button_mapping
                        && widgets::check(
                            ui,
                            &mut self.settings.hud_items,
                            tr(lang, "hud_items"),
                            tr(lang, "hud_items_desc"),
                            None,
                            &mut help,
                        )
                    {
                        changed = true;
                    }
                    ui.add_space(3.0);
                    widgets::two_columns(ui, 700.0, 0.5, 260.0, |ui, column| match column {
                        widgets::Column::Left => {
                            ui.horizontal(|ui| {
                                // The pictures of the HUD buttons: Switch, PlayStation or Xbox.
                                let styles = vec![
                                    (ButtonStyle::Switch, tr(lang, "g_default").to_string()),
                                    (
                                        ButtonStyle::PlayStation,
                                        tr(lang, "g_playstation").to_string(),
                                    ),
                                    (ButtonStyle::Xbox, tr(lang, "g_xbox").to_string()),
                                ];
                                if widgets::combo_row(
                                    ui,
                                    tr(lang, "variant"),
                                    "variant",
                                    &mut self.settings.button_style,
                                    &styles,
                                    130.0,
                                ) {
                                    changed = true;
                                }
                                widgets::help_icon(ui, "variant_d", lang, &mut help);
                            });
                            ui.add_space(2.0);
                            // How many buttons the mod draws: the assignable ones only, or the extended set.
                            ui.horizontal(|ui| {
                                let options = vec![
                                    (false, tr(lang, "bm_ext").to_string()),
                                    (true, tr(lang, "bm_simple").to_string()),
                                ];
                                if widgets::combo_row(
                                    ui,
                                    tr(lang, "bm_layout"),
                                    "bm-layout",
                                    &mut self.settings.bm_simple,
                                    &options,
                                    150.0,
                                ) {
                                    changed = true;
                                }
                                let key = if self.settings.bm_simple {
                                    "bm_simple_d"
                                } else {
                                    "bm_ext_d"
                                };
                                widgets::help_icon(ui, key, lang, &mut help);
                            });
                        }
                        widgets::Column::Right => {
                            ui.horizontal(|ui| {
                                // The face-button positions, the same scheme as the recompiled games.
                                let places = vec![
                                    (
                                        BmLayout::Nintendo,
                                        tr(lang, "bm_place_nintendo").to_string(),
                                    ),
                                    (BmLayout::Ab, tr(lang, "bm_place_ab").to_string()),
                                    (BmLayout::Xbox, tr(lang, "bm_place_xbox").to_string()),
                                    (
                                        BmLayout::Gamecube,
                                        tr(lang, "bm_place_gamecube").to_string(),
                                    ),
                                ];
                                if widgets::combo_row(
                                    ui,
                                    tr(lang, "bm_place"),
                                    "bm-place",
                                    &mut self.settings.bm_layout,
                                    &places,
                                    200.0,
                                ) {
                                    changed = true;
                                }
                                widgets::help_icon(ui, "bm_place_d", lang, &mut help);
                            });
                        }
                    });
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        if widgets::percent_slider(
                            ui,
                            tr(lang, "hud_ctrl_scale"),
                            &mut self.settings.hud_ctrl_scale,
                            50,
                            200,
                            5,
                        ) {
                            changed = true;
                        }
                        widgets::help_icon(ui, "hud_ctrl_scale_desc", lang, &mut help);
                    });
                },
            );

            // The editor: one row per button, its roles, and the two previews.
            ui.add_space(8.0);
            theme::section(ui, tr(lang, "rm_title"), Some(tr(lang, "rm_help")), |ui| {
                // The rows are about 260 px wide: the previews sit right beside them with a gap of 18, in the same place
                // whatever buttons the table has (a percentage split left them far right of the selects).
                widgets::two_columns_sized(ui, 300.0, 18.0, 320.0, |ui, column| match column {
                    widgets::Column::Left => self.remap_rows(ui, lang, &mut changed, &mut help),
                    widgets::Column::Right => self.remap_previews(ui, lang, &mut help),
                });
            });
        });
        self.help = help;
        changed
    }

    /// Whether the settings are exactly this preset: its table with the user's own long presses, and the layout it
    /// sets (the GameCube preset is the classic table with the GameCube positions).
    fn preset_matches(&self, key: &str) -> bool {
        preset_matches(&self.settings, key)
    }

    /// One card of the presets: its name, the first sentence of its description, and the rest behind the "?".
    ///
    /// Same hit test as the profile cards: the whole card clicks and the "?" keeps its own button.
    fn preset_card(
        &mut self,
        ui: &mut egui::Ui,
        key: &'static str,
        help: &mut Option<&'static str>,
        changed: &mut bool,
    ) -> bool {
        let lang = self.ui_lang;
        // "Switch GameCube" is the classic table with the GameCube face-button positions: comparing the tables alone
        // marked "Clásico" as well, and the first card of the list won.
        let selected = self.preset_matches(key);
        let width = ui.available_width();
        let frame = egui::Frame::NONE
            .fill(if selected {
                theme::ACCENT
            } else {
                theme::PANEL2
            })
            .stroke(egui::Stroke::new(
                1.0,
                if selected {
                    theme::ACCENT
                } else {
                    theme::BORDER
                },
            ))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(10, 6));
        let mut help_rect = None;
        let drawn = frame.show(ui, |ui| {
            ui.set_width(width - 22.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr(lang, preset_name_key(key)))
                        .size(12.5)
                        .strong()
                        .color(if selected {
                            theme::ON_ACCENT
                        } else {
                            theme::TEXT
                        }),
                );
                help_rect = Some(widgets::help_icon(ui, preset_desc_key_for(key), lang, help).rect);
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(tr(lang, preset_desc_key_for(key)))
                        .size(11.0)
                        .color(if selected {
                            theme::ON_ACCENT
                        } else {
                            theme::DIM
                        }),
                );
            });
        });
        if card_pressed(ui, drawn.response.rect, help_rect) {
            self.apply_preset(key, changed);
        }
        selected
    }

    /// Apply one of the remap presets (the GameCube one also switches the style and the face-button positions).
    fn apply_preset(&mut self, key: &'static str, changed: &mut bool) {
        if key == "gamecube" {
            self.settings.bm_layout = BmLayout::Gamecube;
            self.settings.button_style = ButtonStyle::Switch;
            self.settings.bm_simple = false;
            self.settings.button_mapping = true;
        } else if self.settings.bm_layout == BmLayout::Gamecube {
            self.settings.bm_layout = BmLayout::Nintendo;
        }
        let table_key = if key == "gamecube" { "classic" } else { key };
        if let Some(table) = shb_core::roles::remap_preset(table_key) {
            let table = shb_core::roles::keep_holds(&table, &self.settings.bm_roles);
            // A hold a preset names fires after the normal action, so that button does not wait for it.
            for (button, role) in &table {
                if let Some(owner) = shb_core::roles::hold_owner(*button)
                    && *role != Role::None
                {
                    self.settings.bm_hold_wait.insert(owner, false);
                }
            }
            if table.values().any(|r| *r == Role::Shield) {
                self.settings.manual_shield = true;
            }
            self.settings.bm_roles = table;
            *changed = true;
        }
    }
    fn remap_rows(
        &mut self,
        ui: &mut egui::Ui,
        lang: Lang,
        changed: &mut bool,
        help: &mut Option<&'static str>,
    ) {
        let items = self
            .settings
            .bm_roles
            .values()
            .filter(|r| matches!(r, Role::Item1 | Role::Item2 | Role::Item3 | Role::ItemMain))
            .count();
        // The counter is about the item side of the mapping: with the item HUD off there is nothing to count.
        if self.settings.hud_items && self.settings.button_mapping {
            ui.label(
                egui::RichText::new(fill(tr(lang, "rm_items"), &[("n", &items.to_string())]))
                    .size(11.5)
                    .color(theme::DIM),
            );
        }
        let rows: [Button; 17] = [
            Button::X,
            Button::XHold,
            Button::Y,
            Button::YHold,
            Button::A,
            Button::AHold,
            Button::B,
            Button::BHold,
            Button::L,
            Button::LHold,
            Button::R,
            Button::RHold,
            Button::Zl,
            Button::Zr,
            Button::L3,
            Button::R3,
            Button::Dr,
        ];
        // One sentence for the whole list: a row marked "Hold" belongs to the button above it.
        ui.label(
            egui::RichText::new(tr(lang, "hold_intro"))
                .size(11.5)
                .color(theme::DIM),
        );
        ui.add_space(2.0);
        for button in rows {
            // A long press is the row of the button above it: its own role, when it acts held or at once (shield,
            // target, sprint, the selector, the armory, an item), leaves nothing to hold for.
            let owner = shb_core::roles::hold_owner(button);
            let blocked_by = owner.and_then(|own| {
                let role = self
                    .settings
                    .bm_roles
                    .get(&own)
                    .copied()
                    .unwrap_or(Role::None);
                shb_core::roles::HOLD_BLOCK_ROLES
                    .contains(&role)
                    .then_some(role)
            });
            ui.horizontal(|ui| {
                // The button drawn the way the preview draws it (a Switch A, an Xbox B, a PlayStation ○). A long
                // press is that same button, indented and tagged "Hold": the list is the pad, not its names.
                if owner.is_some() {
                    ui.add_space(26.0);
                }
                let drawn = owner.unwrap_or(button);
                preview::chip(ui, self.settings.button_style, drawn, None);
                if owner.is_some() {
                    ui.label(
                        egui::RichText::new(tr(lang, "hold_tag"))
                            .size(11.5)
                            .strong()
                            .color(theme::ACCENT),
                    );
                    widgets::help_icon(ui, "hold_help", lang, help);
                }
                let current = self
                    .settings
                    .bm_roles
                    .get(&button)
                    .copied()
                    .unwrap_or(Role::None);
                let mut picked = current;
                // The list follows the switches, as the Python editor's does: the item roles are only offered with
                // the custom item HUD, the shield only with the manual shield. The item selector is a role of the
                // remap itself (it only presses the game's own selector key), so it stays without the item HUD.
                let items_on = self.settings.hud_items && self.settings.button_mapping;
                let shield_on = self.settings.manual_shield;
                let choices: Vec<(Role, String)> = role_choices(
                    button,
                    current,
                    self.settings.button_mapping,
                    items_on,
                    shield_on,
                    blocked_by.is_some(),
                )
                .into_iter()
                .map(|role| (role, into_role_name(lang, role)))
                .collect();
                let moved = ui
                    .add_enabled_ui(blocked_by.is_none(), |ui| {
                        widgets::combo_row(
                            ui,
                            "",
                            &format!("role-{button:?}"),
                            &mut picked,
                            &choices,
                            150.0,
                        )
                    })
                    .inner;
                if moved {
                    self.settings.bm_roles = take_role(&self.settings.bm_roles, button, picked);
                    *changed = true;
                }
                if let Some(key) = role_help_key(picked) {
                    widgets::help_icon(ui, key, lang, help);
                }
            });
            // The options of the role, one under the other: stacked, they leave the previews their room.
            let role = self
                .settings
                .bm_roles
                .get(&button)
                .copied()
                .unwrap_or(Role::None);
            if let Some(blocking) = blocked_by {
                ui.indent(("hold-blocked", button), |ui| {
                    let text = fill(
                        tr(lang, "hold_blocked"),
                        &[("a", &into_role_name(lang, blocking))],
                    );
                    ui.label(egui::RichText::new(text).size(11.0).color(theme::DIM));
                });
            } else if let Some(wait_key) = owner.filter(|_| role != Role::None) {
                // How the short press tells itself from the long one: wait for the hold, or act at once. Only a face
                // button or L / R has this choice: the D-pad right's short press is the game's Navi call, so it always
                // waits for the hold (the note below says so).
                let mut wait = self
                    .settings
                    .bm_hold_wait
                    .get(&wait_key)
                    .copied()
                    .unwrap_or(false);
                ui.indent(("hold-wait", button), |ui| {
                    if widgets::check(
                        ui,
                        &mut wait,
                        tr(lang, "opt_hwait"),
                        "",
                        Some("opt_hwait_help"),
                        help,
                    ) {
                        self.settings.bm_hold_wait.insert(wait_key, wait);
                        *changed = true;
                    }
                });
            }
            if button == Button::Dr {
                // The arrow is the game's Navi call: its short press is not ours to configure, only the long one.
                ui.indent(("dpad-note", button), |ui| {
                    ui.label(
                        egui::RichText::new(tr(lang, "dr_note"))
                            .size(11.0)
                            .color(theme::DIM),
                    );
                });
            }
            if role == Role::Target {
                ui.indent(("target-options", button), |ui| {
                    if widgets::check(
                        ui,
                        &mut self.settings.manual_shield,
                        tr(lang, "opt_shield"),
                        "",
                        Some("opt_shield_help"),
                        help,
                    ) {
                        *changed = true;
                    }
                    if widgets::check(
                        ui,
                        &mut self.settings.switch_target,
                        tr(lang, "opt_switch"),
                        "",
                        Some("opt_switch_help"),
                        help,
                    ) {
                        *changed = true;
                    }
                    if widgets::check(
                        ui,
                        &mut self.settings.auto_target,
                        tr(lang, "opt_auto"),
                        "",
                        Some("opt_auto_help"),
                        help,
                    ) {
                        *changed = true;
                    }
                });
            } else if role == Role::Sprint {
                ui.indent(("sprint-options", button), |ui| {
                    if widgets::check(
                        ui,
                        &mut self.settings.sprint_l3_toggle,
                        tr(lang, "opt_toggle"),
                        "",
                        Some("opt_toggle_help"),
                        help,
                    ) {
                        *changed = true;
                    }
                });
            }
        }
        let missing: Vec<String> = shb_core::roles::roles_missing(&self.settings.bm_roles)
            .iter()
            .map(|role| into_role_name(lang, *role))
            .collect();
        if missing.is_empty() {
            ui.label(
                egui::RichText::new(tr(lang, "ctrl_all_assigned"))
                    .size(11.5)
                    .color(theme::DIM),
            );
        } else {
            ui.label(
                egui::RichText::new(fill(tr(lang, "rm_missing"), &[("a", &missing.join(", "))]))
                    .size(11.5)
                    .color(theme::AMBER),
            );
        }
        if self.settings.manual_shield
            && !self.settings.bm_roles.values().any(|r| *r == Role::Shield)
        {
            ui.label(
                egui::RichText::new(tr(lang, "rm_no_shield"))
                    .size(11.5)
                    .color(theme::AMBER),
            );
        }
    }

    /// The controller and the in-game HUD, side by side when the column is wide and stacked when it is not.
    fn remap_previews(&mut self, ui: &mut egui::Ui, lang: Lang, help: &mut Option<&'static str>) {
        ui.label(
            egui::RichText::new(tr(lang, "rm_pad"))
                .size(11.5)
                .color(theme::DIM)
                .strong(),
        );
        preview::pad(
            ui,
            &self.settings.bm_roles,
            self.settings.button_style,
            |role| into_role_name(lang, role),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr(lang, "rm_hud"))
                    .size(11.5)
                    .color(theme::DIM)
                    .strong(),
            );
            widgets::help_icon(ui, "rm_hud_help", lang, help);
        });
        preview::hud(
            ui,
            &self.settings.bm_roles,
            self.settings.button_style,
            self.settings.bm_simple,
            self.settings.bm_layout == BmLayout::Gamecube,
            |role| into_role_name(lang, role),
        );
    }
    fn page_hud(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        let mut help = self.help;
        theme::section(
            ui,
            tr(lang, "hud_title"),
            Some(tr(lang, "hud_desc")),
            |ui| {
                let modes = [
                    (HudMode::Wide, "hud_mode_wide", "hud_mode_wide_desc"),
                    (HudMode::Simple, "hud_mode_simple", "hud_mode_simple_desc"),
                    (HudMode::Plain, "hud_mode_plain", "hud_mode_plain_desc"),
                ];
                let options: Vec<(HudMode, String)> = modes
                    .iter()
                    .map(|(m, key, _)| (*m, tr(lang, key).to_string()))
                    .collect();
                if widgets::combo_row(
                    ui,
                    tr(lang, "hud_mode"),
                    "hud-mode",
                    &mut self.settings.hud_mode,
                    &options,
                    220.0,
                ) {
                    changed = true;
                }
                for (_, _, key) in modes {
                    widgets::desc_under(ui, tr(lang, key));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut self.settings.hud_rupees, tr(lang, "hud_rupees"))
                        .changed()
                    {
                        changed = true;
                    }
                    ui.add_space(18.0);
                    // The clock: off, 12 hours or 24 hours (two flags in the config block).
                    let mut clock = if !self.settings.hud_clock {
                        0u8
                    } else if self.settings.clock_24h {
                        2
                    } else {
                        1
                    };
                    let options = vec![
                        (0u8, tr(lang, "clock_off").to_string()),
                        (1, tr(lang, "clock_12").to_string()),
                        (2, tr(lang, "clock_24").to_string()),
                    ];
                    if widgets::combo_row(
                        ui,
                        tr(lang, "clock_mode"),
                        "clock-mode",
                        &mut clock,
                        &options,
                        130.0,
                    ) {
                        self.settings.hud_clock = clock > 0;
                        self.settings.clock_24h = clock == 2;
                        changed = true;
                    }
                });
                ui.add_space(8.0);
                if widgets::check(
                    ui,
                    &mut self.settings.hud_collectibles,
                    tr(lang, "hud_collectibles"),
                    tr(lang, "hud_collectibles_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                if widgets::check(
                    ui,
                    &mut self.settings.hide_minimap,
                    tr(lang, "hide_minimap"),
                    tr(lang, "hide_minimap_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(tr(lang, "hpbars_title"))
                        .size(13.0)
                        .strong(),
                );
                if widgets::check(
                    ui,
                    &mut self.settings.enemy_bars,
                    tr(lang, "enemy_bars_on"),
                    tr(lang, "enemy_bars_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                if widgets::check(
                    ui,
                    &mut self.settings.boss_bar,
                    tr(lang, "boss_bar_on"),
                    tr(lang, "boss_bar_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                ui.label(
                    egui::RichText::new(tr(lang, "hpbars_note"))
                        .size(12.0)
                        .color(theme::DIM),
                );
                ui.add_space(8.0);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if widgets::percent_slider(
                        ui,
                        tr(lang, "hud_count_scale"),
                        &mut self.settings.hud_count_scale,
                        50,
                        200,
                        5,
                    ) {
                        changed = true;
                    }
                    widgets::help_icon(ui, "hud_count_scale_desc", lang, &mut help);
                });
                ui.label(
                    egui::RichText::new(tr(lang, "hud_note"))
                        .size(12.0)
                        .color(theme::DIM),
                );
            },
        );
        ui.add_space(10.0);
        theme::section(
            ui,
            tr(lang, "game_layout"),
            Some(tr(lang, "game_layout_d")),
            |ui| {
                let options = vec![
                    (GameLayout::None, tr(lang, "game_layout_none").to_string()),
                    (
                        GameLayout::PlayStation,
                        tr(lang, "g_playstation").to_string(),
                    ),
                    (GameLayout::Xbox, tr(lang, "g_xbox").to_string()),
                ];
                let gamecube = self.settings.bm_layout == BmLayout::Gamecube;
                ui.add_enabled_ui(!gamecube, |ui| {
                    if widgets::combo_row(
                        ui,
                        "",
                        "game-layout",
                        &mut self.settings.game_layout,
                        &options,
                        180.0,
                    ) {
                        changed = true;
                    }
                });
                ui.add_space(4.0);
                if gamecube {
                    ui.label(
                        egui::RichText::new(tr(lang, "bm_place_gamecube_help"))
                            .size(12.0)
                            .color(theme::DIM),
                    );
                } else {
                    match self.settings.game_layout {
                        GameLayout::None => {
                            ui.label(
                                egui::RichText::new(tr(lang, "layout_note_none"))
                                    .size(12.0)
                                    .color(theme::DIM),
                            );
                        }
                        style => {
                            let name = if style == GameLayout::PlayStation {
                                tr(lang, "g_playstation")
                            } else {
                                tr(lang, "g_xbox")
                            };
                            if shb_core::layouts::available(style.as_str()) {
                                ui.label(
                                    egui::RichText::new(fill(
                                        tr(lang, "layout_note_ready"),
                                        &[("a", name)],
                                    ))
                                    .size(12.0)
                                    .color(theme::ACCENT),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new(fill(
                                        tr(lang, "layout_note_missing"),
                                        &[("a", name)],
                                    ))
                                    .size(12.0)
                                    .color(theme::AMBER),
                                );
                            }
                        }
                    }
                }
            },
        );
        self.help = help;
        changed
    }
    fn page_misc(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        let mut help = self.help;
        theme::section(
            ui,
            tr(lang, "misc_title"),
            Some(tr(lang, "misc_desc")),
            |ui| {
                ui.label(
                    egui::RichText::new(tr(lang, "songbook_title"))
                        .size(13.0)
                        .strong()
                        .color(theme::ACCENT),
                );
                if widgets::check(
                    ui,
                    &mut self.settings.songbook,
                    tr(lang, "songbook_on"),
                    tr(lang, "songbook_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                ui.label(
                    egui::RichText::new(tr(lang, "songbook_note"))
                        .size(12.0)
                        .color(theme::AMBER),
                );
                ui.add_space(8.0);
                if widgets::check(
                    ui,
                    &mut self.settings.quick_boots,
                    tr(lang, "quick_boots_on"),
                    tr(lang, "quick_boots_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                if widgets::check(
                    ui,
                    &mut self.settings.quick_mask,
                    tr(lang, "quick_mask_on"),
                    tr(lang, "quick_mask_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                ui.label(
                    egui::RichText::new(tr(lang, "quick_note"))
                        .size(12.0)
                        .color(theme::DIM),
                );
            },
        );
        ui.add_space(10.0);
        theme::section(
            ui,
            tr(lang, "world_beta_title"),
            Some(tr(lang, "world_beta_desc")),
            |ui| {
                ui.label(
                    egui::RichText::new(tr(lang, "great_fairy"))
                        .size(13.0)
                        .strong()
                        .color(theme::ACCENT),
                );
                if widgets::check(
                    ui,
                    &mut self.settings.great_fairy,
                    tr(lang, "great_fairy_on"),
                    tr(lang, "great_fairy_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(tr(lang, "farore"))
                        .size(13.0)
                        .strong()
                        .color(theme::ACCENT),
                );
                if widgets::check(
                    ui,
                    &mut self.settings.farore_outdoor,
                    tr(lang, "farore_on"),
                    tr(lang, "farore_desc"),
                    None,
                    &mut help,
                ) {
                    changed = true;
                }
            },
        );
        self.help = help;
        changed
    }
    fn page_about(&mut self, ui: &mut egui::Ui) {
        let lang = self.ui_lang;
        let mut action: Option<Update> = None;
        section(ui, tr(lang, "about_title"), None, |ui| {
            match &self.update {
                UpdateState::Idle => {
                    if ghost_button(ui, tr(lang, "update_check")).clicked() {
                        self.start_check();
                    }
                }
                UpdateState::Checking => {
                    ui.label(tr(lang, "update_checking"));
                }
                UpdateState::UpToDate => {
                    ui.label(fill(tr(lang, "update_latest"), &[("v", shb_core::VERSION)]));
                    if ghost_button(ui, tr(lang, "update_check")).clicked() {
                        self.start_check();
                    }
                }
                UpdateState::Available(found) => {
                    let found = found.clone();
                    ui.label(
                        egui::RichText::new(fill(
                            tr(lang, "update_banner"),
                            &[("v", &found.tag), ("cur", shb_core::VERSION)],
                        ))
                        .color(theme::AMBER),
                    );
                    ui.horizontal(|ui| {
                        if primary_button(ui, tr(lang, "update_download")).clicked() {
                            action = Some(found.clone());
                        }
                        if ghost_button(ui, tr(lang, "update_later")).clicked() {
                            self.update = UpdateState::Idle;
                        }
                    });
                }
                UpdateState::Downloading { done, total } => {
                    let bytes = done.load(Ordering::Relaxed);
                    let fraction = if *total > 0 {
                        bytes as f32 / *total as f32
                    } else {
                        0.0
                    };
                    ui.add(egui::ProgressBar::new(fraction).text(format!(
                        "{} {:.0}%",
                        tr(lang, "update_downloading").replace("{p}", ""),
                        fraction * 100.0
                    )));
                }
                UpdateState::Ready(path) => {
                    ui.label(tr(lang, "update_ready"));
                    ui.label(
                        egui::RichText::new(path.display().to_string())
                            .size(12.0)
                            .color(theme::DIM),
                    );
                    ui.horizontal(|ui| {
                        if primary_button(ui, tr(lang, "update_download")).clicked() {
                            let path = path.clone();
                            if !update::apply_and_restart(&path) {
                                update::reveal(&path);
                            }
                        }
                        if ghost_button(ui, "…").clicked() {
                            update::reveal(path);
                        }
                    });
                }
                UpdateState::Failed(err) => {
                    ui.label(egui::RichText::new(err).color(theme::RED));
                    ui.label(tr(lang, "update_failed"));
                    ui.horizontal(|ui| {
                        if ghost_button(ui, tr(lang, "update_check")).clicked() {
                            self.start_check();
                        }
                        if ghost_button(ui, "GitHub").clicked() {
                            update::open_releases();
                        }
                    });
                }
            }
            ui.add_space(8.0);
            ui.label(egui::RichText::new(tr(lang, "about_body")).color(theme::TEXT));
            ui.add_space(6.0);
            ui.label(egui::RichText::new(tr(lang, "about_link")).color(theme::DIM));
            ui.label(
                egui::RichText::new(format!("{} — {}", shb_core::VERSION, find::TITLE_ID))
                    .color(theme::DIM),
            );
        });
        if let Some(found) = action {
            self.start_download(found);
        }
    }
}

/// Whether the card was clicked: the pointer is inside its rectangle, the click is a press, and the "?" (a button of
/// its own inside the card) is not what was hit. Also asks for the hand cursor over the card.
fn card_pressed(ui: &egui::Ui, card: egui::Rect, help: Option<egui::Rect>) -> bool {
    let Some(pos) = ui.input(|i| i.pointer.interact_pos().or_else(|| i.pointer.hover_pos())) else {
        return false;
    };
    if help.is_some_and(|rect| rect.expand(2.0).contains(pos)) || !card.contains(pos) {
        return false;
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    ui.input(|i| i.pointer.primary_clicked())
}

/// Whether the settings are exactly this preset: its table with the user's own long presses, *and* the face-button
/// layout it sets. Without the layout, "Switch GameCube" (the classic table with the GameCube positions) marked the
/// classic card too, and the first card of the list won.
fn preset_matches(s: &shb_core::settings::Settings, key: &str) -> bool {
    let table_key = if key == "gamecube" { "classic" } else { key };
    let Some(table) = shb_core::roles::remap_preset(table_key) else {
        return false;
    };
    let roles = shb_core::roles::keep_holds(&table, &s.bm_roles) == s.bm_roles;
    let gamecube = s.bm_layout == shb_core::settings::BmLayout::Gamecube;
    roles && (gamecube == (key == "gamecube"))
}

/// What a running action sends to the window.
#[derive(Debug, Clone)]
enum Busy {
    /// A line of the builder's own output.
    Line(String),
    /// Something this app says about the build (the builder it chose, DLSS kept, …).
    Note(String),
    /// How far it is, in which stage, and what it is doing.
    Progress {
        fraction: f32,
        stage: String,
        text: String,
    },
    /// The end: the error, when there was one, and what was being done.
    Done(Option<String>, shb_core::rebuild::Mode),
}

/// The text key of an action, for the button and for the log.
fn action_name(mode: shb_core::rebuild::Mode) -> &'static str {
    match mode {
        shb_core::rebuild::Mode::Install => "install",
        shb_core::rebuild::Mode::Recompile => "recomp_recompile",
        shb_core::rebuild::Mode::Settings => "recomp_settings",
        shb_core::rebuild::Mode::Reinstall => "recomp_reinstall",
    }
}

/// The text key of what an action did, when it worked.
fn action_done(mode: shb_core::rebuild::Mode) -> &'static str {
    match mode {
        shb_core::rebuild::Mode::Install => "install_ok",
        shb_core::rebuild::Mode::Recompile => "recomp_ok",
        shb_core::rebuild::Mode::Settings => "settings_ok",
        shb_core::rebuild::Mode::Reinstall => "reinstall_ok",
    }
}

/// Whether a click outside the help window closes it.
///
/// The click that opens the window is, by definition, outside the window that did not exist yet when it landed, so the
/// first frame after opening is ignored: without that, every "?" looked dead.
fn closes_help(frame: u64, opened_on: u64, clicked_outside: bool) -> bool {
    clicked_outside && frame > opened_on + 1
}

/// Open the window. This is the whole application when it is started without arguments; the CLI calls it for `--gui`
/// and when nothing else was asked for, so one executable does both.
pub fn run() -> eframe::Result {
    // `--version` runs before any window work, which is also how a broken graphics stack is told apart from a
    // broken binary.
    if std::env::args()
        .skip(1)
        .any(|a| a == "--version" || a == "-V")
    {
        println!("shb-gui {}", shb_core::VERSION);
        return Ok(());
    }
    // `SHB_WINDOW=1400x1800` opens a window that tall: how the whole of a page is captured for the docs.
    let size = std::env::var("SHB_WINDOW")
        .ok()
        .and_then(|text| {
            let (w, h) = text.split_once('x')?;
            Some([w.trim().parse().ok()?, h.trim().parse().ok()?])
        })
        .unwrap_or([1060.0, 820.0]);
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size([800.0, 560.0])
        .with_title("SHB-OcarinaTweaks");
    if let Some(icon) = theme::icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "SHB-OcarinaTweaks",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::{closes_help, preset_matches, role_choices};
    use shb_core::roles::{Button, ITEM_ROLES, Role};

    #[test]
    fn the_item_selector_stays_with_the_custom_item_hud_off() {
        // The custom item HUD takes the item roles away; the selector is a role of the remap that only presses the
        // game's own selector key, so it is offered either way.
        let with_items = role_choices(Button::B, Role::None, true, true, false, false);
        let without_items = role_choices(Button::B, Role::None, true, false, false, false);
        assert!(
            without_items.contains(&Role::Selector),
            "the selector is not an item"
        );
        assert!(with_items.contains(&Role::Selector));
        assert!(
            without_items.iter().all(|r| !ITEM_ROLES.contains(r)),
            "the item roles go with the item HUD"
        );
        assert!(with_items.iter().filter(|r| ITEM_ROLES.contains(r)).count() == ITEM_ROLES.len());
        // A table made before keeps the role the button already has, so it is not silently rewritten.
        assert!(
            role_choices(Button::B, Role::Item1, true, false, false, false).contains(&Role::Item1)
        );
        // The mapping is what the remap itself rides on: off, its own roles go too (the whole card is off).
        let no_mapping = role_choices(Button::B, Role::None, false, true, false, false);
        assert!(!no_mapping.contains(&Role::Selector));
    }

    #[test]
    fn the_click_that_opens_a_help_window_does_not_close_it() {
        // The click lands on the "?" before the window exists: that frame and the next are ignored.
        assert!(!closes_help(10, 10, true));
        assert!(!closes_help(11, 10, true));
        // Any later click outside does close it, and one inside never does.
        assert!(closes_help(12, 10, true));
        assert!(!closes_help(12, 10, false));
    }

    #[test]
    fn the_gamecube_preset_is_the_classic_table_with_its_own_layout() {
        use shb_core::settings::{BmLayout, Settings};
        let classic = shb_core::roles::remap_preset("classic").unwrap();
        let mut s = Settings {
            bm_roles: classic,
            bm_layout: BmLayout::Nintendo,
            ..Settings::default()
        };
        assert!(
            preset_matches(&s, "classic"),
            "the classic table with Nintendo positions is Clasico"
        );
        assert!(!preset_matches(&s, "gamecube"), "and not Switch GameCube");
        s.bm_layout = BmLayout::Gamecube;
        assert!(
            preset_matches(&s, "gamecube"),
            "the same table with GameCube positions is Switch GameCube"
        );
        assert!(!preset_matches(&s, "classic"), "and not Clasico any more");
    }
}
