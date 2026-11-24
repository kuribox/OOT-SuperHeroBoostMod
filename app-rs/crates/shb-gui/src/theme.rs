//! The look of the app: the Python app's palette and fonts (`app/ui_kit.py`), mapped onto egui.
//!
//! The Python app draws a dark forest page with gold accents, rounded cards with a 1 px border and a 3 px accent
//! rule on top, and Segoe UI for the text with Palatino Linotype for titles. This module is the only place that
//! knows any of that, so a page only says "card", "primary button" or "section title".

use eframe::egui;
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Stroke};

// --------------------------------------------------------------------------- palette
pub const BG: Color32 = Color32::from_rgb(0x0A, 0x14, 0x10);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x0C, 0x1A, 0x13);
pub const PANEL: Color32 = Color32::from_rgb(0x13, 0x26, 0x1C);
pub const PANEL2: Color32 = Color32::from_rgb(0x0C, 0x1A, 0x14);
pub const CARD: Color32 = Color32::from_rgb(0x16, 0x2B, 0x20);
pub const BORDER: Color32 = Color32::from_rgb(0x2A, 0x48, 0x36);
pub const ACCENT: Color32 = Color32::from_rgb(0xE7, 0xBA, 0x4B);
pub const ACCENT2: Color32 = Color32::from_rgb(0xF2, 0xD2, 0x7A);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0x7A, 0x5C, 0x1F);
pub const TEAL: Color32 = Color32::from_rgb(0x63, 0xC7, 0x9A);
pub const TEXT: Color32 = Color32::from_rgb(0xF5, 0xEF, 0xDD);
pub const DIM: Color32 = Color32::from_rgb(0xA8, 0xB8, 0xAA);
#[allow(dead_code)] // the whole Python palette is kept here, so a page never invents a colour
pub const DISABLED: Color32 = Color32::from_rgb(0x6B, 0x7C, 0x70);
pub const RED: Color32 = Color32::from_rgb(0xE0, 0x6A, 0x5A);
pub const AMBER: Color32 = Color32::from_rgb(0xF2, 0xB4, 0x5E);
#[allow(dead_code)]
pub const HELP_BG: Color32 = Color32::from_rgb(0x2B, 0x24, 0x12);
pub const ROW_HOVER: Color32 = Color32::from_rgb(0x24, 0x40, 0x2E);
/// Text on top of the gold.
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x1A, 0x14, 0x08);

/// The family of the titles (Palatino Linotype in the Python app).
pub const DISPLAY: &str = "display";

/// A title in the display face.
pub fn title(size: f32, text: impl Into<String>) -> RichText {
    RichText::new(text)
        .family(FontFamily::Name(DISPLAY.into()))
        .size(size)
        .color(TEXT)
}

/// A small dimmed line under a title.
pub fn hint(text: impl Into<String>) -> RichText {
    RichText::new(text).size(11.5).color(DIM)
}

