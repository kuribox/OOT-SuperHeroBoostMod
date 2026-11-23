//! The window: the same options as the Python app, in one scrolling page.
//!
//! Everything here goes through `shb-core`, so the window and `shb` (the CLI) cannot drift apart: the same
//! settings produce the same bytes. The window adds no logic of its own beyond remembering the last path.

use eframe::egui;
use shb_core::find::{self, MOD_NAME};
use shb_core::install::{self, InstallRequest};
use shb_core::presets::{self, PROFILE_KEYS};
use shb_core::roles::{self, ROLE_KEYS, Role};
use shb_core::settings::{BmLayout, ButtonStyle, GameLayout, HudMode, LMode, Lang, Settings};
use shb_core::{paths, payload_root};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiLang {
    En,
    Es,
}

impl UiLang {
    fn t(self, key: &'static str) -> &'static str {
        match (self, key) {
            (UiLang::En, "title") => "Super Hero Boost",
            (UiLang::En, "subtitle") => "Configurator (Rust)",
            (UiLang::En, "emu") => "Emulator folder",
            (UiLang::En, "detect") => "Detect",
            (UiLang::En, "mods_ok") => "Mod folder found",
            (UiLang::En, "mods_missing") => "No mod folder under that path",
            (UiLang::En, "installed") => "installed",
            (UiLang::En, "not_installed") => "not installed",
            (UiLang::En, "profile") => "Game profile",
            (UiLang::En, "apply") => "Apply",
            (UiLang::En, "stamina") => "Stamina",
            (UiLang::En, "enabled") => "Stamina system",
            (UiLang::En, "preset") => "Stamina costs",
            (UiLang::En, "actions") => "Actions that cost stamina",
            (UiLang::En, "jump") => "Jump",
            (UiLang::En, "attack") => "Attack",
            (UiLang::En, "sprint") => "Sprint",
            (UiLang::En, "block") => "Block",
            (UiLang::En, "shoot") => "Shoot",
            (UiLang::En, "roll") => "Roll",
            (UiLang::En, "combat_only") => "Only while in combat",
            (UiLang::En, "difficulty") => "Difficulty",
            (UiLang::En, "enemy_hp") => "Enemy health, % of vanilla",
            (UiLang::En, "damage") => "Damage taken, % of vanilla",
            (UiLang::En, "sprint_speed") => "Sprint speed, % of vanilla",
            (UiLang::En, "no_heart") => "No heart drops (green rupees)",
            (UiLang::En, "no_magic") => "No magic drops (green rupees)",
            (UiLang::En, "hud") => "HUD",
            (UiLang::En, "hud_mode") => "Layout",
            (UiLang::En, "rupees") => "Rupee counter",
            (UiLang::En, "clock") => "Clock",
            (UiLang::En, "clock24") => "24 hour clock",
            (UiLang::En, "collectibles") => "Skulltula / heart piece counter",
            (UiLang::En, "stamina_bar") => "Stamina as a bar (not the ring)",
            (UiLang::En, "hide_minimap") => "Hide the minimap",
            (UiLang::En, "ctrl_scale") => "Buttons size, %",
            (UiLang::En, "count_scale") => "Counters size, %",
            (UiLang::En, "controls") => "Controls",
            (UiLang::En, "button_mapping") => "buttonMapping (item buttons on the HUD)",
            (UiLang::En, "button_style") => "Button pictures",
            (UiLang::En, "bm_layout") => "Face button layout",
            (UiLang::En, "bm_simple") => "Simple layout",
            (UiLang::En, "l_mode") => "L does",
            (UiLang::En, "remap") => "Button remap",
            (UiLang::En, "table") => "Ready-made table",
            (UiLang::En, "manual_shield") => "Manual shield (ZL holds the guard)",
            (UiLang::En, "target_r3") => "Target with R3",
            (UiLang::En, "sprint_l3") => "L3 sprints",
            (UiLang::En, "sprint_l3_toggle") => "L3 toggles the sprint",
            (UiLang::En, "auto_target") => "Lock on to the next enemy",
            (UiLang::En, "switch_target") => "Switch target with the other shoulder",
            (UiLang::En, "songbook") => "Play songs from the songbook",
            (UiLang::En, "quick_boots") => "D-pad down: boots",
            (UiLang::En, "quick_mask") => "D-pad up: masks",
            (UiLang::En, "language") => "Language of the mod's HUD",
            (UiLang::En, "install") => "Install",
            (UiLang::En, "uninstall") => "Uninstall",
            (UiLang::En, "save") => "Save settings",
            (UiLang::En, "log") => "Messages",
            (UiLang::En, "payload") => "Payload",
            (UiLang::En, "payload_missing") => "payload not found (app/payload or --payload)",
            (UiLang::En, "profile_changed") => "Profile set to",
            (UiLang::En, "saved") => "Settings saved",
            (UiLang::En, "game_prompts") => "game prompts",
            (UiLang::Es, "title") => "Super Hero Boost",
            (UiLang::Es, "subtitle") => "Configurador (Rust)",
            (UiLang::Es, "emu") => "Carpeta del emulador",
            (UiLang::Es, "detect") => "Detectar",
            (UiLang::Es, "mods_ok") => "Carpeta de mods encontrada",
            (UiLang::Es, "mods_missing") => "No hay carpeta de mods en esa ruta",
            (UiLang::Es, "installed") => "instalado",
            (UiLang::Es, "not_installed") => "sin instalar",
            (UiLang::Es, "profile") => "Perfil de juego",
            (UiLang::Es, "apply") => "Aplicar",
            (UiLang::Es, "stamina") => "Resistencia",
            (UiLang::Es, "enabled") => "Sistema de resistencia",
            (UiLang::Es, "preset") => "Costes de resistencia",
            (UiLang::Es, "actions") => "Acciones que gastan resistencia",
            (UiLang::Es, "jump") => "Salto",
            (UiLang::Es, "attack") => "Ataque",
            (UiLang::Es, "sprint") => "Esprintar",
            (UiLang::Es, "block") => "Bloquear",
            (UiLang::Es, "shoot") => "Disparar",
            (UiLang::Es, "roll") => "Rodar",
            (UiLang::Es, "combat_only") => "Solo en combate",
            (UiLang::Es, "difficulty") => "Dificultad",
            (UiLang::Es, "enemy_hp") => "Vida enemiga, % del original",
            (UiLang::Es, "damage") => "Daño recibido, % del original",
            (UiLang::Es, "sprint_speed") => "Velocidad al esprintar, % del original",
            (UiLang::Es, "no_heart") => "Sin corazones (rupias verdes)",
            (UiLang::Es, "no_magic") => "Sin jarras de magia (rupias verdes)",
            (UiLang::Es, "hud") => "HUD",
            (UiLang::Es, "hud_mode") => "Disposición",
            (UiLang::Es, "rupees") => "Contador de rupias",
            (UiLang::Es, "clock") => "Reloj",
            (UiLang::Es, "clock24") => "Reloj de 24 h",
            (UiLang::Es, "collectibles") => "Contador de skulltulas / piezas de corazón",
            (UiLang::Es, "stamina_bar") => "Resistencia como barra (no el anillo)",
            (UiLang::Es, "hide_minimap") => "Ocultar el minimapa",
            (UiLang::Es, "ctrl_scale") => "Tamaño de botones, %",
            (UiLang::Es, "count_scale") => "Tamaño de contadores, %",
            (UiLang::Es, "controls") => "Controles",
            (UiLang::Es, "button_mapping") => "buttonMapping (botones de objeto en el HUD)",
            (UiLang::Es, "button_style") => "Dibujos de los botones",
            (UiLang::Es, "bm_layout") => "Disposición de los botones",
            (UiLang::Es, "bm_simple") => "Disposición simple",
            (UiLang::Es, "l_mode") => "L hace",
            (UiLang::Es, "remap") => "Remapeo de botones",
            (UiLang::Es, "table") => "Tabla preparada",
            (UiLang::Es, "manual_shield") => "Escudo manual (ZL mantiene la guardia)",
            (UiLang::Es, "target_r3") => "Fijar objetivo con R3",
            (UiLang::Es, "sprint_l3") => "L3 esprinta",
            (UiLang::Es, "sprint_l3_toggle") => "L3 activa el esprint",
            (UiLang::Es, "auto_target") => "Fijar al siguiente enemigo",
            (UiLang::Es, "switch_target") => "Cambiar de objetivo con el otro hombro",
            (UiLang::Es, "songbook") => "Tocar canciones desde el cancionero",
            (UiLang::Es, "quick_boots") => "Cruceta abajo: botas",
            (UiLang::Es, "quick_mask") => "Cruceta arriba: máscaras",
            (UiLang::Es, "language") => "Idioma del HUD del mod",
            (UiLang::Es, "install") => "Instalar",
            (UiLang::Es, "uninstall") => "Desinstalar",
            (UiLang::Es, "save") => "Guardar ajustes",
            (UiLang::Es, "log") => "Mensajes",
            (UiLang::Es, "payload") => "Payload",
            (UiLang::Es, "payload_missing") => "no encuentro el payload (app/payload o --payload)",
            (UiLang::Es, "profile_changed") => "Perfil cambiado a",
            (UiLang::Es, "saved") => "Ajustes guardados",
            (UiLang::Es, "game_prompts") => "prompts del juego",
            (_, missing) => missing,
        }
    }
}

/// The labels of the stamina modes (`preset` in the config block).
const PRESET_NAMES: [&str; 4] = ["hardcore", "combat only", "sprint only", "advanced"];

struct App {
    emu: String,
    settings: Settings,
    profile: String,
    ui_lang: UiLang,
    payload: Option<PathBuf>,
    log: Vec<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let (settings, _) = paths::load_settings();
        let emu = std::env::var("SHB_EMU").unwrap_or_default();
        let mut app = App {
            emu,
            settings,
            profile: presets::DEFAULT_PROFILE.to_string(),
            ui_lang: UiLang::En,
            payload: payload_root(None),
            log: Vec::new(),
        };
        match app.payload.clone() {
            Some(path) => app.say(format!("payload: {}", path.display())),
            None => app.say(app.ui_lang.t("payload_missing")),
        }
        app.refresh_emulator();
        app
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
                self.ui_lang.t("installed")
            )
        });
        match (&info.mods, installed) {
            (Some(mods), Some(state)) => {
                let line = format!(
                    "{}: {} ({state})",
                    self.ui_lang.t("mods_ok"),
                    mods.display()
                );
                self.say(line);
                if let Some(exe) = &info.exe {
                    let line = format!("emulator: {}", exe.display());
                    self.say(line);
                }
            }
            (Some(mods), None) => {
                let line = format!(
                    "{}: {} ({})",
                    self.ui_lang.t("mods_ok"),
                    mods.display(),
                    self.ui_lang.t("not_installed")
                );
                self.say(line);
            }
            (None, _) => self.say(self.ui_lang.t("mods_missing")),
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
        let line = format!("{} {}", self.ui_lang.t("profile_changed"), self.profile);
        self.say(line);
    }

    fn install(&mut self) {
        let Some(payload) = self.payload.clone() else {
            self.say(self.ui_lang.t("payload_missing"));
            return;
        };
        if self.emu.trim().is_empty() {
            self.say(self.ui_lang.t("mods_missing"));
            return;
        }
        let req = InstallRequest::new(
            PathBuf::from(self.emu.trim()),
            self.settings.clone(),
            payload,
        );
        match install::install(&req) {
            Ok(result) => {
                let line = format!(
                    "{} {}",
                    self.ui_lang.t("installed"),
                    result.target.display()
                );
                self.say(line);
                for note in result.notes {
                    self.say(format!("  {note}"));
                }
            }
            Err(err) => self.say(format!("error: {err}")),
        }
    }

    fn uninstall(&mut self) {
        if self.emu.trim().is_empty() {
            return;
        }
        let emu = PathBuf::from(self.emu.trim());
        match install::uninstall(&emu) {
            Ok(true) => self.say(format!("{MOD_NAME}: removed")),
            Ok(false) => self.say("nothing to remove"),
            Err(err) => self.say(format!("error: {err}")),
        }
    }

    fn save(&mut self) {
        match paths::save_settings(&self.settings) {
            Ok(path) => {
                let line = format!("{}: {}", self.ui_lang.t("saved"), path.display());
                self.say(line);
            }
            Err(err) => self.say(format!("error: {err}")),
        }
    }
}

