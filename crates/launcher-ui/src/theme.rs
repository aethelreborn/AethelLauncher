//! Theme + design system.
//!
//! Single source of truth for colour, spacing, motion and widget chrome, per
//! [08 · UI design](../../../opencode-docs/08-ui-design.md). Screens never paint
//! a raw hex literal or re-implement hover/press logic — they call the helpers
//! here so every control shares one interaction state machine.

use eframe::egui;
use egui::{
    Align2, Color32, CornerRadius, FontId, Rect, Response, RichText, Sense, Stroke, StrokeKind,
    Vec2,
};

// ── Colour tokens (§2.1) ────────────────────────────────────────────────────
pub const BG: Color32 = Color32::from_rgb(11, 11, 15);
pub const BG_RAISE: Color32 = Color32::from_rgb(21, 21, 28);
pub const BG_HOVER: Color32 = Color32::from_rgb(29, 29, 40);
pub const BORDER: Color32 = Color32::from_rgb(38, 38, 54);

pub const ACCENT: Color32 = Color32::from_rgb(108, 92, 231);
pub const ACCENT_2: Color32 = Color32::from_rgb(0, 210, 255);
pub const SUCCESS: Color32 = Color32::from_rgb(46, 204, 113);
pub const WARN: Color32 = Color32::from_rgb(243, 156, 18);
pub const DANGER: Color32 = Color32::from_rgb(231, 76, 60);

pub const TEXT: Color32 = Color32::from_rgb(236, 239, 244);
pub const TEXT_DIM: Color32 = Color32::from_rgb(138, 143, 163);

/// Ink used on top of saturated status fills — white on `success`/`warn`/
/// `accent2` fails contrast (08 §5.2), near-black passes.
pub const INK: Color32 = Color32::from_rgb(11, 11, 15);

// Legacy aliases kept so existing screens keep compiling.
pub const BG_DARK: Color32 = BG;
pub const SURFACE: Color32 = BG_RAISE;
pub const SURFACE_LIGHT: Color32 = BG_HOVER;
pub const ACCENT_VIOLET: Color32 = ACCENT;
pub const ACCENT_CYAN: Color32 = ACCENT_2;
pub const TEXT_PRIMARY: Color32 = TEXT;
pub const TEXT_SECONDARY: Color32 = TEXT_DIM;
pub const SUCCESS_GREEN: Color32 = SUCCESS;
pub const ORANGE: Color32 = WARN;
pub const DANGER_RED: Color32 = DANGER;

// ── Spacing grid: everything is a multiple of 4 (§2.3) ─────────────────────
pub const SPACE_1: f32 = 4.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;
pub const SPACE_5: f32 = 24.0;
pub const SPACE_6: f32 = 32.0;

// ── Motion tokens (§2.6), in seconds ───────────────────────────────────────
pub const MOTION_FAST: f32 = 0.12;
pub const MOTION_BASE: f32 = 0.18;
pub const MOTION_SLOW: f32 = 0.34;

/// Reduced-motion flag. When set, every animation collapses to an instant
/// state change (08 §5.3).
#[derive(Debug, Clone, Copy, Default)]
pub struct MotionPrefs {
    pub reduced: bool,
}

impl MotionPrefs {
    fn duration(&self, seconds: f32) -> f32 {
        if self.reduced {
            0.0
        } else {
            seconds
        }
    }
}

/// Installed once at startup; read back by helpers that need the flag.
#[derive(Debug, Clone, Copy, Default)]
struct InstalledTheme {
    prefs: MotionPrefs,
}

fn theme_memo() -> &'static std::sync::Mutex<InstalledTheme> {
    use std::sync::{Mutex, OnceLock};
    static MEMO: OnceLock<Mutex<InstalledTheme>> = OnceLock::new();
    MEMO.get_or_init(|| Mutex::new(InstalledTheme::default()))
}

pub fn motion_prefs() -> MotionPrefs {
    theme_memo().lock().map(|m| m.prefs).unwrap_or_default()
}

