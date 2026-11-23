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

/// A gold "?" that opens `key`'s full text in a window (`App::help`).
pub fn help_icon(
    ui: &mut egui::Ui,
    key: &'static str,
    lang: Lang,
    open: &mut Option<&'static str>,
) {
    let button = egui::Button::new(RichText::new("?").size(12.0).strong().color(theme::ACCENT))
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(1.0, theme::ACCENT_DARK))
        .corner_radius(egui::CornerRadius::same(9))
        .min_size(egui::vec2(18.0, 18.0));
    if ui.add(button).on_hover_text(short(tr(lang, key))).clicked() {
        *open = Some(key);
    }
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
