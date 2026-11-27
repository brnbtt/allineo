//! Visual language (shared with comprimo): near-black layered surfaces, hairline borders, white
//! as the only accent, Segoe UI in three weights.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, Ui,
    Vec2,
};

pub const BG: Color32 = Color32::from_gray(12);
pub const SURFACE: Color32 = Color32::from_gray(19);
pub const SURFACE_2: Color32 = Color32::from_gray(28);
pub const BORDER: Color32 = Color32::from_gray(35);
pub const BORDER_STRONG: Color32 = Color32::from_gray(58);
pub const TEXT: Color32 = Color32::from_gray(238);
pub const TEXT_2: Color32 = Color32::from_gray(156);
pub const TEXT_3: Color32 = Color32::from_gray(102);
pub const ACCENT: Color32 = Color32::from_gray(246);
pub const ON_ACCENT: Color32 = Color32::from_gray(14);

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn light() -> FontFamily {
    FontFamily::Name("light".into())
}

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.faint_bg_color = SURFACE;
    v.extreme_bg_color = Color32::from_gray(9);
    v.hyperlink_color = TEXT;
    v.selection.bg_fill = Color32::from_gray(72);
    v.selection.stroke = Stroke::new(1.0, TEXT);
    v.text_cursor.stroke = Stroke::new(1.5, TEXT);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    for (w, bg, stroke) in [
        (&mut v.widgets.inactive, SURFACE_2, BORDER),
        (&mut v.widgets.hovered, Color32::from_gray(36), BORDER_STRONG),
        (&mut v.widgets.active, Color32::from_gray(44), Color32::from_gray(90)),
        (&mut v.widgets.open, Color32::from_gray(36), BORDER_STRONG),
    ] {
        w.bg_fill = bg;
        w.weak_bg_fill = bg;
        w.bg_stroke = Stroke::new(1.0, stroke);
        w.fg_stroke = Stroke::new(1.0, TEXT);
        w.corner_radius = CornerRadius::same(8);
        w.expansion = 0.0;
    }
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.global_style_mut(|s| {
        use egui::TextStyle::*;
        s.text_styles = [
            (Small, FontId::new(12.0, FontFamily::Proportional)),
            (Body, FontId::new(14.0, FontFamily::Proportional)),
            (Button, FontId::new(14.0, FontFamily::Proportional)),
            (Heading, FontId::new(22.0, semibold())),
            (Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(14.0, 6.0);
        s.spacing.interact_size.y = 30.0;
    });
}

/// Windows' own Segoe UI in regular, semibold and semilight, with Segoe UI Symbol for glyphs.
fn install_fonts(ctx: &egui::Context) {
    let dir =
        std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("Fonts");
    let mut defs = egui::FontDefinitions::default();
    let mut load = |name: &str, file: &str| -> Option<String> {
        let bytes = std::fs::read(dir.join(file)).ok()?;
        defs.font_data.insert(name.into(), Arc::new(egui::FontData::from_owned(bytes)));
        Some(name.to_string())
    };
    let regular = load("segoe", "segoeui.ttf");
    let bold = load("segoe-semibold", "seguisb.ttf");
    let thin = load("segoe-semilight", "segoeuisl.ttf");
    let symbol = load("segoe-symbol", "seguisym.ttf");

    let fallback = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |first: &Option<String>| -> Vec<String> {
        [first, &regular, &symbol].into_iter().flatten().cloned().chain(fallback.iter().cloned()).collect()
    };
    defs.families.insert(FontFamily::Proportional, family(&None));
    defs.families.insert(semibold(), family(&bold));
    defs.families.insert(light(), family(&thin));
    ctx.set_fonts(defs);
}

// ---------------------------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------------------------

/// A 1px divider spanning the available width.
pub fn hairline(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, BORDER));
}

pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(ON_ACCENT).family(semibold()))
        .fill(ACCENT)
        .stroke(Stroke::NONE)
        .corner_radius(8)
        .min_size(Vec2::new(84.0, 32.0))
}

pub fn secondary(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(TEXT))
        .fill(SURFACE_2)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(8)
        .min_size(Vec2::new(0.0, 32.0))
}

