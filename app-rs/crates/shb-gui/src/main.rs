//! The window. Same pages, same palette, same two fonts and the same texts as the Python app (`app/ui_kit.py`,
//! `app/i18n.py`).
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
use shb_core::settings::{BmLayout, ButtonStyle, GameLayout, HudMode, LMode, Lang, Settings};
use shb_core::update::{self, Update};
use shb_core::{paths, payload_root};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use theme::{ghost_button, primary_button, section, title};

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
    Help,
    About,
}

impl Page {
    const ALL: [Page; 9] = [
        Page::Install,
        Page::Presets,
        Page::Gameplay,
        Page::Difficulty,
        Page::Controls,
        Page::Hud,
        Page::Misc,
        Page::Help,
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
            Page::Help => "help",
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
            Page::Help => "help_title",
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

/// The stamina modes of the config block, with the Python app's names.
const PRESET_KEYS: [&str; 4] = ["m_hardcore", "m_combat", "m_sprint", "m_advanced"];

struct App {
    page: Page,
    target: Target,
    emu: String,
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
    /// What the user typed in the help page's search box.
    help_search: String,
    update_rx: Option<Receiver<UpdateMsg>>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let (settings, _) = paths::load_settings();
        let emu = std::env::var("SHB_EMU").unwrap_or_default();
        // The window speaks the system language when it is one of ours, like the Python app.
        let ui_lang = system_lang();
        let mut app = App {
            // `SHB_PAGE=difficulty` opens straight on that page: how the screenshots of this window are taken.
            page: Page::from_env(),
            target: Target::Emulator,
            emu,
            settings,
            profile: presets::DEFAULT_PROFILE.to_string(),
            ui_lang,
            payload: payload_root(None),
            log: Vec::new(),
            notes: Vec::new(),
            failed: false,
            logo: theme::logo(&cc.egui_ctx),
            status: None,
            update: UpdateState::Idle,
            help: None,
            help_search: String::new(),
            update_rx: None,
        };
        match app.payload.clone() {
            Some(path) => app.say(format!("payload: {}", path.display())),
            None => app.say(tr(app.ui_lang, "e_no_payload")),
        }
        app.refresh_emulator();
        app
    }

    fn t(&self, key: &'static str) -> &'static str {
        tr(self.ui_lang, key)
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
        let emu = PathBuf::from(self.emu.trim());
        let info = install::inspect(&emu);
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
            self.say(format!("emulator: {}", exe.display()));
        }
    }

    fn apply_profile(&mut self) {
        // A profile sets the gameplay options; the style, the layout and the language stay the user's own.
        let mut fresh = presets::settings_for(&self.profile);
        fresh.button_style = self.settings.button_style;
        fresh.bm_layout = self.settings.bm_layout;
        fresh.game_layout = self.settings.game_layout;
        fresh.bm_l_mode = self.settings.bm_l_mode;
        fresh.lang = self.settings.lang;
        fresh.hud_ctrl_scale = self.settings.hud_ctrl_scale;
        fresh.hud_count_scale = self.settings.hud_count_scale;
        self.settings = fresh;
        let line = format!("{} {}", self.t("profile"), self.profile);
        self.say(line);
    }

    fn install(&mut self) {
        match self.target {
            Target::Emulator => self.install_emulator(),
            Target::Recomp => self.install_game(),
            Target::Rekarina => self.install_rekarina(),
        }
    }

    /// A Rekarina install: the settings half only — the module's config block, the hash `Rekarina.exe` stores for it,
    /// the romfs files and our mod first in `mods_order`. The lift and the compile are Rekarina's build.
    fn install_rekarina(&mut self) {
        let Some(payload) = self.payload.clone() else {
            self.failed = true;
            self.notes = vec![self.t("e_no_payload").to_string()];
            return;
        };
        let path = PathBuf::from(self.emu.trim());
        let Some(found) = shb_core::rekarina::find(&path) else {
            self.failed = true;
            self.notes = vec![format!("{} {}", self.t("rekarina_missing"), path.display())];
            return;
        };
        match shb_core::rekarina::apply_settings(&found, &self.settings, &payload) {
            Ok(()) => {
                self.failed = false;
                self.notes = vec![
                    format!("{} (rekarina)", found.root.display()),
                    self.t("rekarina_built").to_string(),
                ];
                let line = format!("{} {}", self.t("installed"), found.root.display());
                self.say(line);
            }
            Err(err) => {
                self.failed = true;
                self.notes = vec![format!("{err}")];
            }
        }
    }

    /// A recompiled game (Longshot / Hookshot / ReLink): the module, `image.bin` and the romfs files.
    fn install_game(&mut self) {
        let Some(payload) = self.payload.clone() else {
            self.failed = true;
            self.notes = vec![self.t("e_no_payload").to_string()];
            return;
        };
        let path = PathBuf::from(self.emu.trim());
        let Some(found) = shb_core::game::find(&path) else {
            self.failed = true;
            self.notes = vec![if shb_core::game::looks_like_rekarina(&path) {
                self.t("e_rekarina_needs_build").to_string()
            } else {
                format!("{} {}", self.t("recomp_missing"), path.display())
            }];
            return;
        };
        match shb_core::game::apply_settings(&found, &self.settings, &payload) {
            Ok(image) => {
                self.failed = false;
                self.notes = vec![
                    format!(
                        "{} ({}, {})",
                        found.root.display(),
                        found.flavor.name(),
                        found.exefs.display()
                    ),
                    if image {
                        self.t("recomp_applied").to_string()
                    } else {
                        self.t("recomp_needs_build").to_string()
                    },
                ];
                let line = format!("{} {}", self.t("installed"), found.root.display());
                self.say(line);
            }
            Err(err) => {
                self.failed = true;
                self.notes = vec![format!("error: {err}")];
            }
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
                self.failed = false;
                self.notes = result.notes;
                let line = format!("{} {}", self.t("installed"), result.target.display());
                self.say(line);
                self.refresh_emulator();
            }
            Err(err) => {
                self.failed = true;
                self.notes = vec![format!("error: {err}")];
            }
        }
    }

