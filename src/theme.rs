//! Visual theme and small custom widgets for the Phone Audio UI.
//!
//! The goal is a calm, dense "control panel" look: dark background, soft cards,
//! one accent color, clear status colors, and generous but consistent spacing.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Margin, Response, Sense,
    Stroke, TextStyle, Ui, Vec2,
};

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub log_bg: Color32,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(0x0D, 0x0F, 0x14),
            panel: Color32::from_rgb(0x11, 0x14, 0x1B),
            card: Color32::from_rgb(0x16, 0x1A, 0x24),
            card_hover: Color32::from_rgb(0x1C, 0x21, 0x30),
            border: Color32::from_rgb(0x24, 0x2B, 0x3A),
            border_strong: Color32::from_rgb(0x32, 0x3C, 0x50),
            text: Color32::from_rgb(0xE8, 0xEC, 0xF4),
            text_dim: Color32::from_rgb(0x9A, 0xA4, 0xB8),
            text_faint: Color32::from_rgb(0x68, 0x71, 0x86),
            accent: Color32::from_rgb(0x6C, 0x8C, 0xFF),
            accent_hover: Color32::from_rgb(0x8A, 0xA3, 0xFF),
            accent_soft: Color32::from_rgba_unmultiplied(0x6C, 0x8C, 0xFF, 34),
            on_accent: Color32::from_rgb(0x0A, 0x0D, 0x18),
            success: Color32::from_rgb(0x3F, 0xD6, 0x9B),
            warning: Color32::from_rgb(0xF2, 0xB2, 0x4C),
            danger: Color32::from_rgb(0xFF, 0x6B, 0x72),
            log_bg: Color32::from_rgb(0x09, 0x0B, 0x10),
        }
    }

    pub fn light() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(0xF2, 0xF5, 0xFA),
            panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            card: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            card_hover: Color32::from_rgb(0xF6, 0xF8, 0xFD),
            border: Color32::from_rgb(0xE0, 0xE6, 0xF0),
            border_strong: Color32::from_rgb(0xC9, 0xD3, 0xE3),
            text: Color32::from_rgb(0x1B, 0x22, 0x30),
            text_dim: Color32::from_rgb(0x5B, 0x65, 0x77),
            text_faint: Color32::from_rgb(0x8B, 0x94, 0xA6),
            accent: Color32::from_rgb(0x4C, 0x6F, 0xFF),
            accent_hover: Color32::from_rgb(0x6B, 0x86, 0xFF),
            accent_soft: Color32::from_rgba_unmultiplied(0x4C, 0x6F, 0xFF, 26),
            on_accent: Color32::WHITE,
            success: Color32::from_rgb(0x17, 0xA9, 0x7A),
            warning: Color32::from_rgb(0xD9, 0x8A, 0x1F),
            danger: Color32::from_rgb(0xE5, 0x48, 0x4D),
            log_bg: Color32::from_rgb(0xEC, 0xF1, 0xF8),
        }
    }

    pub fn for_ui(ui: &Ui) -> Self {
        if ui.visuals().dark_mode {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

// ---------------------------------------------------------------------------
// Theme installation
// ---------------------------------------------------------------------------

/// Apply the custom styles and select `system` / `dark` / `light`.
pub fn apply(ctx: &egui::Context, preference: &str) {
    ctx.set_style_of(egui::Theme::Dark, build_style(Palette::dark()));
    ctx.set_style_of(egui::Theme::Light, build_style(Palette::light()));
    ctx.set_theme(match preference {
        "dark" => egui::ThemePreference::Dark,
        "light" => egui::ThemePreference::Light,
        _ => egui::ThemePreference::System,
    });
}

fn build_style(p: Palette) -> egui::Style {
    let mut style = egui::Style::default();

    // Typography -----------------------------------------------------------
    use TextStyle::*;
    let mut text_styles = std::collections::BTreeMap::new();
    text_styles.insert(Heading, FontId::new(24.0, FontFamily::Proportional));
    text_styles.insert(
        TextStyle::Name("CardTitle".into()),
        FontId::new(15.0, FontFamily::Proportional),
    );
    text_styles.insert(
        TextStyle::Name("Subtitle".into()),
        FontId::new(11.5, FontFamily::Proportional),
    );
    text_styles.insert(Body, FontId::new(13.5, FontFamily::Proportional));
    text_styles.insert(Button, FontId::new(13.5, FontFamily::Proportional));
    text_styles.insert(Small, FontId::new(11.0, FontFamily::Proportional));
    text_styles.insert(Monospace, FontId::new(11.5, FontFamily::Monospace));
    style.text_styles = text_styles;

    // Spacing --------------------------------------------------------------
    let spacing = &mut style.spacing;
    spacing.item_spacing = Vec2::new(10.0, 8.0);
    spacing.window_margin = Margin::same(16);
    spacing.button_padding = Vec2::new(12.0, 6.0);
    spacing.menu_margin = Margin::same(8);
    spacing.indent = 16.0;
    spacing.interact_size = Vec2::new(38.0, 30.0);
    spacing.slider_width = 150.0;
    spacing.combo_width = 150.0;
    spacing.text_edit_width = 220.0;
    spacing.extra_text_line_spacing = 2.0;

    style.animation_time = 0.18;

    // Visuals --------------------------------------------------------------
    let mut v = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = if p.dark {
        Color32::from_rgb(0x0A, 0x0C, 0x11)
    } else {
        Color32::from_rgb(0xF4, 0xF7, 0xFC)
    };
    v.faint_bg_color = p.card_hover;
    v.code_bg_color = p.log_bg;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.weak_text_alpha = 0.8;
    v.weak_text_color = Some(p.text_dim);
    v.button_frame = true;
    v.collapsing_header_frame = false;
    v.indent_has_left_vline = false;
    v.slider_trailing_fill = true;
    v.striped = false;

    v.selection.bg_fill = with_alpha(p.accent, 60);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.ime_composition.active_underline_stroke = Stroke::new(1.0, p.accent);
    v.ime_composition.inactive_underline_stroke = Stroke::new(1.0, with_alpha(p.accent, 160));

    let w = |bg_fill, weak_bg_fill, stroke, radius, fg: Color32| egui::style::WidgetVisuals {
        bg_fill,
        weak_bg_fill,
        bg_stroke: stroke,
        corner_radius: CornerRadius::same(radius),
        fg_stroke: Stroke::new(1.25, fg),
        expansion: 0.0,
    };

    v.widgets.noninteractive = w(
        p.card,
        Color32::TRANSPARENT,
        Stroke::new(1.0, p.border),
        9,
        p.text,
    );
    v.widgets.inactive = w(p.card, p.card, Stroke::new(1.0, p.border), 9, p.text_dim);
    v.widgets.hovered = w(
        p.card_hover,
        p.card_hover,
        Stroke::new(1.0, p.border_strong),
        9,
        p.text,
    );
    v.widgets.active = w(
        p.accent_soft,
        p.accent_soft,
        Stroke::new(1.0, p.accent),
        9,
        p.accent,
    );
    v.widgets.open = w(
        p.card_hover,
        p.card_hover,
        Stroke::new(1.0, p.border_strong),
        9,
        p.text,
    );

    style.visuals = v;
    style
}

// ---------------------------------------------------------------------------
// Widgets
// ---------------------------------------------------------------------------

/// A rounded card that fills the available width.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_min_height(ui, 0.0, add)
}

/// A card with a minimum outer height; extra space is added after the content.
pub fn card_min_height<R>(ui: &mut Ui, min_outer_height: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::for_ui(ui);
    egui::Frame::NONE
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let start = ui.cursor().top();
            let result = add(ui);
            let used = ui.cursor().top() - start;
            let target = (min_outer_height - 28.0).max(0.0);
            if target > used {
                ui.add_space(target - used);
            }
            result
        })
        .inner
}

