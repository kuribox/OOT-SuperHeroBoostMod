//! The two previews of the Python app (`app/remap_ui.py`) and the stamina one (`app/main.py`), drawn with egui's
//! painter instead of a Tk canvas.
//!
//! The coordinates are the Python app's, in its own units: the controller is 400 × 228 px, and the in-game cluster
//! uses the units of `core/hud.c` scaled down (`sb_xy()`), so what the window shows is where the mod draws.

use eframe::egui;
use egui::{Color32, Pos2, Rect, Stroke};
use shb_core::roles::{Button, Role, Roles};
use shb_core::settings::ButtonStyle;

// --------------------------------------------------------------------------- the mod's numbers

/// `core/hud.c`: where each physical button sits (layout units, y up).
fn hud_pos(button: Button) -> (f32, f32) {
    match button {
        Button::L => (1020.4, 872.3),
        Button::X => (1292.6, 843.0),
        Button::R => (1545.2, 872.3),
        Button::Y => (1129.6, 680.0),
        Button::A => (1455.6, 680.0),
        Button::Zr => (1677.2, 680.0),
        Button::B => (1292.6, 517.0),
        Button::Zl => (908.0, 680.0),
        // The sticks, the D-pad and the hold variants have no HUD button.
        _ => (0.0, 0.0),
    }
}

/// `core/hud.c` `g_sb_gamecube`: the Switch GameCube face-button centres.
fn hud_gamecube(button: Button) -> (f32, f32) {
    match button {
        Button::A => (1292.6, 680.0),
        Button::B => (1144.6, 612.0),
        Button::X => (1434.6, 710.0),
        Button::Y => (1254.6, 813.0),
        other => hud_pos(other),
    }
}

/// The "simple" layout: only the items, centred (`core/hud.c`).
fn hud_simple_offset(item: u8) -> (f32, f32) {
    match item {
        1 => (1120.4, 872.3),
        2 => (1445.2, 872.3),
        3 => (1677.2, 680.0),
        _ => (1292.6, 680.0),
    }
}

const ITEM_OFF: (f32, f32) = (-25.3, 25.3);

/// The long press of a face button, which has no art of its own.
fn hold_of(button: Button) -> Option<Button> {
    match button {
        Button::A => Some(Button::AHold),
        Button::B => Some(Button::BHold),
        Button::X => Some(Button::XHold),
        Button::Y => Some(Button::YHold),
        _ => None,
    }
}

/// The glyph of a button in a style (`tools/make_buttons.py` GLYPHS).
fn glyph(style: ButtonStyle, button: Button) -> &'static str {
    match style {
        ButtonStyle::Switch => match button {
            Button::A => "A",
            Button::B => "B",
            Button::X => "X",
            Button::Y => "Y",
            Button::L => "L",
            Button::R => "R",
            Button::Zl => "ZL",
            Button::Zr => "ZR",
            Button::L3 => "L3",
            Button::R3 => "R3",
            Button::Dr => "▶",
            _ => "",
        },
        ButtonStyle::Xbox => match button {
            Button::A => "B",
            Button::B => "A",
            Button::X => "Y",
            Button::Y => "X",
            Button::L => "LB",
            Button::R => "RB",
            Button::Zl => "LT",
            Button::Zr => "RT",
            Button::L3 => "LS",
            Button::R3 => "RS",
            Button::Dr => "▶",
            _ => "",
        },
        ButtonStyle::PlayStation => match button {
            Button::A => "circle",
            Button::B => "cross",
            Button::X => "triangle",
            Button::Y => "square",
            Button::L => "L1",
            Button::R => "R1",
            Button::Zl => "L2",
            Button::Zr => "R2",
            Button::L3 => "L3",
            Button::R3 => "R3",
            Button::Dr => "▶",
            _ => "",
        },
    }
}

fn ps_color(glyph: &str) -> Option<Color32> {
    Some(match glyph {
        "circle" => Color32::from_rgb(0xE0, 0x60, 0x5A),
        "cross" => Color32::from_rgb(0x7A, 0xA6, 0xE8),
        "triangle" => Color32::from_rgb(0x5F, 0xC7, 0xA0),
        "square" => Color32::from_rgb(0xD9, 0x8C, 0xC8),
        _ => return None,
    })
}

fn ps_text(glyph: &str) -> Option<&'static str> {
    Some(match glyph {
        "circle" => "○",
        "cross" => "✕",
        "triangle" => "△",
        "square" => "□",
        _ => return None,
    })
}

