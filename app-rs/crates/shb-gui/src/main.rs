//! The window. Same pages, same palette, same two fonts and the same texts as the Python app (`app/ui_kit.py`,
//! `app/i18n.py`).
//!
//! Everything goes through `shb-core`, so the window and `shb` (the CLI) cannot drift apart: the same settings
//! produce the same bytes. The window adds no logic of its own beyond remembering the last path.

// A windowed program, like the PyInstaller build of the Python app: no console in release. `--version` still
// prints, which is what the build scripts use.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod i18n;
mod theme;

use eframe::egui;
use i18n::{fill, lang_name, tr};
use shb_core::find;
use shb_core::install::{self, InstallRequest};
use shb_core::presets::{self, PROFILE_KEYS};
use shb_core::roles::{self, ROLE_KEYS, Role};
use shb_core::settings::{BmLayout, ButtonStyle, GameLayout, HudMode, LMode, Lang, Settings};
use shb_core::{enemy, paths, payload_root};
use std::path::PathBuf;
use theme::{ghost_button, primary_button, section, title};

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
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let (settings, _) = paths::load_settings();
        let emu = std::env::var("SHB_EMU").unwrap_or_default();
        // The window speaks the system language when it is one of ours, like the Python app.
        let ui_lang = system_lang();
        let mut app = App {
            page: Page::Install,
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
            Target::Rekarina => {
                self.failed = true;
                self.notes = vec![self.t("e_rekarina_needs_build").to_string()];
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

/// A checkbox that tells the caller whether the user changed it (any change means "custom").
fn toggle(ui: &mut egui::Ui, value: &mut bool, text: &str, changed: &mut bool) {
    if ui.checkbox(value, text).changed() {
        *changed = true;
    }
}

/// A row of choices, the way the Python app draws its segmented controls (`ChoiceCard`).
fn choice_row<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    label: &str,
    values: &[T],
    current: &mut T,
    text: impl Fn(T) -> &'static str,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        if !label.is_empty() {
            ui.label(label);
        }
        for value in values {
            if ui.selectable_value(current, *value, text(*value)).changed() {
                changed = true;
            }
        }
    });
    changed
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