    fn uninstall(&mut self) {
        if self.emu.trim().is_empty() {
            return;
        }
        let emu = PathBuf::from(self.emu.trim());
        if self.target == Target::Rekarina
            && let Some(found) = shb_core::rekarina::find(&emu)
        {
            match shb_core::rekarina::uninstall(&found) {
                Ok(true) => {
                    self.notes.clear();
                    self.failed = false;
                    self.say(self.t("uninstalled"));
                }
                Ok(false) => self.say(self.t("nothing")),
                Err(err) => self.say(format!("error: {err}")),
            }
            return;
        }
        if self.target == Target::Recomp
            && let Some(found) = shb_core::game::find(&emu)
        {
            match shb_core::game::uninstall(&found) {
                Ok(true) => {
                    self.notes.clear();
                    self.failed = false;
                    self.say(self.t("uninstalled"));
                }
                Ok(false) => self.say(self.t("nothing")),
                Err(err) => self.say(format!("error: {err}")),
            }
            return;
        }
        match install::uninstall(&emu) {
            Ok(true) => {
                self.notes.clear();
                self.failed = false;
                self.say(self.t("uninstalled"));
                self.refresh_emulator();
            }
            Ok(false) => self.say(self.t("nothing")),
            Err(err) => self.say(format!("error: {err}")),
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

/// The label of a row of the editor: the button's glyph, or the long press of a face button.
fn button_label(button: Button) -> &'static str {
    match button {
        Button::AHold => "A ↓",
        Button::BHold => "B ↓",
        Button::XHold => "X ↓",
        Button::YHold => "Y ↓",
        Button::Dr => "▶",
        other => other.as_str(),
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

/// One role on one button: the role that was there moves to the button that had the new one, so every role stays on
/// a single button (the module's rule, `core/remap.h`).
fn take_role(current: &Roles, button: Button, picked: Role) -> Roles {
    let mut table = current.clone();
    let old = table.get(&button).copied().unwrap_or(Role::None);
    if picked != Role::None && picked != old {
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

/// The title key of a help topic.
fn help_title_key(topic: &str) -> &'static str {
    match topic {
        "start" => "help_start_title",
        "install" => "help_install_title",
        "controls" => "help_controls_title",
        "rekarina" => "help_rekarina_title",
        _ => "help_problems_title",
    }
}

/// The body key of a help topic.
fn help_body_key(topic: &str) -> &'static str {
    match topic {
        "start" => "help_start_body",
        "install" => "help_install_body",
        "controls" => "help_controls_body",
        "rekarina" => "help_rekarina_body",
        _ => "help_problems_body",
    }
}

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

        egui::Panel::top("header")
            .frame(
                egui::Frame::NONE
                    .fill(theme::SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(logo) = &self.logo {
                        let height = 40.0;
                        let size = logo.size_vec2();
                        let width = height * size.x / size.y.max(1.0);
                        ui.image((logo.id(), egui::vec2(width, height)));
                        ui.add_space(6.0);
                    }
                    ui.vertical(|ui| {
                        ui.label(title(24.0, tr(lang, "app_title")));
                        ui.label(
                            egui::RichText::new(tr(lang, "subtitle"))
                                .size(12.0)
                                .color(theme::DIM),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("ui-lang")
                            .selected_text(lang_name(self.ui_lang))
                            .width(120.0)
                            .show_ui(ui, |ui| {
                                for code in Lang::ALL {
                                    ui.selectable_value(&mut self.ui_lang, *code, lang_name(*code));
                                }
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
                        chip(ui, &fill(tr(lang, "profile_current"), &[("a", &name)]));
                    });
                });
            });

        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(196.0)
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
                    if ui.add_sized([ui.available_width(), 34.0], button).clicked() {
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
                    if primary_button(ui, tr(lang, "install")).clicked() {
                        self.install();
                    }
                    if ghost_button(ui, tr(lang, "uninstall")).clicked() {
                        self.uninstall();
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
                        Page::Help => self.page_help(ui),
                        Page::About => self.page_about(ui),
                    });
            });

        if let Some(key) = self.help {
            let mut open = true;
            let text = tr(lang, key).to_string();
            let title = text.split('.').next().unwrap_or(&text).to_string();
            let window = egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .default_width(560.0);
            window.show(ui.ctx(), |ui| {
                ui.set_max_width(540.0);
                ui.label(egui::RichText::new(text).size(13.0).color(theme::TEXT));
            });
            if !open {
                self.help = None;
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
                            self.target = target;
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
                    ui.add(
                        egui::TextEdit::singleline(&mut self.emu)
                            .desired_width(ui.available_width() - 110.0)
                            .hint_text("E:/Switch2/Emu/…"),
                    );
                    if ghost_button(ui, tr(lang, "browse")).clicked() {
                        self.refresh_emulator();
                    }
                });
                if let Some(status) = &self.status {
                    ui.label(egui::RichText::new(status).size(12.0).color(theme::TEAL));
                }
                if self.target != Target::Emulator {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {} / {}",
                            tr(lang, "target"),
                            tr(lang, "target_recomp"),
                            tr(lang, "target_rekarina")
                        ))
                        .size(12.0)
                        .color(theme::AMBER),
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
                for key in PROFILE_KEYS {
                    let selected = self.profile == *key;
                    let name = profile_name(lang, key);
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
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(12, 8));
                    frame.show(ui, |ui| {
                        ui.set_width(width - 26.0);
                        ui.horizontal(|ui| {
                            let text =
                                egui::RichText::new(name)
                                    .size(14.0)
                                    .strong()
                                    .color(if selected {
                                        theme::ON_ACCENT
                                    } else {
                                        theme::TEXT
                                    });
                            if ui
                                .add(egui::Label::new(text).sense(egui::Sense::click()))
                                .clicked()
                            {
                                self.profile = (*key).to_string();
                            }
                            if !shown.is_empty() && shown != full {
                                widgets::help_icon(ui, key_desc, lang, &mut help);
                            }
                            if selected {
                                ui.label(
                                    egui::RichText::new(tr(lang, "profile_current"))
                                        .size(12.0)
                                        .color(theme::ON_ACCENT),
                                );
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(&shown).size(12.0).color(if selected {
                                theme::ON_ACCENT
                            } else {
                                theme::DIM
                            }));
                        });
                    });
                    ui.add_space(4.0);
                }
                ui.add_space(4.0);
                if primary_button(ui, tr(lang, "reapply")).clicked() {
                    self.apply_profile();
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(tr(lang, "profile_shield_note"))
                        .size(12.0)
                        .color(theme::DIM),
                );
            },
        );
        self.help = help;
        false
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
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_value(
                                    &mut self.settings.stamina_bar,
                                    false,
                                    tr(lang, "stamina_ring"),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            widgets::help_icon(ui, "stamina_ring_d", lang, &mut help);
                            if ui
                                .selectable_value(
                                    &mut self.settings.stamina_bar,
                                    true,
                                    tr(lang, "stamina_bar"),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            widgets::help_icon(ui, "stamina_bar_d", lang, &mut help);
                        });
                        let ring = widgets::short(tr(lang, "stamina_ring_d"));
                        widgets::desc_under(ui, &ring);
                        let bar = widgets::short(tr(lang, "stamina_bar_d"));
                        widgets::desc_under(ui, &bar);
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(tr(lang, "stamina_mode"))
                                .size(13.0)
                                .strong(),
                        );
                        for (value, key) in PRESET_KEYS.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_value(
                                        &mut self.settings.preset,
                                        value as u8,
                                        tr(lang, key),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                                let desc = format!("{key}_d");
                                widgets::help_icon(ui, preset_desc_key(value), lang, &mut help);
                                let _ = desc;
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
                ui.add_space(8.0);
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

        // The presets: one card per table, the way the Python app opens the page.
        theme::section(
            ui,
            tr(lang, "ctrl_preset_title"),
            Some(tr(lang, "ctrl_preset_desc")),
            |ui| {
                egui::Grid::new("remap-presets")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        for (i, key) in [
                            "classic",
                            "action_down",
                            "shield",
                            "souls",
                            "triggers",
                            "gamecube",
                        ]
                        .iter()
                        .enumerate()
                        {
                            let name = tr(lang, preset_name_key(key));
                            let desc = tr(lang, preset_desc_key_for(key));
                            let selected = shb_core::roles::remap_preset(key)
                                .map(|table| {
                                    shb_core::roles::keep_holds(&table, &self.settings.bm_roles)
                                        == self.settings.bm_roles
                                })
                                .unwrap_or(false);
                            let width = (ui.available_width() * 0.5 - 14.0).clamp(220.0, 420.0);
                            ui.vertical(|ui| {
                                let button = egui::Button::new(
                                    egui::RichText::new(name)
                                        .color(if selected {
                                            theme::ON_ACCENT
                                        } else {
                                            theme::TEXT
                                        })
                                        .strong(),
                                )
                                .fill(if selected {
                                    theme::ACCENT
                                } else {
                                    theme::PANEL2
                                })
                                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                                .corner_radius(egui::CornerRadius::same(10))
                                .min_size(egui::vec2(width, 0.0));
                                if ui.add(button).clicked() {
                                    if *key == "gamecube" {
                                        self.settings.bm_layout = BmLayout::Gamecube;
                                        self.settings.button_style = ButtonStyle::Switch;
                                        self.settings.bm_simple = false;
                                        self.settings.button_mapping = true;
                                    } else if self.settings.bm_layout == BmLayout::Gamecube {
                                        self.settings.bm_layout = BmLayout::Nintendo;
                                    }
                                    let table_key =
                                        if *key == "gamecube" { "classic" } else { key };
                                    if let Some(table) = shb_core::roles::remap_preset(table_key) {
                                        let table = shb_core::roles::keep_holds(
                                            &table,
                                            &self.settings.bm_roles,
                                        );
                                        if table.values().any(|r| *r == Role::Shield) {
                                            self.settings.manual_shield = true;
                                        }
                                        self.settings.bm_roles = table;
                                        changed = true;
                                    }
                                }
                                ui.add_space(2.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.set_max_width(width);
                                    ui.label(
                                        egui::RichText::new(desc).size(12.0).color(theme::DIM),
                                    );
                                });
                            });
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    widgets::help_icon(ui, "rm_help", lang, &mut help);
                    ui.label(
                        egui::RichText::new(widgets::short(tr(lang, "rm_help")))
                            .size(12.0)
                            .color(theme::DIM),
                    );
                });
            },
        );

        // The buttons themselves: on/off, the pictures, the face-button positions.
        ui.add_space(10.0);
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
                ui.add_space(4.0);
                // The pictures of the HUD buttons: Switch, PlayStation or Xbox.
                ui.horizontal(|ui| {
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
                        150.0,
                    ) {
                        changed = true;
                    }
                    widgets::help_icon(ui, "variant_d", lang, &mut help);
                });
                ui.add_space(4.0);
                // How many buttons the mod draws: the assignable ones only, or the extended set.
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(tr(lang, "bm_layout")).size(13.0));
                    if ui
                        .selectable_value(&mut self.settings.bm_simple, false, tr(lang, "bm_ext"))
                        .changed()
                    {
                        changed = true;
                    }
                    widgets::help_icon(ui, "bm_ext_d", lang, &mut help);
                    if ui
                        .selectable_value(&mut self.settings.bm_simple, true, tr(lang, "bm_simple"))
                        .changed()
                    {
                        changed = true;
                    }
                    widgets::help_icon(ui, "bm_simple_d", lang, &mut help);
                });
                ui.add_space(4.0);
                // The face-button positions, the same scheme as the recompiled games (GameCube included).
                ui.horizontal(|ui| {
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
                        230.0,
                    ) {
                        changed = true;
                    }
                    widgets::help_icon(ui, "bm_place_d", lang, &mut help);
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let lmodes = vec![
                        (LMode::Assign, tr(lang, "bm_l_assign").to_string()),
                        (LMode::Selector, tr(lang, "bm_l_selector").to_string()),
                    ];
                    if widgets::combo_row(
                        ui,
                        tr(lang, "bm_l"),
                        "bm-l",
                        &mut self.settings.bm_l_mode,
                        &lmodes,
                        190.0,
                    ) {
                        changed = true;
                    }
                });
                ui.add_space(4.0);
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
        ui.add_space(10.0);
        theme::section(ui, tr(lang, "rm_title"), Some(tr(lang, "rm_help")), |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    let items = self
                        .settings
                        .bm_roles
                        .values()
                        .filter(|r| {
                            matches!(r, Role::Item1 | Role::Item2 | Role::Item3 | Role::ItemMain)
                        })
                        .count();
                    ui.label(
                        egui::RichText::new(fill(
                            tr(lang, "rm_items"),
                            &[("n", &items.to_string())],
                        ))
                        .size(12.0)
                        .color(theme::DIM),
                    );
                    let rows: [Button; 15] = [
                        Button::X,
                        Button::XHold,
                        Button::Y,
                        Button::YHold,
                        Button::A,
                        Button::AHold,
                        Button::B,
                        Button::BHold,
                        Button::L,
                        Button::R,
                        Button::Zl,
                        Button::Zr,
                        Button::L3,
                        Button::R3,
                        Button::Dr,
                    ];
                    for button in rows {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(button_label(button))
                                    .size(12.0)
                                    .strong()
                                    .color(theme::ACCENT),
                            );
                            ui.add_space(4.0);
                            let current = self
                                .settings
                                .bm_roles
                                .get(&button)
                                .copied()
                                .unwrap_or(Role::None);
                            let mut picked = current;
                            let choices: Vec<(Role, String)> = Role::ALL
                                .iter()
                                .copied()
                                .filter(|role| shb_core::roles::role_allowed(*role, button))
                                .map(|role| (role, into_role_name(lang, role)))
                                .collect();
                            if widgets::combo_row(
                                ui,
                                "",
                                &format!("role-{button:?}"),
                                &mut picked,
                                &choices,
                                190.0,
                            ) {
                                self.settings.bm_roles =
                                    take_role(&self.settings.bm_roles, button, picked);
                                changed = true;
                            }
                            if let Some(key) = role_help_key(picked) {
                                widgets::help_icon(ui, key, lang, &mut help);
                            }
                        });
                        // The options of the role, on its own line (target: shield, next one; sprint: toggle).
                        if self.settings.bm_roles.get(&button) == Some(&Role::Target) {
                            ui.horizontal(|ui| {
                                ui.add_space(34.0);
                                if widgets::check(
                                    ui,
                                    &mut self.settings.manual_shield,
                                    tr(lang, "opt_shield"),
                                    "",
                                    Some("opt_shield_help"),
                                    &mut help,
                                ) {
                                    changed = true;
                                }
                                if widgets::check(
                                    ui,
                                    &mut self.settings.switch_target,
                                    tr(lang, "opt_switch"),
                                    "",
                                    Some("opt_switch_help"),
                                    &mut help,
                                ) {
                                    changed = true;
                                }
                                if widgets::check(
                                    ui,
                                    &mut self.settings.auto_target,
                                    tr(lang, "opt_auto"),
                                    "",
                                    Some("opt_auto_help"),
                                    &mut help,
                                ) {
                                    changed = true;
                                }
                            });
                        } else if self.settings.bm_roles.get(&button) == Some(&Role::Sprint) {
                            ui.horizontal(|ui| {
                                ui.add_space(34.0);
                                if widgets::check(
                                    ui,
                                    &mut self.settings.sprint_l3_toggle,
                                    tr(lang, "opt_toggle"),
                                    "",
                                    Some("opt_toggle_help"),
                                    &mut help,
                                ) {
                                    changed = true;
                                }
                            });
                        }
                    }
                    let missing: Vec<String> =
                        shb_core::roles::roles_missing(&self.settings.bm_roles)
                            .iter()
                            .map(|role| into_role_name(lang, *role))
                            .collect();
                    if !missing.is_empty() {
                        ui.label(
                            egui::RichText::new(fill(
                                tr(lang, "rm_missing"),
                                &[("a", &missing.join(", "))],
                            ))
                            .size(12.0)
                            .color(theme::AMBER),
                        );
                    }
                    if self.settings.manual_shield
                        && !self.settings.bm_roles.values().any(|r| *r == Role::Shield)
                    {
                        ui.label(
                            egui::RichText::new(tr(lang, "rm_no_shield"))
                                .size(12.0)
                                .color(theme::AMBER),
                        );
                    }
                });
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(tr(lang, "rm_pad"))
                            .size(12.0)
                            .color(theme::DIM)
                            .strong(),
                    );
                    preview::pad(
                        ui,
                        &self.settings.bm_roles,
                        self.settings.button_style,
                        |role| into_role_name(lang, role),
                    );
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr(lang, "rm_hud"))
                                .size(12.0)
                                .color(theme::DIM)
                                .strong(),
                        );
                        widgets::help_icon(ui, "rm_hud_help", lang, &mut help);
                    });
                    preview::hud(
                        ui,
                        &self.settings.bm_roles,
                        self.settings.button_style,
                        self.settings.bm_simple,
                        self.settings.bm_layout == BmLayout::Gamecube,
                        |role| into_role_name(lang, role),
                    );
                });
            });
        });
        self.help = help;
        changed
    }

    fn page_help(&mut self, ui: &mut egui::Ui) {
        let lang = self.ui_lang;
        theme::section(
            ui,
            tr(lang, "help_title"),
            Some(tr(lang, "help_desc")),
            |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.help_search)
                        .hint_text(tr(lang, "help_search"))
                        .desired_width(ui.available_width().min(420.0)),
                );
                ui.add_space(6.0);
                let topics: [&str; 5] = ["start", "install", "controls", "rekarina", "problems"];
                let query = self.help_search.trim().to_lowercase();
                let mut shown = 0;
                for topic in topics {
                    let title = tr(lang, help_title_key(topic));
                    let body = tr(lang, help_body_key(topic));
                    if !query.is_empty()
                        && !title.to_lowercase().contains(&query)
                        && !body.to_lowercase().contains(&query)
                    {
                        continue;
                    }
                    shown += 1;
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(title)
                            .size(15.0)
                            .strong()
                            .color(theme::ACCENT),
                    );
                    for line in body.split('\n') {
                        ui.label(egui::RichText::new(line).size(13.0));
                    }
                }
                if shown == 0 {
                    ui.label(egui::RichText::new(tr(lang, "help_no_results")).color(theme::AMBER));
                }
            },
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
                egui::RichText::new(format!("Rust {} — {}", shb_core::VERSION, find::TITLE_ID))
                    .color(theme::DIM),
            );
            if let Some(payload) = &self.payload {
                ui.label(
                    egui::RichText::new(format!("payload: {}", payload.display()))
                        .color(theme::DIM),
                );
            }
        });
        if let Some(found) = action {
            self.start_download(found);
        }
    }
}

fn main() -> eframe::Result {
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
