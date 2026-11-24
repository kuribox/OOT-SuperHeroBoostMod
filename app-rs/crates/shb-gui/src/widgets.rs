//! The small pieces every page is built from: the dim line under an option, the gold "?" that opens the full
//! explanation, the labelled dropdowns and the percent sliders.
//!
//! The Python app (`app/ui_kit.py`, `app/main.py`) has the same vocabulary — a card, a control, its one-sentence
//! description and a "?" with the rest — so a page here reads like a page there.

use crate::i18n::tr;
use crate::theme;
use eframe::egui;
use egui::RichText;
use shb_core::settings::Lang;

/// The one-sentence description under a control: dim, small, and wrapped to the card.
pub fn desc(ui: &mut egui::Ui, text: &str) {
    if text.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.add_space(2.0);
        ui.label(RichText::new(text).size(12.0).color(theme::DIM));
    });
}

/// The same, indented under a checkbox.
pub fn desc_under(ui: &mut egui::Ui, text: &str) {
    if text.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.add_space(18.0);
        ui.label(RichText::new(text).size(12.0).color(theme::DIM));
    });
}

/// Which of the two columns a page is drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Left,
    Right,
}

/// Two columns when the window is wide enough for them, one under the other when it is not.
///
/// The body runs once per column, so a page keeps its own borrows: `|ui, column| match column { … }`.
/// `min_width` is where two columns still make sense; the left one keeps `share` of it, the right one the rest,
/// never less than `right_min`.
pub fn two_columns(
    ui: &mut egui::Ui,
    min_width: f32,
    share: f32,
    right_min: f32,
    mut body: impl FnMut(&mut egui::Ui, Column),
) {
    let total = ui.available_width();
    if total >= min_width {
        let left_width = (total * share)
            .min(total - right_min - 16.0)
            .max(total * 0.4);
        ui.horizontal_top(|ui| {
            let mut first = true;
            ui.allocate_ui_with_layout(
                egui::vec2(left_width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    if first {
                        body(ui, Column::Left);
                    }
                    first = false;
                },
            );
            ui.add_space(16.0);
            ui.vertical(|ui| body(ui, Column::Right));
        });
    } else {
        body(ui, Column::Left);
        ui.add_space(12.0);
        body(ui, Column::Right);
    }
}

/// A small five-pointed star, drawn: the fonts in this window do not all carry "★", and a profile that is only a
/// name without its star is not the same profile to the person looking at it.
pub fn star(ui: &mut egui::Ui, size: f32, color: egui::Color32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(size + 2.0, size + 2.0), egui::Sense::hover());
    let centre = rect.center();
    let outer = size * 0.5;
    let inner = outer * 0.45;
    let mut points = Vec::with_capacity(10);
    for i in 0..10 {
        let radius = if i % 2 == 0 { outer } else { inner };
        let angle = std::f32::consts::PI * (i as f32) / 5.0 - std::f32::consts::FRAC_PI_2;
        points.push(centre + egui::vec2(angle.cos(), angle.sin()) * radius);
    }
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
}

/// A gold "?" that opens `key`'s full text in a window (`App::help`).
pub fn help_icon(
    ui: &mut egui::Ui,
    key: &'static str,
    lang: Lang,
    open: &mut Option<&'static str>,
) -> egui::Response {
    // No frame: a gold "?" on the panel, quiet until the pointer is over it, and close to its text — the button's
    // padding and the item spacing are what made it look spaced out from the sentence it belongs to.
    let button = egui::Button::new(RichText::new("?").size(12.0).strong().color(theme::ACCENT))
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius::same(8))
        .min_size(egui::vec2(14.0, 14.0))
        .small();
    let clicked = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.spacing_mut().button_padding = egui::vec2(1.0, 0.0);
            ui.add(button).on_hover_text(short(tr(lang, key)))
        })
        .inner;
    if clicked.clicked() {
        *open = Some(key);
    }
    clicked
}