/// One of the enemy components: an `Off`/`x1.5`/… dropdown.
fn enemy_level(ui: &mut egui::Ui, lang: Lang, label: &str, value: &mut u8) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(format!("enemy-{label}"))
            .selected_text(match enemy::factor(*value) {
                Some(factor) => format!("x{factor}"),
                None => tr(lang, "enemy_off").to_string(),
            })
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(value, 0, tr(lang, "enemy_off"))
                    .changed()
                {
                    changed = true;
                }
                for (level, factor) in enemy::LEVELS {
                    if ui
                        .selectable_value(value, level, format!("x{factor}"))
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
    });
    changed
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let lang = self.ui_lang;
        let mut changed = false;

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
                        Page::About => self.page_about(ui),
                    });
            });

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
        section(
            ui,
            tr(lang, "home_title"),
            Some(tr(lang, "home_desc")),
            |ui| {
                egui::Grid::new("profiles")
                    .num_columns(3)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        for (i, key) in PROFILE_KEYS.iter().enumerate() {
                            let selected = self.profile == *key;
                            let button =
                                egui::Button::new(egui::RichText::new(*key).color(if selected {
                                    theme::ON_ACCENT
                                } else {
                                    theme::TEXT
                                }))
                                .fill(if selected {
                                    theme::ACCENT
                                } else {
                                    theme::PANEL2
                                })
                                .stroke(egui::Stroke::new(1.0, theme::BORDER));
                            if ui.add_sized([170.0, 28.0], button).clicked() {
                                self.profile = (*key).to_string();
                            }
                            if i % 3 == 2 {
                                ui.end_row();
                            }
                        }
                    });
                ui.add_space(6.0);
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
        false
    }

    fn page_gameplay(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        section(
            ui,
            tr(lang, "stamina_title"),
            Some(tr(lang, "stamina_desc")),
            |ui| {
                toggle(
                    ui,
                    &mut self.settings.stamina,
                    tr(lang, "stamina_on"),
                    &mut changed,
                );
                ui.label(
                    egui::RichText::new(tr(lang, "stamina_on_desc"))
                        .size(12.0)
                        .color(theme::DIM),
                );
                ui.label(
                    egui::RichText::new(tr(lang, "stamina_display"))
                        .size(12.0)
                        .color(theme::DIM),
                );
                changed |= choice_row(
                    ui,
                    "",
                    &[false, true],
                    &mut self.settings.stamina_bar,
                    |bar| {
                        if bar {
                            tr(lang, "stamina_bar")
                        } else {
                            tr(lang, "stamina_ring")
                        }
                    },
                );
                ui.label(
                    egui::RichText::new(tr(lang, "stamina_mode"))
                        .size(12.0)
                        .color(theme::DIM),
                );
                ui.horizontal_wrapped(|ui| {
                    for (value, key) in PRESET_KEYS.iter().enumerate() {
                        if ui
                            .selectable_value(&mut self.settings.preset, value as u8, tr(lang, key))
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            },
        );
        ui.add_space(10.0);
        section(
            ui,
            tr(lang, "costs_title"),
            Some(tr(lang, "costs_desc")),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (value, key) in [
                        (&mut self.settings.jump, "a_jump"),
                        (&mut self.settings.attack, "a_attack"),
                        (&mut self.settings.sprint, "a_sprint"),
                        (&mut self.settings.block, "a_block"),
                        (&mut self.settings.shoot, "a_shoot"),
                        (&mut self.settings.roll, "a_roll"),
                    ] {
                        toggle(ui, value, tr(lang, key), &mut changed);
                    }
                });
                toggle(
                    ui,
                    &mut self.settings.combat_only,
                    tr(lang, "combat_only"),
                    &mut changed,
                );
            },
        );
        changed
    }

    fn page_difficulty(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        section(
            ui,
            tr(lang, "difficulty_title"),
            Some(tr(lang, "difficulty_desc")),
            |ui| {
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings.enemy_hp, 50..=400)
                            .text(tr(lang, "enemy_hp")),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings.damage, 50..=400)
                            .text(tr(lang, "damage")),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings.sprint_speed, 100..=200)
                            .text(tr(lang, "sprint_speed")),
                    )
                    .changed();
                toggle(
                    ui,
                    &mut self.settings.no_heart_drop,
                    tr(lang, "no_heart_drop"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.no_magic_drop,
                    tr(lang, "no_magic_drop"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.hide_minimap,
                    tr(lang, "hide_minimap"),
                    &mut changed,
                );
            },
        );
        ui.add_space(10.0);
        section(
            ui,
            tr(lang, "enemies_title"),
            Some(tr(lang, "enemies_desc")),
            |ui| {
                toggle(
                    ui,
                    &mut self.settings.enemy_double,
                    tr(lang, "enemy_double_on"),
                    &mut changed,
                );
                ui.add_space(4.0);
                ui.label(egui::RichText::new(tr(lang, "enemy_speed")).strong());
                changed |= enemy_level(
                    ui,
                    lang,
                    tr(lang, "enemy_move"),
                    &mut self.settings.enemy_move,
                );
                changed |= enemy_level(
                    ui,
                    lang,
                    tr(lang, "enemy_anim"),
                    &mut self.settings.enemy_anim,
                );
                changed |= enemy_level(
                    ui,
                    lang,
                    tr(lang, "enemy_attack"),
                    &mut self.settings.enemy_attack,
                );
                changed |= enemy_level(
                    ui,
                    lang,
                    tr(lang, "enemy_proj"),
                    &mut self.settings.enemy_proj,
                );
                changed |= enemy_level(
                    ui,
                    lang,
                    tr(lang, "enemy_alert"),
                    &mut self.settings.enemy_alert,
                );
                ui.label(egui::RichText::new(tr(lang, "boss_speed")).strong());
                changed |= enemy_level(
                    ui,
                    lang,
                    tr(lang, "enemy_move"),
                    &mut self.settings.boss_move,
                );
                changed |= enemy_level(
                    ui,
                    lang,
                    tr(lang, "enemy_anim"),
                    &mut self.settings.boss_anim,
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label(tr(lang, "enemy_aggro"));
                    for level in enemy::AGGRO_LEVELS {
                        let name = match level {
                            1 => tr(lang, "aggro_aggressive"),
                            2 => tr(lang, "aggro_max"),
                            _ => tr(lang, "aggro_normal"),
                        };
                        if ui
                            .selectable_value(&mut self.settings.enemy_aggro, level, name)
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            },
        );
        changed
    }

    fn page_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        section(
            ui,
            tr(lang, "buttons_title"),
            Some(tr(lang, "controls_desc")),
            |ui| {
                toggle(
                    ui,
                    &mut self.settings.button_mapping,
                    tr(lang, "bm_on"),
                    &mut changed,
                );
                changed |= choice_row(
                    ui,
                    tr(lang, "variant"),
                    ButtonStyle::ALL,
                    &mut self.settings.button_style,
                    |s| s.as_str(),
                );
                changed |= choice_row(
                    ui,
                    tr(lang, "bm_place"),
                    BmLayout::ALL,
                    &mut self.settings.bm_layout,
                    |s| s.as_str(),
                );
                changed |= choice_row(
                    ui,
                    tr(lang, "bm_l"),
                    LMode::ALL,
                    &mut self.settings.bm_l_mode,
                    |m| match m {
                        LMode::Assign => tr(lang, "bm_l_assign"),
                        LMode::Selector => tr(lang, "bm_l_selector"),
                    },
                );
                toggle(
                    ui,
                    &mut self.settings.bm_simple,
                    tr(lang, "bm_simple"),
                    &mut changed,
                );
            },
        );
        ui.add_space(10.0);
        section(
            ui,
            tr(lang, "controls_title"),
            Some(tr(lang, "ms_note")),
            |ui| {
                toggle(
                    ui,
                    &mut self.settings.manual_shield,
                    tr(lang, "ms_on"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.target_r3,
                    tr(lang, "target_r3_on"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.sprint_l3,
                    tr(lang, "sprint_l3_on"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.sprint_l3_toggle,
                    tr(lang, "sprint_l3_toggle_on"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.auto_target,
                    tr(lang, "auto_target_on"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.switch_target,
                    tr(lang, "switch_target_on"),
                    &mut changed,
                );
            },
        );
        ui.add_space(10.0);
        section(
            ui,
            tr(lang, "songbook_title"),
            Some(tr(lang, "songbook_desc")),
            |ui| {
                toggle(
                    ui,
                    &mut self.settings.songbook,
                    tr(lang, "songbook_on"),
                    &mut changed,
                );
            },
        );
        ui.add_space(10.0);
        section(
            ui,
            tr(lang, "quick_title"),
            Some(tr(lang, "quick_desc")),
            |ui| {
                toggle(
                    ui,
                    &mut self.settings.quick_boots,
                    tr(lang, "quick_boots_on"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.quick_mask,
                    tr(lang, "quick_mask_on"),
                    &mut changed,
                );
            },
        );
        ui.add_space(10.0);
        section(ui, tr(lang, "bm_on"), None, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(tr(lang, "recomp_log"));
                for key in roles::REMAP_PRESET_KEYS {
                    if ghost_button(ui, key).clicked()
                        && let Some(table) = roles::remap_preset(key)
                    {
                        self.settings.bm_roles = table;
                        changed = true;
                    }
                }
            });
            egui::Grid::new("remap-grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for button in ROLE_KEYS {
                        ui.label(button.as_str());
                        let current = self
                            .settings
                            .bm_roles
                            .get(&button)
                            .copied()
                            .unwrap_or(Role::None);
                        egui::ComboBox::from_id_salt(format!("role-{}", button.as_str()))
                            .selected_text(current.as_str())
                            .width(160.0)
                            .show_ui(ui, |ui| {
                                let entry =
                                    self.settings.bm_roles.entry(button).or_insert(Role::None);
                                for role in Role::ALL {
                                    if !roles::role_allowed(*role, button) {
                                        continue;
                                    }
                                    if ui.selectable_value(entry, *role, role.as_str()).changed() {
                                        changed = true;
                                    }
                                }
                            });
                        ui.end_row();
                    }
                });
        });
        changed
    }

    fn page_hud(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        section(
            ui,
            tr(lang, "hud_title"),
            Some(tr(lang, "hud_desc")),
            |ui| {
                ui.label(
                    egui::RichText::new(tr(lang, "hud_mode"))
                        .size(12.0)
                        .color(theme::DIM),
                );
                ui.horizontal_wrapped(|ui| {
                    for mode in HudMode::ALL {
                        let name = match mode {
                            HudMode::Wide => tr(lang, "hud_mode_wide"),
                            HudMode::Simple => tr(lang, "hud_mode_simple"),
                            HudMode::Plain => tr(lang, "hud_mode_plain"),
                        };
                        if ui
                            .selectable_value(&mut self.settings.hud_mode, *mode, name)
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
                ui.label(
                    egui::RichText::new(tr(lang, "hud_mode_desc"))
                        .size(12.0)
                        .color(theme::DIM),
                );
                toggle(
                    ui,
                    &mut self.settings.hud_rupees,
                    tr(lang, "hud_rupees"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.hud_clock,
                    tr(lang, "hud_clock"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.clock_24h,
                    tr(lang, "clock_24"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.hud_collectibles,
                    tr(lang, "hud_collectibles"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.stamina_bar,
                    tr(lang, "stamina_bar"),
                    &mut changed,
                );
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings.hud_ctrl_scale, 50..=200)
                            .step_by(5.0)
                            .text(tr(lang, "hud_ctrl_scale")),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut self.settings.hud_count_scale, 50..=200)
                            .step_by(5.0)
                            .text(tr(lang, "hud_count_scale")),
                    )
                    .changed();
            },
        );
        changed
    }

    fn page_misc(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        section(
            ui,
            tr(lang, "misc_title"),
            Some(tr(lang, "misc_desc")),
            |ui| {
                toggle(
                    ui,
                    &mut self.settings.great_fairy,
                    tr(lang, "great_fairy_on"),
                    &mut changed,
                );
                toggle(
                    ui,
                    &mut self.settings.farore_outdoor,
                    tr(lang, "farore_on"),
                    &mut changed,
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label(tr(lang, "layout_title"));
                    for layout in GameLayout::ALL {
                        if ui
                            .selectable_value(
                                &mut self.settings.game_layout,
                                *layout,
                                layout.as_str(),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
            },
        );
        ui.add_space(10.0);
        section(ui, tr(lang, "language"), None, |ui| {
            changed |= choice_row(ui, "", Lang::ALL, &mut self.settings.lang, |c| c.as_str());
            ui.label(
                egui::RichText::new(tr(lang, "rust_note"))
                    .size(12.0)
                    .color(theme::DIM),
            );
        });
        changed
    }

    fn page_about(&mut self, ui: &mut egui::Ui) {
        let lang = self.ui_lang;
        section(ui, tr(lang, "about_title"), None, |ui| {
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
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1060.0, 820.0])
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