/// One button in the chosen style: a round face button, a stick, a shoulder tab or the D-pad arrow.
fn draw_chip(painter: &egui::Painter, at: Pos2, style: ButtonStyle, button: Button, radius: f32) {
    let outline = Stroke::new(1.0, Color32::from_rgb(0x5B, 0x5B, 0x60));
    let fill = Color32::from_rgb(0x10, 0x10, 0x12);
    match button {
        Button::A | Button::B | Button::X | Button::Y => {
            painter.circle_stroke(at, radius, outline);
            painter.circle_filled(at, radius, fill);
        }
        Button::L3 | Button::R3 => {
            painter.circle_stroke(
                at,
                radius * 1.1,
                Stroke::new(2.0, Color32::from_rgb(0x7A, 0x8A, 0x80)),
            );
            painter.circle_filled(at, radius * 1.1, Color32::from_rgb(0x0F, 0x18, 0x13));
        }
        _ => {
            let w = if button == Button::Dr {
                radius * 0.85
            } else {
                radius * 1.45
            };
            let h = if button == Button::Dr {
                radius * 0.85
            } else {
                radius * 0.8
            };
            let rect = Rect::from_center_size(at, egui::vec2(w * 2.0, h * 2.0));
            painter.rect_filled(rect, 3.0, fill);
            painter.rect_stroke(rect, 3.0, outline, egui::StrokeKind::Outside);
        }
    }
    let key = glyph(style, button);
    if let Some(color) = ps_color(key) {
        let s = radius * 0.45;
        match key {
            "circle" => {
                painter.circle_stroke(at, s, Stroke::new(2.0, color));
            }
            "cross" => {
                painter.line_segment(
                    [at + egui::vec2(-s, -s), at + egui::vec2(s, s)],
                    Stroke::new(2.0, color),
                );
                painter.line_segment(
                    [at + egui::vec2(-s, s), at + egui::vec2(s, -s)],
                    Stroke::new(2.0, color),
                );
            }
            "triangle" => {
                let points = vec![
                    at + egui::vec2(0.0, -s),
                    at + egui::vec2(-s, s * 0.8),
                    at + egui::vec2(s, s * 0.8),
                ];
                painter.add(egui::Shape::convex_polygon(
                    points,
                    Color32::TRANSPARENT,
                    Stroke::new(2.0, color),
                ));
            }
            _ => {
                let rect = Rect::from_center_size(at, egui::vec2(s * 2.0, s * 2.0));
                painter.rect_stroke(
                    rect,
                    1.0,
                    Stroke::new(2.0, color),
                    egui::StrokeKind::Outside,
                );
            }
        }
        return;
    }
    let text = ps_text(key).unwrap_or(key);
    let size = (radius * if text.len() == 1 { 0.85 } else { 0.62 }).max(7.0);
    painter.text(
        at,
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(size),
        Color32::from_rgb(0xC8, 0xC8, 0xCC),
    );
}

// --------------------------------------------------------------------------- the previews

/// One row of the pad: the button, where it sits, and where its role's name goes.
type PadSpot = (Button, (f32, f32), (f32, f32, egui::Align2));