pub fn ghost(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(TEXT_2)).frame(false)
}

/// Small round button with a single glyph; only shows a background on hover.
pub fn round_button(ui: &mut Ui, glyph: &str, tip: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(glyph).size(15.0).color(TEXT_2))
            .frame_when_inactive(false)
            .corner_radius(13)
            .min_size(Vec2::splat(26.0)),
    )
    .on_hover_text(tip)
}

/// `−  12 px  +`. Returns which button was clicked as (minus, plus).
pub fn stepper(ui: &mut Ui, value: &str, less: &str, more: &str) -> (bool, bool) {
    // Inside right-aligned layouts egui adds widgets right to left; keep "− value +" reading order.
    let reversed = ui.layout().prefer_right_to_left();
    let (mut minus, mut plus) = (false, false);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let number = |ui: &mut Ui| {
            ui.add_sized([58.0, 26.0], egui::Label::new(RichText::new(value).color(TEXT)));
        };
        if reversed {
            plus = round_button(ui, "+", more).clicked();
            number(ui);
            minus = round_button(ui, "−", less).clicked();
        } else {
            minus = round_button(ui, "−", less).clicked();
            number(ui);
            plus = round_button(ui, "+", more).clicked();
        }
    });
    (minus, plus)
}

/// A rounded group of rows, iOS-settings style.
pub fn group<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, Color32::from_gray(27)))
        .corner_radius(14)
        .inner_margin(Margin::symmetric(16, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A quiet lowercase caption above a group, inset to line up with the rows' content.
pub fn caption(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(RichText::new(text).size(12.0).color(TEXT_3));
    });
    ui.add_space(-2.0);
}

/// Small glyphs shown at the start of settings rows (16 × 16 points), drawn in the same block
/// language as the app icon.
#[derive(Clone, Copy)]
pub enum RowGlyph {
    /// Two windows side by side.
    OtherWindows,
    /// Two stacked windows with their left edges lined up.
    Matching,
    /// A window tucked into the corner of a screen.
    Screen,
    /// A window whose edge stops at a neighbour.
    Resize,
    /// A horseshoe magnet.
    Hold,
    /// Motion lines.
    Flick,
    /// A key cap with the ⌃ (control) mark.
    Key,
    /// The power symbol.
    Startup,
    /// An icon in the taskbar.
    Tray,
    /// A circular arrow.
    Restore,
}

pub const GLYPH_SIZE: f32 = 16.0;
const GLYPH_INK: Color32 = Color32::from_gray(214);
const GLYPH_DIM: Color32 = Color32::from_gray(54);

pub fn glyph_ink() -> Color32 {
    GLYPH_INK
}