pub fn set_motion_prefs(prefs: MotionPrefs) {
    if let Ok(mut memo) = theme_memo().lock() {
        memo.prefs = prefs;
    }
}

/// Animate a bool to 0..1 over `seconds`, honouring reduced motion.
pub fn animate(ui: &egui::Ui, id: egui::Id, target: bool, seconds: f32) -> f32 {
    let duration = motion_prefs().duration(seconds);
    if duration <= 0.0 {
        return if target { 1.0 } else { 0.0 };
    }
    ui.ctx().animate_bool_with_time(id, target, duration)
}

// ── Style installation ──────────────────────────────────────────────────────

/// Install the Aethel style. Call once per context (not per frame).
pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;

    v.dark_mode = true;
    v.panel_fill = BG;
    v.window_fill = BG_RAISE;
    v.extreme_bg_color = BG_HOVER;
    v.faint_bg_color = BG_RAISE;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_corner_radius = CornerRadius::same(12);
    // Buttons keep their frame — without it every control renders as bare text
    // and the UI looks (and feels) unclickable.
    v.button_frame = true;
    v.collapsing_header_frame = false;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT_2);
    v.hyperlink_color = ACCENT_2;
    v.override_text_color = None;

    v.widgets.noninteractive.bg_fill = BG_RAISE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(8);

    v.widgets.inactive.weak_bg_fill = BG_RAISE;
    v.widgets.inactive.bg_fill = BG_RAISE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.inactive.corner_radius = CornerRadius::same(8);

    v.widgets.hovered.weak_bg_fill = BG_HOVER;
    v.widgets.hovered.bg_fill = BG_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.hovered.corner_radius = CornerRadius::same(8);

    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.active.corner_radius = CornerRadius::same(8);

    v.widgets.open.weak_bg_fill = BG_HOVER;
    v.widgets.open.bg_fill = BG_HOVER;
    v.widgets.open.corner_radius = CornerRadius::same(8);

    style.spacing.item_spacing = Vec2::new(SPACE_2, SPACE_2);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.interact_size.y = 30.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;

    ctx.set_style(style);
}

pub fn apply_theme(ctx: &egui::Context) {
    // Kept for callers that still invoke it per frame; cheap, and now idempotent
    // because the style is derived from a fresh clone each time.
    install(ctx);
}

// ── Surfaces ────────────────────────────────────────────────────────────────

/// A raised card. Wraps its content exactly (no bleed into the parent rect).
pub fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(BG_RAISE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, add_contents)
        .inner
}

/// A card that lifts slightly on hover — used for clickable items (instances,
/// cosmetics, list rows).
pub fn interactive_card<R>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> (Response, R) {
    let id = egui::Id::new(("card", id_salt));

    let inner = egui::Frame::new()
        .fill(BG_RAISE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        });

    let response = inner.response;
    let lift = animate(ui, id, response.hovered(), MOTION_FAST);
    if lift > 0.001 {
        ui.painter().rect_stroke(
            response.rect.expand(1.0),
            CornerRadius::same(10),
            Stroke::new(1.0_f32 + lift, ACCENT.gamma_multiply(lift)),
            StrokeKind::Outside,
        );
    }

    (response, inner.inner)
}

/// A left-aligned section header with an optional dim subtitle.
pub fn section_header(ui: &mut egui::Ui, title: &str, subtitle: Option<&str>) {
    ui.label(RichText::new(title).size(20.0).strong().color(TEXT));
    if let Some(subtitle) = subtitle {
        ui.label(RichText::new(subtitle).size(12.0).color(TEXT_DIM));
    }
}

/// Big screen title used at the top of every screen.
pub fn screen_title(ui: &mut egui::Ui, title: &str, accent: Color32) -> egui::Response {
    ui.label(RichText::new(title).size(26.0).strong().color(accent))
}

pub fn dim(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).size(12.0).color(TEXT_DIM));
}

pub fn caption(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(
        RichText::new(text.into())
            .size(11.0)
            .color(TEXT_DIM.gamma_multiply(0.85)),
    );
}