/// The flag of a language, drawn: the system fonts do not carry the flag emoji.
pub fn flag(ui: &mut egui::Ui, lang: Lang) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(19.0, 13.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 1.0, egui::Color32::from_rgb(0x20, 0x20, 0x22));
    let stripe = |y0: f32, y1: f32, color: egui::Color32| {
        let r = egui::Rect::from_min_max(
            egui::pos2(rect.min.x, rect.min.y + rect.height() * y0),
            egui::pos2(rect.max.x, rect.min.y + rect.height() * y1),
        );
        painter.rect_filled(r, 0.0, color);
    };
    let band = |x0: f32, x1: f32, color: egui::Color32| {
        let r = egui::Rect::from_min_max(
            egui::pos2(rect.min.x + rect.width() * x0, rect.min.y),
            egui::pos2(rect.min.x + rect.width() * x1, rect.max.y),
        );
        painter.rect_filled(r, 0.0, color);
    };
    let red = egui::Color32::from_rgb(0xC8, 0x1E, 0x2E);
    let yellow = egui::Color32::from_rgb(0xF2, 0xC5, 0x2E);
    let blue = egui::Color32::from_rgb(0x1E, 0x3F, 0xA8);
    let white = egui::Color32::from_rgb(0xF2, 0xF2, 0xF2);
    let green = egui::Color32::from_rgb(0x1E, 0x7A, 0x3C);
    let black = egui::Color32::from_rgb(0x1A, 0x1A, 0x1A);
    match lang {
        // Spain: red, yellow, red.
        Lang::Es => {
            stripe(0.0, 0.25, red);
            stripe(0.25, 0.75, yellow);
            stripe(0.75, 1.0, red);
        }
        // Portugal: green, red.
        Lang::Pt => {
            band(0.0, 0.4, green);
            band(0.4, 1.0, red);
        }
        // France: blue, white, red.
        Lang::Fr => {
            band(0.0, 0.34, blue);
            band(0.34, 0.66, white);
            band(0.66, 1.0, red);
        }
        // Germany: black, red, gold.
        Lang::De => {
            stripe(0.0, 0.34, black);
            stripe(0.34, 0.66, red);
            stripe(0.66, 1.0, yellow);
        }
        // Italy: green, white, red.
        Lang::It => {
            band(0.0, 0.34, green);
            band(0.34, 0.66, white);
            band(0.66, 1.0, red);
        }
        // English: the Union Jack, as close as three lines get.
        Lang::En => {
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(0x0A, 0x1E, 0x5A));
            let cross = |thickness: f32, color: egui::Color32| {
                let w = rect.width() * thickness;
                let h = rect.height() * thickness;
                painter.rect_filled(
                    egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width(), h)),
                    0.0,
                    color,
                );
                painter.rect_filled(
                    egui::Rect::from_center_size(rect.center(), egui::vec2(w, rect.height())),
                    0.0,
                    color,
                );
            };
            cross(0.22, white);
            cross(0.11, red);
        }
    }
    painter.rect_stroke(
        rect,
        1.0,
        egui::Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
}

/// The first sentence of a long text: what the "?" shows on hover, with the rest in its window.
pub fn short(text: &str) -> String {
    match text.find(['.', '!', '?']) {
        Some(at) => text[..=at].to_string(),
        None => text.to_string(),
    }
}

/// A checkbox with its description under it, and its "?" beside the label.
pub fn check(
    ui: &mut egui::Ui,
    value: &mut bool,
    label: &str,
    note: &str,
    help: Option<&'static str>,
    open: &mut Option<&'static str>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui.checkbox(value, label).changed() {
            changed = true;
        }
        if let Some(key) = help {
            help_icon(ui, key, Lang::En, open);
        }
    });
    desc_under(ui, note);
    changed
}

/// A labelled dropdown. `options` are (value, text) and the current value decides the text shown.
pub fn combo_row<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    current: &mut T,
    options: &[(T, String)],
    width: f32,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        if !label.is_empty() {
            ui.label(RichText::new(format!("{label}:")).size(13.0));
        }
        let selected = options
            .iter()
            .find(|(value, _)| value == current)
            .map(|(_, text)| text.clone())
            .unwrap_or_default();
        egui::ComboBox::from_id_salt(id)
            .selected_text(selected)
            .width(width)
            .show_ui(ui, |ui| {
                for (value, text) in options {
                    if ui.selectable_value(current, *value, text).changed() {
                        changed = true;
                    }
                }
            });
    });
    changed
}