/// A slightly recessed card, used for the log viewer.
pub fn inset<R>(ui: &mut Ui, fill: Color32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::for_ui(ui);
    egui::Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn section_title(ui: &mut Ui, title: &str, hint: Option<&str>) {
    let p = Palette::for_ui(ui);
    ui.horizontal(|ui| {
        let (bar, _) = ui.allocate_exact_size(Vec2::new(3.0, 14.0), Sense::hover());
        ui.painter()
            .rect_filled(bar, CornerRadius::same(2), p.accent);
        ui.add_space(7.0);
        ui.label(egui::RichText::new(title).size(14.5).strong().color(p.text));
        if let Some(hint) = hint {
            ui.add_space(2.0);
            ui.label(egui::RichText::new(hint).size(11.0).color(p.text_faint));
        }
    });
}

pub fn subtle_label(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = Palette::for_ui(ui);
    ui.label(
        egui::RichText::new(text.into())
            .size(11.0)
            .color(p.text_faint),
    )
}

/// Colored status pill with a small dot.
pub fn status_pill(ui: &mut Ui, text: &str, color: Color32, pulse: bool) -> Response {
    let alpha = if pulse { 100 } else { 70 };
    let bg = with_alpha(color, if pulse { 26 } else { 22 });
    let stroke = with_alpha(color, alpha);
    egui::Frame::NONE
        .fill(bg)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(CornerRadius::same(99))
        .inner_margin(Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("● {text}"))
                    .size(11.5)
                    .strong()
                    .color(color),
            );
        })
        .response
}

