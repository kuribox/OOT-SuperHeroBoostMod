//! The previews of the Python app (`app/remap_ui.py`, and the stamina one in `app/main.py`), drawn with egui's
//! painter instead of a Tk canvas.
//!
//! Every preview is drawn in the Python app's own units — the controller and the HUD cluster in a 400 × 228 box, the
//! stamina meter in 262 × 122 — and then scaled to the width the column gives it, so the same drawing serves a narrow
//! window and a wide one. The HUD positions are the ones of `core/hud.c` (`sb_xy()`), so what the window shows is
//! where the mod draws.

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
        _ => (1292.6, 680.0),
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

// --------------------------------------------------------------------------- glyphs

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

// --------------------------------------------------------------------------- the scaled painter

/// A preview's box: its own unit space mapped onto the room the column gives it.
struct Canvas {
    painter: egui::Painter,
    origin: Pos2,
    scale: f32,
    clip: Rect,
}

impl Canvas {
    fn new(ui: &mut egui::Ui, units: egui::Vec2, max_width: f32) -> Canvas {
        let width = max_width.min(ui.available_width()).max(140.0);
        let scale = width / units.x;
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(width, units.y * scale), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, egui::CornerRadius::same(6), crate::theme::PANEL2);
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(6),
            Stroke::new(1.0, crate::theme::BORDER),
            egui::StrokeKind::Inside,
        );
        Canvas {
            painter,
            origin: rect.min,
            scale,
            clip: rect,
        }
    }

    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.origin + egui::vec2(x * self.scale, y * self.scale)
    }

    fn font(&self, size: f32) -> egui::FontId {
        egui::FontId::proportional((size * self.scale).clamp(6.0, 24.0))
    }

    /// Text, left out when it would not fit inside the box.
    fn text(
        &self,
        at: Pos2,
        anchor: egui::Align2,
        text: impl Into<String>,
        size: f32,
        color: Color32,
    ) {
        let galley = self
            .painter
            .layout_no_wrap(text.into(), self.font(size), color);
        let rect = anchor.anchor_size(at, galley.size());
        if !self.clip.contains_rect(rect) {
            return;
        }
        self.painter.galley(rect.min, galley, color);
    }

    fn circle(&self, at: Pos2, radius: f32, fill: Color32, stroke: Stroke) {
        self.painter.circle_filled(at, radius * self.scale, fill);
        self.painter.circle_stroke(at, radius * self.scale, stroke);
    }

    fn rect(&self, centre: Pos2, half: egui::Vec2, fill: Color32, stroke: Stroke, round: f32) {
        let rect = Rect::from_center_size(centre, half * 2.0 * self.scale);
        self.painter.rect_filled(rect, round, fill);
        if stroke != Stroke::NONE {
            self.painter
                .rect_stroke(rect, round, stroke, egui::StrokeKind::Outside);
        }
    }

    fn line(&self, from: Pos2, to: Pos2, stroke: Stroke) {
        self.painter.line_segment(
            [from, to],
            Stroke::new(stroke.width * self.scale, stroke.color),
        );
    }

    fn polygon(&self, points: Vec<Pos2>, fill: Color32, stroke: Stroke) {
        self.painter
            .add(egui::Shape::convex_polygon(points, fill, stroke));
    }
}

/// One button in the chosen style: a round face button, a stick, a shoulder tab or the D-pad arrow.
fn draw_chip(paint: &Canvas, at: Pos2, style: ButtonStyle, button: Button, radius: f32) {
    let outline = Stroke::new(1.0, Color32::from_rgb(0x5B, 0x5B, 0x60));
    let fill = Color32::from_rgb(0x10, 0x10, 0x12);
    match button {
        Button::A | Button::B | Button::X | Button::Y => {
            paint.circle(at, radius, fill, outline);
        }
        Button::L3 | Button::R3 => {
            paint.circle(
                at,
                radius * 1.05,
                Color32::from_rgb(0x0F, 0x18, 0x13),
                Stroke::new(2.0, Color32::from_rgb(0x7A, 0x8A, 0x80)),
            );
        }
        _ => {
            let w = if button == Button::Dr {
                radius * 0.8
            } else {
                radius * 1.35
            };
            let h = if button == Button::Dr {
                radius * 0.8
            } else {
                radius * 0.72
            };
            paint.rect(at, egui::vec2(w, h), fill, outline, 3.0);
        }
    }
    let key = glyph(style, button);
    if let Some(color) = ps_color(key) {
        let s = radius * 0.45;
        match key {
            "circle" => {
                paint.painter.circle_stroke(
                    at,
                    s * paint.scale,
                    Stroke::new(2.0 * paint.scale, color),
                );
            }
            "cross" => {
                paint.line(
                    at + egui::vec2(-s, -s) * paint.scale,
                    at + egui::vec2(s, s) * paint.scale,
                    Stroke::new(2.0, color),
                );
                paint.line(
                    at + egui::vec2(-s, s) * paint.scale,
                    at + egui::vec2(s, -s) * paint.scale,
                    Stroke::new(2.0, color),
                );
            }
            "triangle" => paint.polygon(
                vec![
                    at + egui::vec2(0.0, -s) * paint.scale,
                    at + egui::vec2(-s, s * 0.8) * paint.scale,
                    at + egui::vec2(s, s * 0.8) * paint.scale,
                ],
                Color32::TRANSPARENT,
                Stroke::new(1.6, color),
            ),
            _ => {
                paint.rect(
                    at,
                    egui::vec2(s, s),
                    Color32::TRANSPARENT,
                    Stroke::new(1.6, color),
                    1.0,
                );
            }
        }
        return;
    }
    let text = ps_text(key).unwrap_or(key);
    let size = if text.len() == 1 { 13.0 } else { 9.0 };
    paint.text(
        at,
        egui::Align2::CENTER_CENTER,
        text,
        size,
        Color32::from_rgb(0xC8, 0xC8, 0xCC),
    );
}