/// The visual style and the fonts. Called once, when the window is created.
pub fn install(ctx: &egui::Context) {
    let mut style = egui::Style::default();
    let mut visuals = egui::Visuals::dark();

    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = PANEL2;
    visuals.faint_bg_color = PANEL;
    visuals.code_bg_color = PANEL2;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.menu_corner_radius = CornerRadius::same(10);
    // A chosen item anywhere (a list, a combo's popup) is dark gold with light gold text: the gold fill with cream
    // text it used to be was unreadable.
    visuals.selection.bg_fill = ACCENT_DARK;
    visuals.selection.stroke = Stroke::new(1.5, ACCENT2);
    visuals.hyperlink_color = ACCENT;
    // 5 / 1: every interactive widget shows the hand, the way the Python app's buttons do.
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.warn_fg_color = AMBER;
    visuals.error_fg_color = RED;

    let widget =
        |bg: Color32, border: Color32, fg: Color32, radius: u8| egui::style::WidgetVisuals {
            bg_fill: bg,
            weak_bg_fill: bg,
            bg_stroke: Stroke::new(1.0, border),
            fg_stroke: Stroke::new(1.0, fg),
            corner_radius: CornerRadius::same(radius),
            expansion: 0.0,
        };
    visuals.widgets.noninteractive = widget(PANEL, BORDER, DIM, 8);
    visuals.widgets.inactive = widget(PANEL2, BORDER, TEXT, 8);
    visuals.widgets.hovered = widget(ROW_HOVER, ACCENT_DARK, ACCENT2, 8);
    visuals.widgets.active = widget(ACCENT_DARK, ACCENT, ACCENT2, 8);
    visuals.widgets.open = widget(ROW_HOVER, BORDER, TEXT, 8);

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(8.0, 5.0);
    style.spacing.button_padding = egui::vec2(9.0, 4.0);
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.interact_size.y = 22.0;
    style.spacing.slider_width = 120.0;
    style.spacing.combo_width = 110.0;
    style.spacing.indent = 14.0;
    style.text_styles = [
        (
            egui::TextStyle::Small,
            FontId::new(10.5, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Heading,
            FontId::new(17.0, FontFamily::Name(DISPLAY.into())),
        ),
        (
            egui::TextStyle::Monospace,
            FontId::new(11.5, FontFamily::Monospace),
        ),
    ]
    .into();
    // egui 0.37 keeps one style per theme: ours goes into both, and the window asks for the dark one.
    ctx.all_styles_mut(|current| *current = style.clone());
    ctx.set_theme(egui::Theme::Dark);

    load_fonts(ctx);
}

/// Segoe UI for the text and Palatino Linotype for the titles, when Windows has them: the same two faces the
/// Python app asks for.
fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut families = Vec::new();
    for (key, file) in [("segoe", "segoeui.ttf"), ("palatino", "pala.ttf")] {
        let path = std::path::Path::new("C:/Windows/Fonts").join(file);
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        fonts.font_data.insert(
            key.to_string(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        families.push(key);
    }
    if families.contains(&"segoe") {
        // First in the list, with the built-in font kept as the fallback for glyphs Segoe UI does not have.
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe".to_string());
    }
    for family in [FontFamily::Proportional, FontFamily::Name(DISPLAY.into())] {
        let list = fonts.families.entry(family).or_default();
        for fallback in ["NotoEmoji-Regular", "emoji-icon-font"] {
            if fonts.font_data.contains_key(fallback) && !list.iter().any(|f| f == fallback) {
                list.push(fallback.to_string());
            }
        }
    }
    if families.contains(&"palatino") {
        // The display face, with the fonts that carry the symbols Segoe UI and Palatino lack: the star of the
        // recommended profile ("Recomendado ★") was invisible without them.
        let mut list = vec!["palatino".to_string()];
        if families.contains(&"segoe") {
            list.push("segoe".to_string());
        }
        for fallback in ["NotoEmoji-Regular", "emoji-icon-font"] {
            if fonts.font_data.contains_key(fallback) {
                list.push(fallback.to_string());
            }
        }
        fonts
            .families
            .insert(FontFamily::Name(DISPLAY.into()), list);
    }
    ctx.set_fonts(fonts);
}

/// A card: the panel colour, a 1 px border, 16 px corners and the 3 px accent rule on top.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::NONE
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            let width = ui.available_width();
            let (rule, _) = ui.allocate_exact_size(egui::vec2(width, 2.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rule, CornerRadius::same(2), ACCENT);
            ui.add_space(6.0);
            add(ui)
        })
        .inner
}

/// A card with a display-font title and an optional dimmed line under it.
pub fn section<R>(
    ui: &mut egui::Ui,
    heading: &str,
    description: Option<&str>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    card(ui, |ui| {
        ui.label(title(15.0, heading));
        if let Some(text) = description {
            ui.label(hint(text));
        }
        ui.add_space(3.0);
        add(ui)
    })
}

/// The main action of a page: gold background, dark text.
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let button = egui::Button::new(RichText::new(text).color(ON_ACCENT).strong())
        .fill(ACCENT)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(10))
        .min_size(egui::vec2(0.0, 28.0));
    ui.add(button)
}

/// The quiet action: the panel colour and a border.
pub fn ghost_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let button = egui::Button::new(RichText::new(text).color(TEXT))
        .fill(PANEL2)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .min_size(egui::vec2(0.0, 28.0));
    ui.add(button)
}

/// The logo (`app/assets/shb.png`) as a texture, for the header and the window icon.
pub fn logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../app/assets/shb.png"
    ));
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let pixels = image.into_raw();
    let color = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Some(ctx.load_texture("logo", color, egui::TextureOptions::LINEAR))
}

/// The same PNG as the window icon (the .ico of the Python app is Windows-only).
pub fn icon() -> Option<egui::IconData> {
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../app/assets/shb.png"
    ));
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    Some(egui::IconData {
        rgba: image.clone().into_raw(),
        width: image.width(),
        height: image.height(),
    })
}

/// A primary button that can be disabled (a running build, an action that needs the mod installed).
pub fn primary_button_enabled(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    if !enabled {
        return ui.add_enabled(
            false,
            egui::Button::new(egui::RichText::new(text).color(DIM))
                .fill(PANEL2)
                .corner_radius(egui::CornerRadius::same(8)),
        );
    }
    primary_button(ui, text)
}

/// A quiet button that can be disabled.
pub fn ghost_button_enabled(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    if !enabled {
        return ui.add_enabled(
            false,
            egui::Button::new(egui::RichText::new(text).color(DIM))
                .fill(egui::Color32::TRANSPARENT)
                .stroke(egui::Stroke::new(1.0, BORDER))
                .corner_radius(egui::CornerRadius::same(8)),
        );
    }
    ghost_button(ui, text)
}
