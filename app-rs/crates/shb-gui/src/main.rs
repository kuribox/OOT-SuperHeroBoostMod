//! The window: the same pages as the Python app (sidebar, then one page at a time).
//!
//! Everything here goes through `shb-core`, so the window and `shb` (the CLI) cannot drift apart: the same
//! settings produce the same bytes. The window adds no logic of its own beyond remembering the last path.

use eframe::egui;
use shb_core::find::{self, MOD_NAME};
use shb_core::install::{self, InstallRequest};
use shb_core::presets::{self, PROFILE_KEYS};
use shb_core::roles::{self, ROLE_KEYS, Role};
use shb_core::settings::{BmLayout, ButtonStyle, GameLayout, HudMode, LMode, Lang, Settings};
use shb_core::{enemy, paths, payload_root};
use std::path::PathBuf;

/// A page of the sidebar. The order is the Python app's (`nav` in `main.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Install,
    Home,
    Stamina,
    Difficulty,
    Controls,
    Hud,
    Misc,
    About,
}

impl Page {
    const ALL: [Page; 8] = [
        Page::Install,
        Page::Home,
        Page::Stamina,
        Page::Difficulty,
        Page::Controls,
        Page::Hud,
        Page::Misc,
        Page::About,
    ];

    fn key(self) -> &'static str {
        match self {
            Page::Install => "nav_install",
            Page::Home => "nav_home",
            Page::Stamina => "nav_stamina",
            Page::Difficulty => "nav_difficulty",
            Page::Controls => "nav_controls",
            Page::Hud => "nav_hud",
            Page::Misc => "nav_misc",
            Page::About => "nav_about",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiLang {
    En,
    Es,
}

impl UiLang {
    fn t(self, key: &'static str) -> &'static str {
        let es = self == UiLang::Es;
        match key {
            "title" => "Super Hero Boost",
            "subtitle" if es => "Configurador",
            "subtitle" => "Configurator",
            "nav_install" if es => "Instalar",
            "nav_install" => "Install",
            "nav_home" if es => "Inicio",
            "nav_home" => "Home",
            "nav_stamina" if es => "Resistencia",
            "nav_stamina" => "Stamina",
            "nav_difficulty" if es => "Dificultad",
            "nav_difficulty" => "Difficulty",
            "nav_controls" if es => "Controles",
            "nav_controls" => "Controls",
            "nav_hud" => "HUD",
            "nav_misc" if es => "Varios",
            "nav_misc" => "Misc",
            "nav_about" if es => "Acerca de",
            "nav_about" => "About",
            "emu" if es => "Carpeta del emulador",
            "emu" => "Emulator folder",
            "detect" if es => "Detectar",
            "detect" => "Detect",
            "mods_ok" if es => "Carpeta de mods encontrada",
            "mods_ok" => "Mod folder found",
            "mods_missing" if es => "No hay carpeta de mods en esa ruta",
            "mods_missing" => "No mod folder under that path",
            "installed" if es => "instalado",
            "installed" => "installed",
            "not_installed" if es => "sin instalar",
            "not_installed" => "not installed",
            "install" if es => "Instalar",
            "install" => "Install",
            "reinstall" if es => "Reinstalar",
            "reinstall" => "Reinstall",
            "uninstall" if es => "Desinstalar",
            "uninstall" => "Uninstall",
            "save" if es => "Guardar ajustes",
            "save" => "Save settings",
            "profile" if es => "Perfil de juego",
            "profile" => "Game profile",
            "apply" if es => "Aplicar perfil",
            "apply" => "Apply profile",
            "profile_help" if es => {
                "Un perfil fija todas las opciones de juego. Si cambias cualquiera, el perfil pasa a personalizado."
            }
            "profile_help" => {
                "A profile sets every gameplay option. Change any of them and the profile becomes custom."
            }
            "custom" if es => "personalizado",
            "custom" => "custom",
            "stamina" if es => "Resistencia",
            "stamina" => "Stamina",
            "enabled" if es => "Sistema de resistencia",
            "enabled" => "Stamina system",
            "preset" if es => "Costes de resistencia",
            "preset" => "Stamina costs",
            "preset_hardcore" if es => "duro",
            "preset_hardcore" => "hardcore",
            "preset_combat" if es => "solo combate",
            "preset_combat" => "combat only",
            "preset_sprint" if es => "solo esprintar",
            "preset_sprint" => "sprint only",
            "preset_advanced" if es => "avanzado",
            "preset_advanced" => "advanced",
            "actions" if es => "Acciones que gastan resistencia",
            "actions" => "Actions that cost stamina",
            "jump" if es => "Salto",
            "jump" => "Jump",
            "attack" if es => "Ataque",
            "attack" => "Attack",
            "sprint" if es => "Esprintar",
            "sprint" => "Sprint",
            "block" if es => "Bloquear",
            "block" => "Block",
            "shoot" if es => "Disparar",
            "shoot" => "Shoot",
            "roll" if es => "Rodar",
            "roll" => "Roll",
            "combat_only" if es => "Solo en combate",
            "combat_only" => "Only while in combat",
            "difficulty" if es => "Dificultad",
            "difficulty" => "Difficulty",
            "enemy_hp" if es => "Vida enemiga, % del original",
            "enemy_hp" => "Enemy health, % of vanilla",
            "damage" if es => "Daño recibido, % del original",
            "damage" => "Damage taken, % of vanilla",
            "sprint_speed" if es => "Velocidad al esprintar, % del original",
            "sprint_speed" => "Sprint speed, % of vanilla",
            "no_heart" if es => "Sin corazones (rupias verdes)",
            "no_heart" => "No heart drops (green rupees)",
            "no_magic" if es => "Sin jarras de magia (rupias verdes)",
            "no_magic" => "No magic drops (green rupees)",
            "enemies" if es => "Enemigos",
            "enemies" => "Enemies",
            "enemy_move" if es => "Velocidad de movimiento",
            "enemy_move" => "Movement speed",
            "enemy_anim" if es => "Velocidad de animación",
            "enemy_anim" => "Animation speed",
            "enemy_attack" if es => "Frecuencia de ataque",
            "enemy_attack" => "Attack cadence",
            "enemy_proj" if es => "Velocidad de proyectiles",
            "enemy_proj" => "Projectile speed",
            "enemy_alert" if es => "Radio de alerta a otros enemigos",
            "enemy_alert" => "Alert radius",
            "enemy_aggro" if es => "Agresividad",
            "enemy_aggro" => "Aggression",
            "boss_move" if es => "Jefes: movimiento",
            "boss_move" => "Bosses: movement",
            "boss_anim" if es => "Jefes: animación",
            "boss_anim" => "Bosses: animation",
            "level_off" if es => "no",
            "level_off" => "off",
            "aggro_normal" if es => "normal",
            "aggro_normal" => "normal",
            "aggro_aggressive" if es => "agresivos",
            "aggro_aggressive" => "aggressive",
            "aggro_max" if es => "máxima",
            "aggro_max" => "maximum",
            "hud" if es => "HUD",
            "hud" => "HUD",
            "hud_mode" if es => "Disposición",
            "hud_mode" => "Layout",
            "rupees" if es => "Contador de rupias",
            "rupees" => "Rupee counter",
            "clock" if es => "Reloj",
            "clock" => "Clock",
            "clock24" if es => "Reloj de 24 h",
            "clock24" => "24 hour clock",
            "collectibles" if es => "Contador de skulltulas / piezas de corazón",
            "collectibles" => "Skulltula / heart piece counter",
            "stamina_bar" if es => "Resistencia como barra (no el anillo)",
            "stamina_bar" => "Stamina as a bar (not the ring)",
            "hide_minimap" if es => "Ocultar el minimapa",
            "hide_minimap" => "Hide the minimap",
            "ctrl_scale" if es => "Tamaño de los botones, %",
            "ctrl_scale" => "Buttons size, %",
            "count_scale" if es => "Tamaño de los contadores, %",
            "count_scale" => "Counters size, %",
            "hud_wide" if es => "ancha",
            "hud_wide" => "wide",
            "hud_simple" if es => "simple",
            "hud_simple" => "simple",
            "hud_plain" if es => "sin placa",
            "hud_plain" => "plain",
            "controls" if es => "Controles",
            "controls" => "Controls",
            "button_mapping" if es => "buttonMapping (botones de objeto en el HUD)",
            "button_mapping" => "buttonMapping (item buttons on the HUD)",
            "button_style" if es => "Dibujos de los botones",
            "button_style" => "Button pictures",
            "bm_layout" if es => "Disposición de los botones",
            "bm_layout" => "Face button layout",
            "bm_simple" if es => "Disposición simple",
            "bm_simple" => "Simple layout",
            "l_mode" if es => "L hace",
            "l_mode" => "L does",
            "remap" if es => "Remapeo de botones",
            "remap" => "Button remap",
            "remap_help" if es => {
                "Un rol por botón como mucho, y solo donde el módulo lo admite. Las pulsaciones largas van aparte."
            }
            "remap_help" => {
                "One role per button at most, and only where the module allows it. Long presses are separate."
            }
            "table" if es => "Tabla preparada",
            "table" => "Ready-made table",
            "manual_shield" if es => "Escudo manual (ZL mantiene la guardia)",
            "manual_shield" => "Manual shield (ZL holds the guard)",
            "target_r3" if es => "Fijar objetivo con R3",
            "target_r3" => "Target with R3",
            "sprint_l3" if es => "L3 esprinta",
            "sprint_l3" => "L3 sprints",
            "sprint_l3_toggle" if es => "L3 activa el esprint",
            "sprint_l3_toggle" => "L3 toggles the sprint",
            "auto_target" if es => "Fijar al siguiente enemigo",
            "auto_target" => "Lock on to the next enemy",
            "switch_target" if es => "Cambiar de objetivo con el otro hombro",
            "switch_target" => "Switch target with the other shoulder",
            "songbook" if es => "Tocar canciones desde el cancionero",
            "songbook" => "Play songs from the songbook",
            "quick_boots" if es => "Cruceta abajo: botas",
            "quick_boots" => "D-pad down: boots",
            "quick_mask" if es => "Cruceta arriba: máscaras",
            "quick_mask" => "D-pad up: masks",
            "misc" if es => "Opciones del mundo",
            "misc" => "World options",
            "great_fairy" if es => "Reordenar los regalos de las Grandes Hadas",
            "great_fairy" => "Reorder the Great Fairy rewards",
            "farore_outdoor" if es => "Farore's Wind fuera de las mazmorras",
            "farore_outdoor" => "Farore's Wind outside dungeons",
            "enemy_double" if es => "Duplicar enemigos (beta)",
            "enemy_double" => "Duplicate enemies (beta)",
            "game_layout" if es => "Prompts del juego (descarga aparte)",
            "game_layout" => "Game prompts (separate download)",
            "language" if es => "Idioma del HUD del mod",
            "language" => "Language of the mod's HUD",
            "about_body" if es => {
                "Super Hero Boost: resistencia, dificultad, HUD y controles al gusto.\n\n\
                 Esta ventana es la versión en Rust del configurador; instala el mismo módulo y escribe los mismos \
                 bytes que la app de Python.\n\n\
                 El mod no incluye código de terceros. Ocarina of Time es una marca de Nintendo; este proyecto no \
                 está afiliado a ella."
            }
            "about_body" => {
                "Super Hero Boost: stamina, difficulty, HUD and controls the way you want them.\n\n\
                 This window is the Rust version of the configurator; it installs the same module and writes the \
                 same bytes as the Python app.\n\n\
                 The mod bundles no third-party code. Ocarina of Time is a Nintendo trademark; this project is not \
                 affiliated with it."
            }
            "log" if es => "Mensajes",
            "log" => "Messages",
            "payload" => "Payload",
            "payload_missing" if es => "no encuentro el payload (app/payload o --payload)",
            "payload_missing" => "payload not found (app/payload or --payload)",
            "profile_changed" if es => "Perfil cambiado a",
            "profile_changed" => "Profile set to",
            "saved" if es => "Ajustes guardados",
            "saved" => "Settings saved",
            "removed" if es => "eliminado",
            "removed" => "removed",
            "nothing_to_remove" if es => "no había nada que quitar",
            "nothing_to_remove" => "nothing to remove",
            "not_ported" if es => "Aún no portado a la app de Rust:",
            "not_ported" => "Not ported to the Rust app yet:",
            "game_prompts" if es => "prompts del juego",
            "game_prompts" => "game prompts",
            "needs_dump" if es => "necesita el volcado del juego",
            "needs_dump" => "needs the game dump",
            _ => key,
        }
    }

    fn hud_mode_name(self, mode: HudMode) -> &'static str {
        self.t(match mode {
            HudMode::Wide => "hud_wide",
            HudMode::Simple => "hud_simple",
            HudMode::Plain => "hud_plain",
        })
    }

    fn aggro_name(self, level: u8) -> &'static str {
        self.t(match level {
            1 => "aggro_aggressive",
            2 => "aggro_max",
            _ => "aggro_normal",
        })
    }
}

/// The labels of the stamina modes (`preset` in the config block).
const PRESET_KEYS: [&str; 4] = [
    "preset_hardcore",
    "preset_combat",
    "preset_sprint",
    "preset_advanced",
];

struct App {
    page: Page,
    emu: String,
    settings: Settings,
    profile: String,
    ui_lang: UiLang,
    payload: Option<PathBuf>,
    log: Vec<String>,
    /// The list of notes the last install produced, shown on the install page.
    notes: Vec<String>,
    has_failed: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let (settings, _) = paths::load_settings();
        let emu = std::env::var("SHB_EMU").unwrap_or_default();
        let mut app = App {
            page: Page::Install,
            emu,
            settings,
            profile: presets::DEFAULT_PROFILE.to_string(),
            ui_lang: UiLang::En,
            payload: payload_root(None),
            log: Vec::new(),
            notes: Vec::new(),
            has_failed: false,
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
            self.notes = vec![self.ui_lang.t("payload_missing").to_string()];
            self.has_failed = true;
            return;
        };
        if self.emu.trim().is_empty() {
            self.notes = vec![self.ui_lang.t("mods_missing").to_string()];
            self.has_failed = true;
            return;
        }
        let req = InstallRequest::new(
            PathBuf::from(self.emu.trim()),
            self.settings.clone(),
            payload,
        );
        match install::install(&req) {
            Ok(result) => {
                self.has_failed = false;
                self.notes = result.notes;
                let line = format!(
                    "{} {}",
                    self.ui_lang.t("installed"),
                    result.target.display()
                );
                self.say(line);
            }
            Err(err) => {
                self.has_failed = true;
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
                self.say(format!("{MOD_NAME} {}", self.ui_lang.t("removed")));
            }
            Ok(false) => self.say(self.ui_lang.t("nothing_to_remove")),
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

/// A row of choices, the way the Python app draws its segmented controls.
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

/// A whole-number slider with the level names of the enemy options under it.
fn enemy_level(ui: &mut egui::Ui, label: &str, value: &mut u8) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(format!("enemy-{label}"))
            .selected_text(match enemy::factor(*value) {
                Some(factor) => format!("x{factor}"),
                None => "off".to_string(),
            })
            .show_ui(ui, |ui| {
                if ui.selectable_value(value, 0, "off").changed() {
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

        egui::Panel::top("header").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(lang.t("title"));
                ui.label(lang.t("subtitle"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.selectable_value(&mut self.ui_lang, UiLang::Es, "ES");
                    ui.selectable_value(&mut self.ui_lang, UiLang::En, "EN");
                });
            });
        });