// --------------------------------------------------------------------------- the previews

/// One row of the pad: the button, where it sits, and where its role's name goes.
type PadSpot = (Button, (f32, f32), (f32, f32, egui::Align2));

/// The controller: an outline, every button with its glyph, and the role each one has.
pub fn pad(ui: &mut egui::Ui, roles: &Roles, style: ButtonStyle, name: impl Fn(Role) -> String) {
    let paint = Canvas::new(ui, egui::vec2(400.0, 240.0), 420.0);
    let body = vec![
        paint.at(66.0, 72.0),
        paint.at(334.0, 72.0),
        paint.at(376.0, 138.0),
        paint.at(376.0, 204.0),
        paint.at(334.0, 226.0),
        paint.at(296.0, 194.0),
        paint.at(104.0, 194.0),
        paint.at(66.0, 226.0),
        paint.at(24.0, 204.0),
        paint.at(24.0, 138.0),
    ];
    paint.polygon(
        body,
        Color32::from_rgb(0x1B, 0x2A, 0x22),
        Stroke::new(1.5, crate::theme::BORDER),
    );

    let spots: [PadSpot; 11] = [
        (
            Button::Y,
            (271.0, 120.0),
            (258.0, 120.0, egui::Align2::RIGHT_CENTER),
        ),
        (
            Button::A,
            (321.0, 120.0),
            (334.0, 120.0, egui::Align2::LEFT_CENTER),
        ),
        (
            Button::X,
            (296.0, 95.0),
            (296.0, 80.0, egui::Align2::CENTER_BOTTOM),
        ),
        (
            Button::B,
            (296.0, 145.0),
            (296.0, 160.0, egui::Align2::CENTER_TOP),
        ),
        (
            Button::Zl,
            (56.0, 26.0),
            (78.0, 26.0, egui::Align2::LEFT_CENTER),
        ),
        (
            Button::L,
            (56.0, 52.0),
            (78.0, 52.0, egui::Align2::LEFT_CENTER),
        ),
        (
            Button::R,
            (344.0, 52.0),
            (322.0, 52.0, egui::Align2::RIGHT_CENTER),
        ),
        (
            Button::Zr,
            (344.0, 26.0),
            (322.0, 26.0, egui::Align2::RIGHT_CENTER),
        ),
        (
            Button::L3,
            (96.0, 120.0),
            (96.0, 144.0, egui::Align2::CENTER_TOP),
        ),
        (
            Button::R3,
            (240.0, 168.0),
            (222.0, 168.0, egui::Align2::RIGHT_CENTER),
        ),
        (
            Button::Dr,
            (56.0, 176.0),
            (76.0, 176.0, egui::Align2::LEFT_CENTER),
        ),
    ];
    for (button, spot, label) in spots {
        draw_chip(&paint, paint.at(spot.0, spot.1), style, button, 11.0);
        let role = roles.get(&button).copied().unwrap_or(Role::None);
        let mut text = name(role);
        if let Some(hold) = hold_of(button) {
            let held = roles.get(&hold).copied().unwrap_or(Role::None);
            if held != Role::None {
                text = if role == Role::None {
                    format!("↓ {}", name(held))
                } else {
                    format!("{} ↓ {}", name(role), name(held))
                };
            }
        }
        let color = if role == Role::None {
            crate::theme::DISABLED
        } else if matches!(
            role,
            Role::Item1 | Role::Item2 | Role::Item3 | Role::ItemMain | Role::Selector
        ) {
            crate::theme::ACCENT
        } else {
            Color32::from_rgb(0x8F, 0xD2, 0xB4)
        };
        paint.text(paint.at(label.0, label.1), label.2, text, 8.5, color);
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
    let paint = Canvas::new(ui, egui::vec2(400.0, 228.0), 420.0);
    paint.painter.rect_filled(
        Rect::from_min_size(paint.origin, paint.clip.size()),
        6.0,
        Color32::from_rgb(0x0C, 0x0C, 0x0E),
    );

    // `core/hud.c` units (y up) onto the box: the whole cluster centred, as the Python app does it.
    let ox = 200.0 - 1292.6 * 0.37;
    let oy = 118.0 + 712.0 * 0.37;
    let to_px = |x: f32, y: f32| paint.at(ox + x * 0.37, oy - y * 0.37);

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
            paint.circle(
                centre,
                15.0,
                Color32::from_rgb(0x10, 0x10, 0x12),
                Stroke::new(1.0, Color32::from_rgb(0x5B, 0x5B, 0x60)),
            );
            paint.text(
                centre,
                egui::Align2::CENTER_CENTER,
                name(Role::Jump),
                6.5,
                Color32::from_rgb(0xE8, 0xE8, 0xE8),
            );
            continue;
        }
        draw_chip(&paint, centre, style, button, 15.0);
        let mark = to_px(bx + ITEM_OFF.0 * 1.9, by + ITEM_OFF.1 * 1.9);
        match role {
            Role::Sword => {
                paint.line(
                    mark + egui::vec2(-5.0, 5.0) * paint.scale,
                    mark + egui::vec2(6.0, -6.0) * paint.scale,
                    Stroke::new(1.6, Color32::from_rgb(0xDF, 0xE6, 0xEE)),
                );
            }
            Role::Shield => {
                let (w, h) = (8.0, 10.0);
                paint.polygon(
                    vec![
                        mark + egui::vec2(-w, -h) * paint.scale,
                        mark + egui::vec2(w, -h) * paint.scale,
                        mark + egui::vec2(w, 1.0) * paint.scale,
                        mark + egui::vec2(0.0, h) * paint.scale,
                        mark + egui::vec2(-w, 1.0) * paint.scale,
                    ],
                    Color32::from_rgb(0x3C, 0x5F, 0xA0),
                    Stroke::new(1.2, Color32::from_rgb(0xC9, 0xD6, 0xEE)),
                );
            }
            other => {
                paint.rect(
                    mark,
                    egui::vec2(9.0, 9.0) * paint.scale,
                    Color32::from_rgb(0x3A, 0x2F, 0x12),
                    Stroke::new(1.4, crate::theme::ACCENT),
                    2.0,
                );
                match other {
                    Role::ItemMain => {
                        paint.polygon(
                            vec![
                                mark + egui::vec2(0.0, -5.0) * paint.scale,
                                mark + egui::vec2(5.0, 0.0) * paint.scale,
                                mark + egui::vec2(0.0, 5.0) * paint.scale,
                                mark + egui::vec2(-5.0, 0.0) * paint.scale,
                            ],
                            crate::theme::ACCENT,
                            Stroke::NONE,
                        );
                    }
                    _ => {
                        let text = if other == Role::Item1 {
                            "1"
                        } else if other == Role::Item2 {
                            "2"
                        } else {
                            "3"
                        };
                        paint.text(
                            mark,
                            egui::Align2::CENTER_CENTER,
                            text,
                            9.0,
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
    let paint = Canvas::new(ui, egui::vec2(262.0, 122.0), 262.0);
    if bar {
        let color = if on {
            crate::theme::ACCENT
        } else {
            crate::theme::DISABLED
        };
        for i in 0..16 {
            paint.rect(
                paint.at(35.0 + i as f32 * 12.0 + 5.5, 61.0),
                egui::vec2(5.5, 6.0),
                color,
                Stroke::NONE,
                1.0,
            );
        }
    } else {
        let color = if on {
            crate::theme::TEAL
        } else {
            crate::theme::DISABLED
        };
        paint.painter.circle_stroke(
            paint.at(131.0, 61.0),
            38.0 * paint.scale,
            Stroke::new(8.0 * paint.scale, color),
        );
    }
    if !on {
        paint.text(
            paint.at(212.0, 20.0),
            egui::Align2::RIGHT_CENTER,
            "OFF",
            10.0,
            crate::theme::DISABLED,
        );
    }
}

#[cfg(test)]
mod geometry {
    /// `app/remap_ui.py` `_draw_hud`: the layout units of `core/hud.c` mapped onto the 400 × 228 box.
    fn to_px(x: f32, y: f32) -> (f32, f32) {
        let scale = 0.37;
        (
            200.0 - 1292.6 * scale + x * scale,
            118.0 + 712.0 * scale - y * scale,
        )
    }

    #[test]
    fn the_cluster_lands_inside_the_box() {
        for (x, y) in [
            (1020.4, 872.3),
            (1292.6, 843.0),
            (1545.2, 872.3),
            (1129.6, 680.0),
            (1455.6, 680.0),
            (1677.2, 680.0),
            (1292.6, 517.0),
            (908.0, 680.0),
        ] {
            let (px, py) = to_px(x, y);
            assert!(
                (0.0..400.0).contains(&px) && (0.0..228.0).contains(&py),
                "({x}, {y}) -> ({px}, {py})"
            );
        }
        let (px, py) = to_px(1292.6, 680.0);
        assert!(
            (px - 200.0).abs() < 0.5 && (py - 129.84).abs() < 0.5,
            "({px}, {py})"
        );
    }
}