/// A checkbox that tells the caller whether the user changed it (any change means "custom").
fn toggle(ui: &mut egui::Ui, value: &mut bool, text: &str, changed: &mut bool) {
    if ui.checkbox(value, text).changed() {
        *changed = true;
    }
}

impl eframe::App for App {
    // egui 0.37 hands the app a root `Ui` (it used to hand a `Context` and the panels were top-level).
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let lang = self.ui_lang;
        egui::Panel::top("header").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(lang.t("title"));
                ui.label(lang.t("subtitle"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.selectable_value(&mut self.ui_lang, UiLang::Es, "ES");
                    ui.selectable_value(&mut self.ui_lang, UiLang::En, "EN");
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(lang.t("emu"));
                ui.add(egui::TextEdit::singleline(&mut self.emu).desired_width(420.0));
                if ui.button(lang.t("detect")).clicked() {
                    self.refresh_emulator();
                }
            });
            if let Some(payload) = self.payload.clone() {
                ui.label(format!("{}: {}", lang.t("payload"), payload.display()));
            }
        });

        egui::Panel::bottom("actions").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button(lang.t("install")).clicked() {
                    self.install();
                }
                if ui.button(lang.t("uninstall")).clicked() {
                    self.uninstall();
                }
                if ui.button(lang.t("save")).clicked() {
                    self.save();
                }
                ui.separator();
                egui::ComboBox::from_label(lang.t("profile"))
                    .selected_text(self.profile.clone())
                    .show_ui(ui, |ui| {
                        for key in PROFILE_KEYS {
                            ui.selectable_value(&mut self.profile, key.to_string(), key);
                        }
                    });
                if ui.button(lang.t("apply")).clicked() {
                    self.apply_profile();
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let mut changed = false;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::CollapsingHeader::new(lang.t("stamina"))
                        .default_open(true)
                        .show(ui, |ui| {
                            toggle(
                                ui,
                                &mut self.settings.stamina,
                                lang.t("enabled"),
                                &mut changed,
                            );
                            ui.horizontal(|ui| {
                                ui.label(lang.t("preset"));
                                // The module's stamina modes, in the order `presets.STAMINA_MODES` uses.
                                for (value, name) in PRESET_NAMES.iter().enumerate() {
                                    if ui
                                        .selectable_value(
                                            &mut self.settings.preset,
                                            value as u8,
                                            *name,
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label(lang.t("actions"));
                                toggle(ui, &mut self.settings.jump, lang.t("jump"), &mut changed);
                                toggle(
                                    ui,
                                    &mut self.settings.attack,
                                    lang.t("attack"),
                                    &mut changed,
                                );
                                toggle(
                                    ui,
                                    &mut self.settings.sprint,
                                    lang.t("sprint"),
                                    &mut changed,
                                );
                                toggle(ui, &mut self.settings.block, lang.t("block"), &mut changed);
                                toggle(ui, &mut self.settings.shoot, lang.t("shoot"), &mut changed);
                                toggle(ui, &mut self.settings.roll, lang.t("roll"), &mut changed);
                            });
                            toggle(
                                ui,
                                &mut self.settings.combat_only,
                                lang.t("combat_only"),
                                &mut changed,
                            );
                        });

                    egui::CollapsingHeader::new(lang.t("difficulty"))
                        .default_open(true)
                        .show(ui, |ui| {
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.settings.enemy_hp, 50..=400)
                                        .text(lang.t("enemy_hp")),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.settings.damage, 50..=400)
                                        .text(lang.t("damage")),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.settings.sprint_speed, 100..=200)
                                        .text(lang.t("sprint_speed")),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            toggle(
                                ui,
                                &mut self.settings.no_heart_drop,
                                lang.t("no_heart"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.no_magic_drop,
                                lang.t("no_magic"),
                                &mut changed,
                            );
                        });

                    egui::CollapsingHeader::new(lang.t("hud"))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(lang.t("hud_mode"));
                                for mode in HudMode::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut self.settings.hud_mode,
                                            *mode,
                                            mode.as_str(),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });
                            toggle(
                                ui,
                                &mut self.settings.hud_rupees,
                                lang.t("rupees"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.hud_clock,
                                lang.t("clock"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.clock_24h,
                                lang.t("clock24"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.hud_collectibles,
                                lang.t("collectibles"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.stamina_bar,
                                lang.t("stamina_bar"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.hide_minimap,
                                lang.t("hide_minimap"),
                                &mut changed,
                            );
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.settings.hud_ctrl_scale, 50..=200)
                                        .step_by(5.0)
                                        .text(lang.t("ctrl_scale")),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.settings.hud_count_scale, 50..=200)
                                        .step_by(5.0)
                                        .text(lang.t("count_scale")),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        });

                    egui::CollapsingHeader::new(lang.t("controls"))
                        .default_open(true)
                        .show(ui, |ui| {
                            toggle(
                                ui,
                                &mut self.settings.button_mapping,
                                lang.t("button_mapping"),
                                &mut changed,
                            );
                            ui.horizontal(|ui| {
                                ui.label(lang.t("button_style"));
                                for style in ButtonStyle::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut self.settings.button_style,
                                            *style,
                                            style.as_str(),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(lang.t("bm_layout"));
                                for layout in BmLayout::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut self.settings.bm_layout,
                                            *layout,
                                            layout.as_str(),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(lang.t("l_mode"));
                                for mode in LMode::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut self.settings.bm_l_mode,
                                            *mode,
                                            mode.as_str(),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });
                            toggle(
                                ui,
                                &mut self.settings.bm_simple,
                                lang.t("bm_simple"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.manual_shield,
                                lang.t("manual_shield"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.target_r3,
                                lang.t("target_r3"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.sprint_l3,
                                lang.t("sprint_l3"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.sprint_l3_toggle,
                                lang.t("sprint_l3_toggle"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.auto_target,
                                lang.t("auto_target"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.switch_target,
                                lang.t("switch_target"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.songbook,
                                lang.t("songbook"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.quick_boots,
                                lang.t("quick_boots"),
                                &mut changed,
                            );
                            toggle(
                                ui,
                                &mut self.settings.quick_mask,
                                lang.t("quick_mask"),
                                &mut changed,
                            );
                            if self.settings.game_layout != GameLayout::None {
                                ui.label(format!(
                                    "{}: {}",
                                    lang.t("game_prompts"),
                                    self.settings.game_layout
                                ));
                            }
                        });

                    egui::CollapsingHeader::new(lang.t("remap"))
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(lang.t("table"));
                                for key in roles::REMAP_PRESET_KEYS {
                                    if ui.button(key).clicked()
                                        && let Some(table) = roles::remap_preset(key)
                                    {
                                        self.settings.bm_roles = table;
                                        changed = true;
                                    }
                                }
                            });
                            egui::Grid::new("remap-grid").num_columns(2).show(ui, |ui| {
                                for button in ROLE_KEYS {
                                    ui.label(button.as_str());
                                    let current = self
                                        .settings
                                        .bm_roles
                                        .get(&button)
                                        .copied()
                                        .unwrap_or(Role::None);
                                    egui::ComboBox::from_id_salt(format!(
                                        "role-{}",
                                        button.as_str()
                                    ))
                                    .selected_text(current.as_str())
                                    .show_ui(ui, |ui| {
                                        let entry = self
                                            .settings
                                            .bm_roles
                                            .entry(button)
                                            .or_insert(Role::None);
                                        for role in Role::ALL {
                                            if !roles::role_allowed(*role, button) {
                                                continue;
                                            }
                                            if ui
                                                .selectable_value(entry, *role, role.as_str())
                                                .changed()
                                            {
                                                changed = true;
                                            }
                                        }
                                    });
                                    ui.end_row();
                                }
                            });
                        });

                    ui.horizontal(|ui| {
                        ui.label(lang.t("language"));
                        for lang_code in Lang::ALL {
                            if ui
                                .selectable_value(
                                    &mut self.settings.lang,
                                    *lang_code,
                                    lang_code.as_str(),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        }
                    });

                    ui.separator();
                    ui.label(lang.t("log"));
                    egui::ScrollArea::vertical()
                        .max_height(140.0)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.log {
                                ui.monospace(line);
                            }
                        });
                });

            if changed && self.profile != "custom" {
                // Any option the user touches turns the profile into "custom", like the Python window.
                self.profile = "custom".to_string();
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
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 760.0])
            .with_min_inner_size([720.0, 520.0]),
        ..Default::default()
    };
    eprintln!("the game's title id: {}", find::TITLE_ID);
    eframe::run_native(
        "Super Hero Boost",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