/// A percent slider with the value beside it, in the Python app's steps (50..200 by 5 for the HUD).
pub fn percent_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut u8,
    min: u8,
    max: u8,
    step: u8,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{label}:")).size(13.0));
        let mut current = *value as i32;
        let response = ui.add(
            egui::Slider::new(&mut current, min as i32..=max as i32)
                .step_by(step as f64)
                .show_value(false),
        );
        ui.label(
            RichText::new(format!("{current} %"))
                .size(13.0)
                .strong()
                .color(theme::ACCENT),
        );
        if response.changed() {
            *value = current.clamp(min as i32, max as i32) as u8;
            changed = true;
        }
    });
    changed
}

/// The three difficulty multipliers, named the way the Python app names them ("Vanilla", "×1.5", …).
pub fn mult_options(lang: Lang) -> Vec<(u16, String)> {
    vec![
        (100, tr(lang, "mult_van").to_string()),
        (150, tr(lang, "mult_15").to_string()),
        (200, tr(lang, "mult_2").to_string()),
        (300, tr(lang, "mult_3").to_string()),
    ]
}

/// The sprint speeds: "Vanilla" and then the multiplier, as the Python app's dropdown.
pub fn speed_options(lang: Lang) -> Vec<(u16, String)> {
    let mut out = vec![(100, tr(lang, "speed_van").to_string())];
    for value in [125u16, 150, 175, 200, 250, 300] {
        out.push((value, format!("×{}", trim(value as f32 / 100.0))));
    }
    out
}

/// `1.50` → `1.5`, `2.00` → `2`: the way the multipliers are written.
pub fn trim(value: f32) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.to_string()
}

/// The enemy levels: "Off" and then `×1.25` … `×6`.
pub fn enemy_options(lang: Lang) -> Vec<(u8, String)> {
    let mut out = vec![(0u8, tr(lang, "enemy_off").to_string())];
    for (level, factor) in shb_core::enemy::LEVELS {
        out.push((level, format!("×{}", trim(factor as f32))));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run one egui frame with `events` and return the rects the dropdown drew (its button, then its items).
    fn frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        value: &mut u16,
        rects: &mut Vec<egui::Rect>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            events,
            ..Default::default()
        };
        let options: [(u16, String); 3] = [
            (100, "one".into()),
            (150, "one and a half".into()),
            (200, "two".into()),
        ];
        let captured = std::cell::RefCell::new(Vec::new());
        // `run_ui` gives the test a Ui of its own: the same thing a page gets inside the panel.
        let mut output = ctx.run_ui(input, |ui| {
            {
                let out = egui::ComboBox::from_id_salt("test-combo")
                    .selected_text("one")
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        for (option, text) in &options {
                            let response = ui.selectable_value(value, *option, text);
                            captured.borrow_mut().push(response.rect);
                        }
                    });
                captured.borrow_mut().insert(0, out.response.rect);
            };
        });
        output.textures_delta.clear(); // the test has no painter to hand them to
        rects.clear();
        rects.extend(captured.into_inner());
    }

    fn click(at: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ]
    }

    #[test]
    fn the_multiplier_dropdown_selects_with_a_click() {
        let ctx = egui::Context::default();
        let mut value: u16 = 100;
        let mut rects = Vec::new();

        // The first frame draws the closed dropdown.
        frame(&ctx, vec![], &mut value, &mut rects);
        let button = rects[0];

        // A click on it opens the list, and the same frame reports where every item is.
        frame(&ctx, click(button.center()), &mut value, &mut rects);
        // The popup is a layer of its own: it takes a frame with the pointer over it before it takes a click.
        frame(
            &ctx,
            vec![egui::Event::PointerMoved(rects[3].center())],
            &mut value,
            &mut rects,
        );
        assert!(
            rects.len() >= 4,
            "the popup did not draw its items: {}",
            rects.len()
        );

        // A click on the third item ("two") picks it.
        let item = rects[3];
        frame(&ctx, click(item.center()), &mut value, &mut rects);
        assert_eq!(value, 200, "the dropdown did not write the chosen value");
    }
}