// ── Buttons (§6 widget state machine) ───────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Primary action — accent fill.
    Primary,
    /// Neutral raised surface.
    Secondary,
    /// Borderless, low emphasis.
    Ghost,
    Danger,
    Success,
}

impl ButtonKind {
    fn idle_fill(self) -> Color32 {
        match self {
            ButtonKind::Primary => ACCENT.gamma_multiply(0.85),
            ButtonKind::Secondary | ButtonKind::Ghost => BG_RAISE,
            ButtonKind::Danger => DANGER.gamma_multiply(0.80),
            ButtonKind::Success => SUCCESS.gamma_multiply(0.80),
        }
    }

    fn hover_fill(self) -> Color32 {
        match self {
            ButtonKind::Primary => ACCENT,
            ButtonKind::Secondary => BG_HOVER,
            ButtonKind::Ghost => BG_RAISE,
            ButtonKind::Danger => DANGER,
            ButtonKind::Success => SUCCESS,
        }
    }

    fn press_fill(self) -> Color32 {
        match self {
            ButtonKind::Primary | ButtonKind::Ghost => ACCENT.gamma_multiply(0.75),
            ButtonKind::Secondary => BG_HOVER.gamma_multiply(0.85),
            ButtonKind::Danger => DANGER.gamma_multiply(0.7),
            ButtonKind::Success => SUCCESS.gamma_multiply(0.7),
        }
    }

    fn ink(self) -> Color32 {
        match self {
            // Saturated fills take near-black ink for contrast (08 §5.2).
            ButtonKind::Success | ButtonKind::Danger => INK,
            ButtonKind::Primary | ButtonKind::Secondary | ButtonKind::Ghost => TEXT,
        }
    }

    fn border(self) -> Color32 {
        match self {
            ButtonKind::Primary => ACCENT_2.gamma_multiply(0.5),
            ButtonKind::Ghost => Color32::TRANSPARENT,
            _ => BORDER,
        }
    }
}

/// The single button implementation every screen uses.
pub fn button(
    ui: &mut egui::Ui,
    label: impl Into<String>,
    kind: ButtonKind,
    enabled: bool,
) -> Response {
    let label = label.into();
    let text_width = ui.fonts(|f| {
        f.layout_no_wrap(label.clone(), FontId::proportional(14.0), TEXT)
            .size()
            .x
    });
    button_sized(
        ui,
        label,
        kind,
        Vec2::new((text_width + 32.0).max(88.0), 34.0),
        enabled,
    )
}

pub fn button_sized(
    ui: &mut egui::Ui,
    label: impl Into<String>,
    kind: ButtonKind,
    size: Vec2,
    enabled: bool,
) -> Response {
    let label = label.into();
    ui.scope(|ui| {
        let w = &mut ui.style_mut().visuals.widgets;
        w.inactive.weak_bg_fill = kind.idle_fill();
        w.hovered.weak_bg_fill = kind.hover_fill();
        w.active.weak_bg_fill = kind.press_fill();
        w.inactive.bg_stroke = Stroke::new(1.0_f32, kind.border());
        w.hovered.bg_stroke = Stroke::new(1.0_f32, kind.border().gamma_multiply(1.6));
        w.active.bg_stroke = Stroke::new(1.0_f32, kind.border());
        w.inactive.fg_stroke = Stroke::new(1.0_f32, kind.ink());
        w.hovered.fg_stroke = Stroke::new(1.0_f32, kind.ink());
        w.active.fg_stroke = Stroke::new(1.0_f32, kind.ink());
        w.inactive.corner_radius = CornerRadius::same(8);
        w.hovered.corner_radius = CornerRadius::same(8);
        w.active.corner_radius = CornerRadius::same(8);
        w.inactive.expansion = 0.0;
        w.hovered.expansion = 1.0;
        w.active.expansion = 0.0;

        ui.add_enabled(
            enabled,
            egui::Button::new(RichText::new(label).size(14.0))
                .corner_radius(CornerRadius::same(8))
                .min_size(size),
        )
    })
    .inner
}