/// The controller: an outline, every button with its glyph, and the role each one has.
pub fn pad(ui: &mut egui::Ui, roles: &Roles, style: ButtonStyle, name: impl Fn(Role) -> String) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(400.0, 228.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, crate::theme::PANEL2);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, crate::theme::BORDER),
        egui::StrokeKind::Inside,
    );

    let at = |x: f32, y: f32| rect.min + egui::vec2(x, y);
    // The body: a rounded blob, the same silhouette the Python app draws.
    let body = vec![
        at(66.0, 64.0),
        at(334.0, 64.0),
        at(376.0, 130.0),
        at(376.0, 196.0),
        at(334.0, 218.0),
        at(296.0, 186.0),
        at(104.0, 186.0),
        at(66.0, 218.0),
        at(24.0, 196.0),
        at(24.0, 130.0),
    ];
    painter.add(egui::Shape::convex_polygon(
        body,
        Color32::from_rgb(0x1B, 0x2A, 0x22),
        Stroke::new(2.0, crate::theme::BORDER),
    ));

    // Where each button is on the picture, and where its role goes.
    let spots: [PadSpot; 11] = [
        (
            Button::Y,
            (271.0, 112.0),
            (256.0, 112.0, egui::Align2::RIGHT_CENTER),
        ),
        (
            Button::A,
            (321.0, 112.0),
            (336.0, 112.0, egui::Align2::LEFT_CENTER),
        ),
        (
            Button::X,
            (296.0, 87.0),
            (296.0, 73.0, egui::Align2::CENTER_CENTER),
        ),
        (
            Button::B,
            (296.0, 137.0),
            (296.0, 151.0, egui::Align2::CENTER_CENTER),
        ),
        (
            Button::Zl,
            (56.0, 18.0),
            (80.0, 18.0, egui::Align2::LEFT_CENTER),
        ),
        (
            Button::L,
            (56.0, 44.0),
            (80.0, 44.0, egui::Align2::LEFT_CENTER),
        ),
        (
            Button::R,
            (344.0, 44.0),
            (320.0, 44.0, egui::Align2::RIGHT_CENTER),
        ),
        (
            Button::Zr,
            (344.0, 18.0),
            (320.0, 18.0, egui::Align2::RIGHT_CENTER),
        ),
        (
            Button::L3,
            (92.0, 110.0),
            (92.0, 136.0, egui::Align2::CENTER_CENTER),
        ),
        (
            Button::R3,
            (236.0, 160.0),
            (210.0, 160.0, egui::Align2::RIGHT_CENTER),
        ),
        (
            Button::Dr,
            (56.0, 168.0),
            (76.0, 168.0, egui::Align2::LEFT_CENTER),
        ),
    ];
    for (button, spot, label) in spots {
        draw_chip(&painter, at(spot.0, spot.1), style, button, 12.0);
        let role = roles.get(&button).copied().unwrap_or(Role::None);
        let mut text = name(role);
        if let Some(hold) = hold_of(button) {
            let held = roles.get(&hold).copied().unwrap_or(Role::None);
            if held != Role::None {
                text = if role == Role::None {
                    format!("↓ {}", name(held))
                } else {
                    format!("{}  ↓ {}", name(role), name(held))
                };
            }
        }
        let color = if role == Role::None {
            crate::theme::DIM
        } else if matches!(
            role,
            Role::Item1 | Role::Item2 | Role::Item3 | Role::ItemMain | Role::Selector
        ) {
            crate::theme::ACCENT
        } else {
            Color32::from_rgb(0x7F, 0xC8, 0xA9)
        };
        painter.text(
            at(label.0, label.1),
            label.2,
            text,
            egui::FontId::proportional(11.0),
            color,
        );
    }
}