pub fn row_glyph(ui: &mut Ui, glyph: RowGlyph, ink: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(GLYPH_SIZE), Sense::hover());
    let p = ui.painter();
    let o = rect.min;
    let block = |x0: f32, y0: f32, x1: f32, y1: f32, color: Color32| {
        p.rect_filled(Rect::from_min_max(o + Vec2::new(x0, y0), o + Vec2::new(x1, y1)), CornerRadius::same(1), color);
    };
    let stroke = Stroke::new(1.5, ink);
    let at = |x: f32, y: f32| o + Vec2::new(x, y);
    // Points along a circle (degrees, clockwise on screen since y points down).
    let arc = |c: Pos2, r: f32, from: f32, to: f32| -> Vec<Pos2> {
        let steps = 20;
        (0..=steps)
            .map(|i| {
                let a = (from + (to - from) * i as f32 / steps as f32).to_radians();
                c + Vec2::new(a.cos(), a.sin()) * r
            })
            .collect()
    };
    match glyph {
        RowGlyph::OtherWindows => {
            block(1.0, 2.0, 7.0, 14.0, ink);
            block(9.0, 2.0, 15.0, 11.0, ink);
        }
        RowGlyph::Matching => {
            block(1.0, 2.0, 15.0, 7.0, ink);
            block(1.0, 9.0, 10.0, 14.0, ink);
        }
        RowGlyph::Screen => {
            block(1.0, 2.0, 15.0, 14.0, GLYPH_DIM);
            block(1.0, 2.0, 9.0, 9.0, ink);
        }
        RowGlyph::Resize => {
            block(1.0, 2.0, 8.0, 14.0, ink);
            block(12.0, 2.0, 15.0, 14.0, GLYPH_DIM);
            p.line_segment([at(9.0, 8.0), at(11.0, 8.0)], stroke);
            p.line_segment([at(9.5, 6.0), at(11.0, 8.0)], stroke);
            p.line_segment([at(9.5, 10.0), at(11.0, 8.0)], stroke);
        }
        RowGlyph::Hold => {
            block(2.0, 2.0, 6.0, 4.5, ink);
            block(10.0, 2.0, 14.0, 4.5, ink);
            block(2.0, 5.5, 6.0, 14.0, ink);
            block(10.0, 5.5, 14.0, 14.0, ink);
            block(2.0, 10.5, 14.0, 14.0, ink);
        }
        RowGlyph::Flick => {
            block(6.0, 3.0, 15.0, 5.0, ink);
            block(2.0, 7.0, 15.0, 9.0, ink);
            block(9.0, 11.0, 15.0, 13.0, ink);
        }
        RowGlyph::Key => {
            let key = Rect::from_min_max(at(1.5, 2.5), at(14.5, 13.5));
            p.rect_stroke(key, CornerRadius::same(3), stroke, egui::StrokeKind::Middle);
            p.line_segment([at(5.5, 9.0), at(8.0, 6.5)], stroke);
            p.line_segment([at(8.0, 6.5), at(10.5, 9.0)], stroke);
        }
        RowGlyph::Startup => {
            p.add(Shape::line(arc(at(8.0, 8.8), 5.6, -55.0, 235.0), stroke));
            p.line_segment([at(8.0, 1.8), at(8.0, 7.6)], stroke);
        }
        RowGlyph::Tray => {
            block(1.0, 10.0, 15.0, 14.0, GLYPH_DIM);
            block(10.0, 11.0, 13.0, 13.0, ink);
            block(5.0, 3.0, 11.0, 8.0, ink);
        }
        RowGlyph::Restore => {
            let c = at(8.0, 8.0);
            let pts = arc(c, 5.5, 40.0, 320.0);
            let tip = *pts.last().unwrap();
            p.add(Shape::line(pts, stroke));
            p.line_segment([tip, tip + Vec2::new(-3.2, -0.6)], stroke);
            p.line_segment([tip, tip + Vec2::new(0.6, -3.2)], stroke);
        }
    }
}

/// A whole-width clickable row (used for "reset to mac feel"): brightens on hover.
pub fn action_row(ui: &mut Ui, height: f32, glyph: RowGlyph, text: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let hovered = response.hovered();
    let color = if hovered { TEXT } else { TEXT_2 };
    let mut child =
        ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    row_glyph(&mut child, glyph, color);
    child.add_space(12.0);
    child.label(RichText::new(text).color(color));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A button-sized pill that fills up as work progresses, with its label inverting over the fill.
/// `progress = None` shows an indeterminate shimmer.
pub fn progress_pill(ui: &mut Ui, height: f32, progress: Option<f32>, label: &str, time: f64) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let p = ui.painter();
    let radius = CornerRadius::same(8);
    p.rect(rect, radius, SURFACE_2, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
    let font = FontId::new(14.0, semibold());
    let fill_w = progress.map_or(0.0, |v| rect.width() * v.clamp(0.0, 1.0));
    let fill = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
    if fill_w > 0.5 {
        p.with_clip_rect(fill).rect_filled(rect, radius, ACCENT);
    }
    if progress.is_none() {
        // Indeterminate: a soft highlight sweeping across.
        let band = rect.width() * 0.35;
        let x = rect.left() - band + ((time * 0.6).fract() as f32) * (rect.width() + band * 2.0);
        let mut mesh = egui::Mesh::default();
        for (sx, alpha) in [(x - band, 0u8), (x, 22u8), (x + band, 0u8)] {
            let c = Color32::from_white_alpha(alpha);
            mesh.colored_vertex(Pos2::new(sx, rect.top()), c);
            mesh.colored_vertex(Pos2::new(sx, rect.bottom()), c);
        }
        for i in 0..2u32 {
            let b = i * 2;
            mesh.add_triangle(b, b + 1, b + 2);
            mesh.add_triangle(b + 1, b + 2, b + 3);
        }
        p.with_clip_rect(rect.shrink(1.0)).add(Shape::mesh(mesh));
    }
    p.text(rect.center(), egui::Align2::CENTER_CENTER, label, font.clone(), TEXT);
    if fill_w > 0.5 {
        p.with_clip_rect(fill).text(rect.center(), egui::Align2::CENTER_CENTER, label, font, ON_ACCENT);
    }
}

/// The finished state of [`progress_pill`]: solid white with a check, "installed".
pub fn done_pill(ui: &mut Ui, height: f32, label: &str, t: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(8), ACCENT);
    let font = FontId::new(14.0, semibold());
    let galley = p.layout_no_wrap(label.to_string(), font, ON_ACCENT);
    let (check, gap) = (16.0, 8.0);
    let total = check + gap + galley.size().x;
    let left = rect.center().x - total / 2.0;
    let c = Pos2::new(left + check / 2.0, rect.center().y);
    draw_check(p, c, check / 2.0, t, ON_ACCENT, 1.8);
    p.galley(Pos2::new(left + check + gap, rect.center().y - galley.size().y / 2.0), galley, ON_ACCENT);
}