/// A pill toggle (segmented filter chips, tab-ish controls).
pub fn pill(ui: &mut egui::Ui, label: &str, selected: bool) -> Response {
    let kind = if selected {
        ButtonKind::Primary
    } else {
        ButtonKind::Secondary
    };
    let width = ui.fonts(|f| {
        f.layout_no_wrap(label.to_string(), FontId::proportional(13.0), TEXT)
            .size()
            .x
    }) + 26.0;
    button_sized(ui, label, kind, Vec2::new(width.max(56.0), 28.0), true)
}

/// A small non-interactive status badge.
pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    let galley = ui.fonts(|f| f.layout_no_wrap(text.to_string(), FontId::proportional(11.0), INK));
    let size = galley.size() + Vec2::new(16.0, 6.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(9), color.gamma_multiply(0.9));
    ui.painter()
        .galley(rect.min + Vec2::new(8.0, 3.0), galley, color);
}

// ── The hero Play button ────────────────────────────────────────────────────

fn lerp_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

/// Rounded corners only on the outer edges of a strip run, so a strip-based
/// gradient still reads as one card.
fn edge_rounding(index: usize, count: usize, radius: u8) -> CornerRadius {
    let first = index == 0;
    let last = index + 1 == count;
    CornerRadius {
        nw: if first { radius } else { 0 },
        sw: if first { radius } else { 0 },
        ne: if last { radius } else { 0 },
        se: if last { radius } else { 0 },
    }
}

/// Paint a rounded rect with a horizontally swept two-colour gradient.
///
/// Built from strips rather than a gradient mesh so the result keeps rounded
/// corners; ~40 quads per frame is free at 60 fps.
fn paint_sweep(
    painter: &egui::Painter,
    rect: Rect,
    corner: u8,
    from: Color32,
    to: Color32,
    phase: f32,
    highlight: f32,
) {
    const STRIPS: usize = 40;
    let strip_w = rect.width() / STRIPS as f32;

    for i in 0..STRIPS {
        let t = i as f32 / (STRIPS - 1) as f32;
        let x0 = rect.left() + strip_w * i as f32;
        let x1 = if i + 1 == STRIPS {
            rect.right()
        } else {
            x0 + strip_w + 0.5
        };
        painter.rect_filled(
            Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom())),
            edge_rounding(i, STRIPS, corner),
            lerp_color(from, to, t),
        );
    }

    if highlight <= 0.001 {
        return;
    }

    // Sheen: a soft moving band swept across the button.
    let centre = phase * 1.6 - 0.3; // travels slightly past both edges
    for i in 0..STRIPS {
        let t = (i as f32 + 0.5) / STRIPS as f32;
        let distance = (t - centre).abs();
        if distance > 0.22 {
            continue;
        }
        let alpha = (1.0 - distance / 0.22).powi(2) * 0.28 * highlight;
        let x0 = rect.left() + strip_w * i as f32;
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(x0, rect.top()),
                egui::pos2(x0 + strip_w + 0.5, rect.bottom()),
            ),
            edge_rounding(i, STRIPS, corner),
            Color32::from_white_alpha((alpha * 255.0) as u8),
        );
    }
}