/// Smaller pill used inside the compact device row.
pub fn status_pill_compact(ui: &mut Ui, text: &str, color: Color32) -> Response {
    egui::Frame::NONE
        .fill(with_alpha(color, 22))
        .stroke(Stroke::new(1.0, with_alpha(color, 70)))
        .corner_radius(CornerRadius::same(99))
        .inner_margin(Margin::symmetric(7, 1))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(format!("● {text}"))
                    .size(10.5)
                    .strong()
                    .color(color),
            );
        })
        .response
}
/// Big primary action button, painted by hand for a consistent look.
pub fn primary_button(
    ui: &mut Ui,
    label: &str,
    width: f32,
    enabled: bool,
    danger: bool,
) -> Response {
    let p = Palette::for_ui(ui);
    let desired = Vec2::new(width, 40.0);
    let (rect, response) = ui.allocate_exact_size(
        desired,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );

    if ui.is_rect_visible(rect) {
        let hovered = enabled && response.hovered();
        let fill = if !enabled {
            with_alpha(p.border, 120)
        } else if danger {
            if hovered {
                Color32::from_rgb(0xFF, 0x85, 0x8A)
            } else {
                p.danger
            }
        } else if hovered {
            p.accent_hover
        } else {
            p.accent
        };
        let rounding = CornerRadius::same(11);
        ui.painter().rect_filled(rect, rounding, fill);
        let text_color = if enabled { p.on_accent } else { p.text_faint };
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::new(15.5, FontFamily::Proportional),
            text_color,
        );
    }

    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Compact segmented control. Returns the index of the clicked item.
pub fn segmented(ui: &mut Ui, items: &[(&str, bool)]) -> Option<usize> {
    let p = Palette::for_ui(ui);
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (index, (label, selected)) in items.iter().enumerate() {
            let (fill, stroke, color) = if *selected {
                (p.accent_soft, Stroke::new(1.0, p.accent), p.accent)
            } else {
                (Color32::TRANSPARENT, Stroke::new(1.0, p.border), p.text_dim)
            };
            let response = ui.add(
                egui::Button::new(egui::RichText::new(*label).size(12.0).color(color))
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(CornerRadius::same(8))
                    .min_size(Vec2::new(0.0, 26.0)),
            );
            if response.clicked() {
                clicked = Some(index);
            }
        }
    });
    clicked
}