/// A check mark that draws itself in as `t` goes 0 → 1, centered on `c` with half-size `s`.
fn draw_check(p: &egui::Painter, c: Pos2, s: f32, t: f32, color: Color32, width: f32) {
    let ease = 1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3);
    let pts =
        [c + Vec2::new(-0.62 * s, 0.02 * s), c + Vec2::new(-0.18 * s, 0.46 * s), c + Vec2::new(0.66 * s, -0.44 * s)];
    let seg1 = (pts[1] - pts[0]).length();
    let seg2 = (pts[2] - pts[1]).length();
    let drawn = ease * (seg1 + seg2);
    let stroke = Stroke::new(width, color);
    if drawn <= seg1 {
        p.line_segment([pts[0], pts[0] + (pts[1] - pts[0]) * (drawn / seg1)], stroke);
    } else {
        p.line_segment([pts[0], pts[1]], stroke);
        p.line_segment([pts[1], pts[1] + (pts[2] - pts[1]) * ((drawn - seg1) / seg2)], stroke);
    }
}

/// A soft glow behind the top of the window.
pub fn top_glow(ui: &Ui) {
    let full = ui.clip_rect();
    let rect = Rect::from_min_size(full.min, Vec2::new(full.width(), 220.0));
    let mut mesh = egui::Mesh::default();
    let (top, bottom) = (Color32::from_gray(26), BG);
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    ui.painter().with_clip_rect(full).add(Shape::mesh(mesh));
}

/// The logo with a soft drop shadow.
pub fn logo_with_shadow(ui: &mut Ui, tex: &egui::TextureHandle, size: f32) {
    egui::Frame::new()
        .corner_radius((size * 0.23) as u8)
        .shadow(egui::Shadow { offset: [0, 6], blur: 22, spread: 0, color: Color32::from_black_alpha(160) })
        .show(ui, |ui| logo(ui, tex, size));
}

pub fn logo(ui: &mut Ui, tex: &egui::TextureHandle, size: f32) {
    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), Vec2::splat(size))));
}

pub fn load_logo(ctx: &egui::Context) -> egui::TextureHandle {
    let px = 128;
    let image = egui::ColorImage::from_rgba_unmultiplied([px, px], &crate::icon::rgba(px as u32));
    ctx.load_texture("logo", image, egui::TextureOptions::LINEAR)
}

/// iOS-style switch.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let size = Vec2::new(38.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.14);
    let track = lerp_color(SURFACE_2, ACCENT, t);
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(11),
        track,
        Stroke::new(1.0, lerp_color(BORDER_STRONG, ACCENT, t)),
        egui::StrokeKind::Inside,
    );
    let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
    painter.circle_filled(Pos2::new(x, rect.center().y), 8.0, lerp_color(TEXT_2, ON_ACCENT, t));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}