        egui::Panel::left("nav").resizable(false).show(ui, |ui| {
            ui.add_space(6.0);
            for page in Page::ALL {
                let selected = self.page == page;
                let button = egui::Button::new(lang.t(page.key())).selected(selected);
                if ui.add_sized([ui.available_width(), 32.0], button).clicked() {
                    self.page = page;
                }
            }
        });

        egui::Panel::bottom("footer").show(ui, |ui| {
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
                let name = if self.profile == "custom" {
                    lang.t("custom")
                } else {
                    self.profile.as_str()
                };
                ui.label(format!("{}: {name}", lang.t("profile")));
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.page {
                    Page::Install => changed |= self.page_install(ui),
                    Page::Home => changed |= self.page_home(ui),
                    Page::Stamina => changed |= self.page_stamina(ui),
                    Page::Difficulty => changed |= self.page_difficulty(ui),
                    Page::Controls => changed |= self.page_controls(ui),
                    Page::Hud => changed |= self.page_hud(ui),
                    Page::Misc => changed |= self.page_misc(ui),
                    Page::About => self.page_about(ui),
                });
        });

        if changed && self.profile != "custom" {
            // Any option the user touches turns the profile into "custom", like the Python window.
            self.profile = "custom".to_string();
        }
    }
}

impl App {
    fn page_install(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(lang.t("emu"));
            ui.add(egui::TextEdit::singleline(&mut self.emu).desired_width(f32::INFINITY));
        });
        ui.horizontal(|ui| {
            if ui.button(lang.t("detect")).clicked() {
                self.refresh_emulator();
            }
            if let Some(payload) = &self.payload {
                ui.label(format!("{}: {}", lang.t("payload"), payload.display()));
            }
        });
        ui.add_space(6.0);
        if !self.notes.is_empty() {
            ui.separator();
            for note in &self.notes {
                if self.has_failed {
                    ui.colored_label(egui::Color32::from_rgb(220, 120, 110), note);
                } else {
                    ui.label(note);
                }
            }
        }
        ui.separator();
        ui.label(lang.t("log"));
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.log {
                    ui.monospace(line);
                }
            });
        false
    }

    fn page_home(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        ui.add_space(4.0);
        ui.label(lang.t("profile_help"));
        ui.add_space(6.0);
        for key in PROFILE_KEYS {
            let selected = self.profile == key;
            if ui.selectable_label(selected, key).clicked() {
                self.profile = key.to_string();
            }
        }
        ui.add_space(6.0);
        if ui.button(lang.t("apply")).clicked() {
            self.apply_profile();
        }
        ui.separator();
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(lang.t("language"));
            for code in Lang::ALL {
                if ui
                    .selectable_value(&mut self.settings.lang, *code, code.as_str())
                    .changed()
                {
                    changed = true;
                }
            }
        });
        changed
    }

    fn page_stamina(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        toggle(
            ui,
            &mut self.settings.stamina,
            lang.t("enabled"),
            &mut changed,
        );
        ui.horizontal_wrapped(|ui| {
            ui.label(lang.t("preset"));
            for (value, key) in PRESET_KEYS.iter().enumerate() {
                if ui
                    .selectable_value(&mut self.settings.preset, value as u8, lang.t(key))
                    .changed()
                {
                    changed = true;
                }
            }
        });
        ui.add_space(4.0);
        ui.label(lang.t("actions"));
        for (value, key) in [
            (&mut self.settings.jump, "jump"),
            (&mut self.settings.attack, "attack"),
            (&mut self.settings.sprint, "sprint"),
            (&mut self.settings.block, "block"),
            (&mut self.settings.shoot, "shoot"),
            (&mut self.settings.roll, "roll"),
        ] {
            toggle(ui, value, lang.t(key), &mut changed);
        }
        toggle(
            ui,
            &mut self.settings.combat_only,
            lang.t("combat_only"),
            &mut changed,
        );
        changed
    }

    fn page_difficulty(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        changed |= ui
            .add(egui::Slider::new(&mut self.settings.enemy_hp, 50..=400).text(lang.t("enemy_hp")))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.settings.damage, 50..=400).text(lang.t("damage")))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.settings.sprint_speed, 100..=200)
                    .text(lang.t("sprint_speed")),
            )
            .changed();
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
        ui.separator();
        ui.label(lang.t("enemies"));
        changed |= enemy_level(ui, lang.t("enemy_move"), &mut self.settings.enemy_move);
        changed |= enemy_level(ui, lang.t("enemy_anim"), &mut self.settings.enemy_anim);
        changed |= enemy_level(ui, lang.t("enemy_attack"), &mut self.settings.enemy_attack);
        changed |= enemy_level(ui, lang.t("enemy_proj"), &mut self.settings.enemy_proj);
        changed |= enemy_level(ui, lang.t("enemy_alert"), &mut self.settings.enemy_alert);
        changed |= enemy_level(ui, lang.t("boss_move"), &mut self.settings.boss_move);
        changed |= enemy_level(ui, lang.t("boss_anim"), &mut self.settings.boss_anim);
        ui.horizontal_wrapped(|ui| {
            ui.label(lang.t("enemy_aggro"));
            for level in enemy::AGGRO_LEVELS {
                if ui
                    .selectable_value(
                        &mut self.settings.enemy_aggro,
                        level,
                        lang.aggro_name(level),
                    )
                    .changed()
                {
                    changed = true;
                }
            }
        });
        toggle(
            ui,
            &mut self.settings.enemy_double,
            lang.t("enemy_double"),
            &mut changed,
        );
        changed
    }

    fn page_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        toggle(
            ui,
            &mut self.settings.button_mapping,
            lang.t("button_mapping"),
            &mut changed,
        );
        changed |= choice_row(
            ui,
            lang.t("button_style"),
            ButtonStyle::ALL,
            &mut self.settings.button_style,
            |s| s.as_str(),
        );
        changed |= choice_row(
            ui,
            lang.t("bm_layout"),
            BmLayout::ALL,
            &mut self.settings.bm_layout,
            |s| s.as_str(),
        );
        changed |= choice_row(
            ui,
            lang.t("l_mode"),
            LMode::ALL,
            &mut self.settings.bm_l_mode,
            |s| s.as_str(),
        );
        toggle(
            ui,
            &mut self.settings.bm_simple,
            lang.t("bm_simple"),
            &mut changed,
        );
        ui.separator();
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
        ui.separator();
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
        ui.separator();
        ui.label(lang.t("remap"));
        ui.label(lang.t("remap_help"));
        ui.horizontal_wrapped(|ui| {
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
                egui::ComboBox::from_id_salt(format!("role-{}", button.as_str()))
                    .selected_text(current.as_str())
                    .show_ui(ui, |ui| {
                        let entry = self.settings.bm_roles.entry(button).or_insert(Role::None);
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
        changed
    }

    fn page_hud(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(lang.t("hud_mode"));
            for mode in HudMode::ALL {
                if ui
                    .selectable_value(
                        &mut self.settings.hud_mode,
                        *mode,
                        lang.hud_mode_name(*mode),
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
        changed |= ui
            .add(
                egui::Slider::new(&mut self.settings.hud_ctrl_scale, 50..=200)
                    .step_by(5.0)
                    .text(lang.t("ctrl_scale")),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.settings.hud_count_scale, 50..=200)
                    .step_by(5.0)
                    .text(lang.t("count_scale")),
            )
            .changed();
        changed
    }

    fn page_misc(&mut self, ui: &mut egui::Ui) -> bool {
        let lang = self.ui_lang;
        let mut changed = false;
        ui.label(lang.t("misc"));
        toggle(
            ui,
            &mut self.settings.great_fairy,
            lang.t("great_fairy"),
            &mut changed,
        );
        toggle(
            ui,
            &mut self.settings.farore_outdoor,
            lang.t("farore_outdoor"),
            &mut changed,
        );
        ui.horizontal_wrapped(|ui| {
            ui.label(lang.t("game_layout"));
            for layout in GameLayout::ALL {
                if ui
                    .selectable_value(&mut self.settings.game_layout, *layout, layout.as_str())
                    .changed()
                {
                    changed = true;
                }
            }
        });
        // What this port does not do yet, said on the page where the user would look for it.
        ui.separator();
        ui.label(lang.t("not_ported"));
        ui.label("  game-wide prompt download + resource table merge");
        ui.label("  GameCube UI diagram (needs the game dump)");
        ui.label("  relink / rekarina / longshot install targets and the updater");
        false || changed
    }

    fn page_about(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.heading(format!("{} {}", self.ui_lang.t("title"), shb_core::VERSION));
        ui.separator();
        ui.label(self.ui_lang.t("about_body"));
        ui.add_space(6.0);
        if let Some(payload) = &self.payload {
            ui.label(format!(
                "{}: {}",
                self.ui_lang.t("payload"),
                payload.display()
            ));
        }
        ui.label(format!("{}: {}", "title id", find::TITLE_ID));
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
            .with_inner_size([1020.0, 780.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Super Hero Boost",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