/// The hero Play/Stop button: gradient fill, hover glow, animated sheen.
///
/// Returns the response plus the rect so callers can overlay progress.
pub fn play_button(ui: &mut egui::Ui, label: &str, running: bool, enabled: bool) -> Response {
    let size = Vec2::new(280.0, 62.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);

    let prefs = motion_prefs();
    let time = ui.input(|i| i.time) as f32;
    let hover = animate(
        ui,
        response.id.with("hover"),
        response.hovered(),
        MOTION_FAST,
    );
    let glow = animate(ui, response.id.with("glow"), enabled, MOTION_SLOW);

    let (from, to) = if running {
        (DANGER, Color32::from_rgb(180, 45, 40))
    } else if enabled {
        (ACCENT, ACCENT_2.gamma_multiply(0.85))
    } else {
        (BG_RAISE, BG_HOVER)
    };

    // Outer glow ring.
    if enabled {
        let spread = 6.0 + hover * 8.0;
        let alpha = (0.10 + hover * 0.16) * glow;
        ui.painter().rect_filled(
            rect.expand(spread),
            CornerRadius::same(22),
            ACCENT.gamma_multiply(alpha),
        );
    }

    // Idle sheen sweep; paused under reduced motion.
    let phase = if prefs.reduced {
        0.5
    } else {
        (time * 0.28).fract()
    };
    let highlight = if enabled { 0.35 + hover * 0.65 } else { 0.0 };
    if prefs.reduced {
        // Static gradient, no animation.
        paint_sweep(ui.painter(), rect, 16, from, to, 1.0, 0.0);
    } else {
        paint_sweep(ui.painter(), rect, 16, from, to, phase, highlight);
    }

    if response.hovered() && enabled {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(16),
            Stroke::new(1.5_f32, Color32::from_white_alpha(70)),
            StrokeKind::Inside,
        );
    }

    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(20.0),
        if enabled { Color32::WHITE } else { TEXT_DIM },
    );

    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

// ── Small niceties ──────────────────────────────────────────────────────────

/// A dimmed label used for empty states.
pub fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str) {
    ui.add_space(SPACE_6);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).size(18.0).color(TEXT_DIM));
        ui.add_space(SPACE_1);
        ui.label(RichText::new(hint).size(12.0).color(BORDER));
    });
}

/// A labelled progress bar with the token palette.
pub fn progress_bar(ui: &mut egui::Ui, fraction: f32, text: impl Into<String>, width: f32) {
    ui.add(
        egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
            .text(text.into())
            .desired_width(width)
            .fill(ACCENT),
    );
}

/// Pulse value 0→1→0 over ~2 s (used for live/online indicators).
pub fn pulse(t: f32) -> f32 {
    ((t * std::f32::consts::TAU).cos() + 1.0) * 0.5
}

// ── Back-compat helpers ─────────────────────────────────────────────────────

/// Older screens call this; it now delegates to [`card`].
pub fn glass_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card(ui, add_contents)
}

/// Older screens call this; it now delegates to [`button_sized`].
pub fn glow_button(ui: &mut egui::Ui, text: &str, _color: Color32) -> Response {
    button(ui, text, ButtonKind::Primary, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_kinds_use_contrasting_ink() {
        // Status fills must take near-black ink, not white (08 §5.2).
        assert_eq!(ButtonKind::Success.ink(), INK);
        assert_eq!(ButtonKind::Danger.ink(), INK);
        assert_eq!(ButtonKind::Primary.ink(), TEXT);
    }

    #[test]
    fn hover_is_brighter_than_idle() {
        for kind in [
            ButtonKind::Primary,
            ButtonKind::Secondary,
            ButtonKind::Danger,
            ButtonKind::Success,
        ] {
            let idle = kind.idle_fill();
            let hover = kind.hover_fill();
            let idle_luma = idle.r() as u32 + idle.g() as u32 + idle.b() as u32;
            let hover_luma = hover.r() as u32 + hover.g() as u32 + hover.b() as u32;
            assert!(
                hover_luma >= idle_luma,
                "{kind:?} hover is darker than idle"
            );
        }
    }

    #[test]
    fn reduced_motion_collapses_durations() {
        let reduced = MotionPrefs { reduced: true };
        assert_eq!(reduced.duration(MOTION_SLOW), 0.0);
        let normal = MotionPrefs { reduced: false };
        assert_eq!(normal.duration(MOTION_SLOW), MOTION_SLOW);
    }

    #[test]
    fn pulse_stays_in_unit_range() {
        for i in 0..100 {
            let v = pulse(i as f32 / 20.0);
            assert!((0.0..=1.0).contains(&v), "pulse out of range: {v}");
        }
    }
}