/// The in-game HUD cluster, the way `core/hud.c` draws it.
pub fn hud(
    ui: &mut egui::Ui,
    roles: &Roles,
    style: ButtonStyle,
    simple: bool,
    gamecube: bool,
    name: impl Fn(Role) -> String,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(400.0, 228.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_rgb(0x10, 0x10, 0x10));
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, crate::theme::BORDER),
        egui::StrokeKind::Inside,
    );

    let scale = 0.37;
    let ox = 200.0 - 1292.6 * scale;
    let oy = 118.0 + 712.0 * scale;
    let to_px = |x: f32, y: f32| rect.min + egui::vec2(ox + x * scale, oy - y * scale);

    // The role → the button that has it.
    let button_of = |role: Role| {
        roles
            .iter()
            .find(|(button, r)| {
                **r == role
                    && !matches!(**r, Role::None)
                    && !matches!(
                        button,
                        Button::AHold | Button::BHold | Button::XHold | Button::YHold
                    )
            })
            .map(|(button, _)| *button)
    };

    let mut order: Vec<(Role, u8)> = vec![
        (Role::Item2, 2),
        (Role::Jump, 0),
        (Role::Item3, 3),
        (Role::Sword, 0),
        (Role::Action, 0),
        (Role::ItemMain, 0),
        (Role::Item1, 1),
    ];
    if roles.values().any(|r| *r == Role::Shield) {
        order.push((Role::Shield, 0));
    }
    for (role, item) in order {
        if simple && matches!(role, Role::Jump | Role::Sword | Role::Action | Role::Shield) {
            continue;
        }
        let button = match button_of(role) {
            Some(button) if !matches!(button, Button::L3 | Button::R3 | Button::Dr) => button,
            _ => continue,
        };
        let (bx, by) = if simple {
            hud_simple_offset(item)
        } else if gamecube {
            hud_gamecube(button)
        } else {
            hud_pos(button)
        };
        let centre = to_px(bx, by);
        if role == Role::Jump {
            painter.circle_filled(centre, 20.0, Color32::from_rgb(0x10, 0x10, 0x12));
            painter.circle_stroke(
                centre,
                20.0,
                Stroke::new(1.0, Color32::from_rgb(0x5B, 0x5B, 0x60)),
            );
            painter.text(
                centre,
                egui::Align2::CENTER_CENTER,
                name(Role::Jump),
                egui::FontId::proportional(8.0),
                Color32::from_rgb(0xE8, 0xE8, 0xE8),
            );
            continue;
        }
        draw_chip(&painter, centre, style, button, 20.0);
        let mark = to_px(bx + ITEM_OFF.0 * 1.9, by + ITEM_OFF.1 * 1.9);
        match role {
            Role::Sword => {
                // A blade: a thin line with a guard, drawn instead of a glyph the fonts may not have.
                let tip = mark + egui::vec2(7.0, -7.0);
                let hilt = mark + egui::vec2(-5.0, 5.0);
                painter.line_segment(
                    [hilt, tip],
                    Stroke::new(2.0, Color32::from_rgb(0xDF, 0xE6, 0xEE)),
                );
                painter.line_segment(
                    [mark + egui::vec2(-6.0, 1.0), mark + egui::vec2(-1.0, 6.0)],
                    Stroke::new(2.0, Color32::from_rgb(0xDF, 0xE6, 0xEE)),
                );
            }
            Role::Shield => {
                let (w, h) = (9.0, 11.0);
                let points = vec![
                    mark + egui::vec2(-w, -h),
                    mark + egui::vec2(w, -h),
                    mark + egui::vec2(w, 1.0),
                    mark + egui::vec2(0.0, h),
                    mark + egui::vec2(-w, 1.0),
                ];
                painter.add(egui::Shape::convex_polygon(
                    points,
                    Color32::from_rgb(0x3C, 0x5F, 0xA0),
                    Stroke::new(2.0, Color32::from_rgb(0xC9, 0xD6, 0xEE)),
                ));
            }
            other => {
                let r = Rect::from_center_size(mark, egui::vec2(20.0, 20.0));
                painter.rect_filled(r, 1.0, Color32::from_rgb(0x3A, 0x2F, 0x12));
                painter.rect_stroke(
                    r,
                    1.0,
                    Stroke::new(2.0, crate::theme::ACCENT),
                    egui::StrokeKind::Inside,
                );
                match other {
                    Role::ItemMain => {
                        // The main item: a diamond, the way the game marks it.
                        let points = vec![
                            mark + egui::vec2(0.0, -6.0),
                            mark + egui::vec2(6.0, 0.0),
                            mark + egui::vec2(0.0, 6.0),
                            mark + egui::vec2(-6.0, 0.0),
                        ];
                        painter.add(egui::Shape::convex_polygon(
                            points,
                            crate::theme::ACCENT,
                            Stroke::NONE,
                        ));
                    }
                    _ => {
                        let text = if other == Role::Item1 {
                            "1"
                        } else if other == Role::Item2 {
                            "2"
                        } else {
                            "3"
                        };
                        painter.text(
                            mark,
                            egui::Align2::CENTER_CENTER,
                            text,
                            egui::FontId::proportional(11.0),
                            crate::theme::ACCENT,
                        );
                    }
                }
            }
        }
    }
}

/// The stamina preview: a ring, a segmented bar, or "OFF" (`app/main.py`).
pub fn stamina(ui: &mut egui::Ui, on: bool, bar: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(262.0, 122.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, crate::theme::PANEL2);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, crate::theme::BORDER),
        egui::StrokeKind::Inside,
    );
    if bar {
        let color = if on {
            crate::theme::ACCENT
        } else {
            crate::theme::DISABLED
        };
        for i in 0..16 {
            let x = 35.0 + i as f32 * 12.0;
            let r = Rect::from_min_size(rect.min + egui::vec2(x, 55.0), egui::vec2(11.0, 12.0));
            painter.rect_filled(r, 1.0, color);
        }
    } else {
        let color = if on {
            crate::theme::TEAL
        } else {
            crate::theme::DISABLED
        };
        // The ring: a thick circle, the way core/hud.c draws the stamina meter.
        let centre = rect.min + egui::vec2(131.0, 59.0);
        painter.circle_stroke(centre, 38.0, Stroke::new(8.0, color));
    }
    if !on {
        painter.text(
            rect.min + egui::vec2(214.0, 18.0),
            egui::Align2::RIGHT_CENTER,
            "OFF",
            egui::FontId::proportional(10.0),
            crate::theme::DISABLED,
        );
    }
}