/// A tiny switch-like checkbox row.
pub fn switch(ui: &mut Ui, checked: &mut bool, label: &str) {
    let p = Palette::for_ui(ui);
    ui.horizontal(|ui| {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(30.0, 17.0), Sense::click());
        if ui.is_rect_visible(rect) {
            let fill = if *checked {
                p.accent
            } else {
                with_alpha(p.border, 160)
            };
            ui.painter().rect_filled(rect, CornerRadius::same(9), fill);
            let knob_x = if *checked {
                rect.right() - 8.5
            } else {
                rect.left() + 8.5
            };
            let knob = egui::pos2(knob_x, rect.center().y);
            ui.painter().circle_filled(
                knob,
                6.0,
                if *checked { p.on_accent } else { p.text_faint },
            );
        }
        if response.clicked() {
            *checked = !*checked;
        }
        let label_response = ui.add(
            egui::Button::new(egui::RichText::new(label).size(12.5).color(p.text_dim)).frame(false),
        );
        if label_response.clicked() {
            *checked = !*checked;
        }
    });
}

/// Log line text color.
pub fn log_color(line: &str, p: &Palette) -> Color32 {
    if line.starts_with('$') {
        p.accent
    } else if line.contains("ERROR")
        || line.contains("error")
        || line.contains("错误")
        || line.contains("失败")
    {
        p.danger
    } else if line.contains("WARN") || line.contains("警告") {
        p.warning
    } else if line.contains("音频已开始") || line.contains("检测完成") || line.contains("已停止")
    {
        p.success
    } else {
        p.text_dim
    }
}

// ---------------------------------------------------------------------------
// App icon
// ---------------------------------------------------------------------------

/// Generates a small RGBA icon (rounded accent square + white headphone).
pub fn app_icon() -> egui::IconData {
    const SIZE: usize = 64;
    let accent = (0x6C, 0x8C, 0xFF);
    let accent2 = (0x4C, 0x6F, 0xFF);
    let mut rgba = vec![0u8; SIZE * SIZE * 4];

    let rounded_rect = |x: f32, y: f32, left: f32, top: f32, right: f32, bottom: f32, r: f32| {
        let cx = x.clamp(left + r, right - r);
        let cy = y.clamp(top + r, bottom - r);
        let dx = x - cx;
        let dy = y - cy;
        dx * dx + dy * dy <= r * r && x >= left && x <= right && y >= top && y <= bottom
    };

    for y in 0..SIZE {
        for x in 0..SIZE {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;

            // Background: rounded square with a soft diagonal gradient.
            if !rounded_rect(fx, fy, 3.0, 3.0, 61.0, 61.0, 15.0) {
                continue;
            }
            let t = (fx + fy) / 128.0;
            let r = lerp_u8(accent.0, accent2.0, t);
            let g = lerp_u8(accent.1, accent2.1, t);
            let b = lerp_u8(accent.2, accent2.2, t);

            // Headphone band: upper half of a ring.
            let dx = fx - 32.0;
            let dy = fy - 34.0;
            let dist = (dx * dx + dy * dy).sqrt();
            let band = fy <= 35.0 && (17.0..=21.0).contains(&dist);

            // Ear cups.
            let cup_left = rounded_rect(fx, fy, 13.0, 32.0, 23.0, 51.0, 5.0);
            let cup_right = rounded_rect(fx, fy, 41.0, 32.0, 51.0, 51.0, 5.0);

            let (pr, pg, pb) = if band || cup_left || cup_right {
                (255u8, 255u8, 255u8)
            } else {
                (r, g, b)
            };
            let index = (y * SIZE + x) * 4;
            rgba[index] = pr;
            rgba[index + 1] = pg;
            rgba[index + 2] = pb;
            rgba[index + 3] = 255;
        }
    }

    egui::IconData {
        rgba,
        width: SIZE as u32,
        height: SIZE as u32,
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)).round() as u8
}
