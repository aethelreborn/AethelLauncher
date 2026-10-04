use eframe::egui;
use egui::{Color32, CornerRadius, FontId, Response, RichText, Sense, Stroke, StrokeKind, Vec2};

pub const BG: Color32 = Color32::from_rgb(14, 14, 16);
pub const BG_RAISE: Color32 = Color32::from_rgb(24, 24, 27);
pub const BG_HOVER: Color32 = Color32::from_rgb(34, 34, 39);
pub const BORDER: Color32 = Color32::from_rgb(47, 47, 54);

pub const ACCENT: Color32 = Color32::from_rgb(239, 68, 68);
pub const ACCENT_2: Color32 = Color32::from_rgb(255, 122, 122);
pub const SUCCESS: Color32 = Color32::from_rgb(46, 204, 113);
pub const WARN: Color32 = Color32::from_rgb(243, 156, 18);
pub const DANGER: Color32 = Color32::from_rgb(231, 76, 60);

pub const TEXT: Color32 = Color32::from_rgb(236, 239, 244);
pub const TEXT_DIM: Color32 = Color32::from_rgb(138, 143, 163);

pub const INK: Color32 = Color32::from_rgb(11, 11, 15);

pub const BG_DARK: Color32 = BG;
pub const SURFACE: Color32 = BG_RAISE;
pub const SURFACE_LIGHT: Color32 = BG_HOVER;
pub const ACCENT_VIOLET: Color32 = ACCENT;
pub const ACCENT_CYAN: Color32 = TEXT;
pub const TEXT_PRIMARY: Color32 = TEXT;
pub const TEXT_SECONDARY: Color32 = TEXT_DIM;
pub const SUCCESS_GREEN: Color32 = SUCCESS;
pub const ORANGE: Color32 = WARN;
pub const DANGER_RED: Color32 = DANGER;

pub const SPACE_1: f32 = 4.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;
pub const SPACE_5: f32 = 24.0;
pub const SPACE_6: f32 = 32.0;

pub const MOTION_FAST: f32 = 0.12;
pub const MOTION_BASE: f32 = 0.18;
pub const MOTION_SLOW: f32 = 0.34;

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

pub fn animate(ui: &egui::Ui, id: egui::Id, target: bool, seconds: f32) -> f32 {
    let duration = motion_prefs().duration(seconds);
    if duration <= 0.0 {
        return if target { 1.0 } else { 0.0 };
    }
    ui.ctx().animate_bool_with_time(id, target, duration)
}

pub fn install(ctx: &egui::Context) {
    crate::fonts::install(ctx);

    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;

    v.dark_mode = true;
    v.panel_fill = BG;
    v.window_fill = BG_RAISE;
    v.extreme_bg_color = BG_HOVER;
    v.faint_bg_color = BG_RAISE;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_corner_radius = CornerRadius::same(8);
    v.button_frame = true;
    v.collapsing_header_frame = false;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0_f32, TEXT);
    v.hyperlink_color = ACCENT_2;
    v.override_text_color = None;

    v.widgets.noninteractive.bg_fill = BG_RAISE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(6);

    v.widgets.inactive.weak_bg_fill = BG_RAISE;
    v.widgets.inactive.bg_fill = BG_HOVER;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.inactive.corner_radius = CornerRadius::same(6);

    v.widgets.hovered.weak_bg_fill = BG_HOVER;
    v.widgets.hovered.bg_fill = BG_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.hovered.corner_radius = CornerRadius::same(6);

    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.active.corner_radius = CornerRadius::same(6);

    v.widgets.open.weak_bg_fill = BG_HOVER;
    v.widgets.open.bg_fill = BG_HOVER;
    v.widgets.open.corner_radius = CornerRadius::same(6);

    style.spacing.item_spacing = Vec2::new(SPACE_2, SPACE_2);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.interact_size.y = 30.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;

    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(24.0, crate::fonts::strong_family()),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.0, crate::fonts::strong_family()),
    );

    ctx.set_style(style);
}

pub fn apply_theme(ctx: &egui::Context) {
    install(ctx);
}

pub fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(BG_RAISE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, add_contents)
        .inner
}

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

pub fn section_header(ui: &mut egui::Ui, title: &str, subtitle: Option<&str>) {
    ui.label(
        RichText::new(title)
            .size(20.0)
            .strong()
            .family(crate::fonts::strong_family())
            .color(TEXT),
    );
    if let Some(subtitle) = subtitle {
        ui.label(RichText::new(subtitle).size(12.0).color(TEXT_DIM));
    }
}

pub fn screen_title(ui: &mut egui::Ui, title: &str, accent: Color32) -> egui::Response {
    ui.label(
        RichText::new(title)
            .size(26.0)
            .strong()
            .family(crate::fonts::strong_family())
            .color(accent),
    )
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
    Danger,
    Success,
}

impl ButtonKind {
    fn idle_fill(self) -> Color32 {
        match self {
            ButtonKind::Primary => ACCENT,
            ButtonKind::Secondary | ButtonKind::Ghost => BG_RAISE,
            ButtonKind::Danger => DANGER,
            ButtonKind::Success => SUCCESS,
        }
    }

    fn hover_fill(self) -> Color32 {
        match self {
            ButtonKind::Primary => ACCENT.gamma_multiply(1.15),
            ButtonKind::Secondary => BG_HOVER,
            ButtonKind::Ghost => BG_RAISE,
            ButtonKind::Danger => DANGER.gamma_multiply(1.15),
            ButtonKind::Success => SUCCESS.gamma_multiply(1.12),
        }
    }

    fn press_fill(self) -> Color32 {
        match self {
            ButtonKind::Primary => ACCENT.gamma_multiply(0.82),
            ButtonKind::Ghost => BG_HOVER,
            ButtonKind::Secondary => BG_HOVER.gamma_multiply(0.85),
            ButtonKind::Danger => DANGER.gamma_multiply(0.8),
            ButtonKind::Success => SUCCESS.gamma_multiply(0.8),
        }
    }

    fn ink(self) -> Color32 {
        match self {
            ButtonKind::Success => INK,
            ButtonKind::Primary | ButtonKind::Danger => TEXT,
            ButtonKind::Secondary | ButtonKind::Ghost => TEXT,
        }
    }

    fn border(self) -> Color32 {
        match self {
            ButtonKind::Primary => ACCENT.gamma_multiply(1.15),
            ButtonKind::Ghost => Color32::TRANSPARENT,
            _ => BORDER,
        }
    }
}

pub fn button(
    ui: &mut egui::Ui,
    label: impl Into<String>,
    kind: ButtonKind,
    enabled: bool,
) -> Response {
    let label = label.into();
    let text_width = ui.fonts(|f| {
        f.layout_no_wrap(label.clone(), crate::fonts::strong_id(14.0), TEXT)
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
    label: impl Into<egui::WidgetText>,
    kind: ButtonKind,
    size: Vec2,
    enabled: bool,
) -> Response {
    let label = match label.into() {
        egui::WidgetText::RichText(text) => {
            egui::WidgetText::RichText(text.size(14.0).family(crate::fonts::strong_family()))
        }
        other => other,
    };
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
            egui::Button::new(label)
                .corner_radius(CornerRadius::same(8))
                .min_size(size),
        )
    })
    .inner
}

pub fn icon_label(icon: &str, text: &str, icon_size: f32, text_size: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        icon,
        0.0,
        egui::text::TextFormat {
            font_id: crate::fonts::icon_id(icon_size),
            color: Color32::PLACEHOLDER,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    if !text.is_empty() {
        job.append(
            text,
            icon_size * 0.35,
            egui::text::TextFormat {
                font_id: crate::fonts::strong_id(text_size),
                color: Color32::PLACEHOLDER,
                valign: egui::Align::Center,
                ..Default::default()
            },
        );
    }
    job
}

pub fn icon_button(
    ui: &mut egui::Ui,
    icon: &str,
    text: &str,
    kind: ButtonKind,
    enabled: bool,
) -> Response {
    let job = icon_label(icon, text, 15.0, 14.0);
    let width = ui.fonts(|f| f.layout_job(job.clone()).size().x) + 32.0;
    button_sized(ui, job, kind, Vec2::new(width.max(72.0), 34.0), enabled)
}

pub fn icon_only_button(
    ui: &mut egui::Ui,
    icon: &str,
    tip: &str,
    kind: ButtonKind,
    size: Vec2,
    enabled: bool,
) -> Response {
    let job = icon_label(icon, "", size.y * 0.58, 14.0);
    button_sized(ui, job, kind, size, enabled).on_hover_text(tip)
}

pub fn pill(ui: &mut egui::Ui, label: &str, selected: bool) -> Response {
    let kind = if selected {
        ButtonKind::Primary
    } else {
        ButtonKind::Secondary
    };
    let width = ui.fonts(|f| {
        f.layout_no_wrap(label.to_string(), crate::fonts::strong_id(13.0), TEXT)
            .size()
            .x
    }) + 26.0;
    button_sized(ui, label, kind, Vec2::new(width.max(56.0), 28.0), true)
}

pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    let galley = ui.fonts(|f| f.layout_no_wrap(text.to_string(), FontId::proportional(11.0), INK));
    let size = galley.size() + Vec2::new(16.0, 6.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(9), color.gamma_multiply(0.9));
    ui.painter()
        .galley(rect.min + Vec2::new(8.0, 3.0), galley, color);
}

pub fn play_button(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    running: bool,
    enabled: bool,
) -> Response {
    let size = Vec2::new(280.0, 62.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);

    let hover = animate(
        ui,
        response.id.with("hover"),
        response.hovered() && enabled,
        MOTION_FAST,
    );

    let (fill, border, ink) = if running {
        (DANGER, DANGER.gamma_multiply(1.15), Color32::WHITE)
    } else if enabled {
        (
            lerp_color(ACCENT, ACCENT.gamma_multiply(1.15), hover),
            ACCENT.gamma_multiply(1.15),
            Color32::WHITE,
        )
    } else {
        (BG_RAISE, BORDER, TEXT_DIM)
    };

    ui.painter().rect_filled(rect, CornerRadius::same(10), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0_f32, border),
        StrokeKind::Inside,
    );

    let galley = label.into().into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        crate::fonts::strong_id(18.0),
    );
    ui.painter()
        .galley(rect.center() - galley.size() * 0.5, galley, ink);

    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

fn lerp_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

pub fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str) {
    ui.add_space(SPACE_6);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).size(18.0).color(TEXT_DIM));
        ui.add_space(SPACE_1);
        ui.label(RichText::new(hint).size(12.0).color(BORDER));
    });
}

pub fn progress_bar(ui: &mut egui::Ui, fraction: f32, text: impl Into<String>, width: f32) {
    ui.add(
        egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
            .text(text.into())
            .desired_width(width)
            .fill(ACCENT),
    );
}

pub fn pulse(t: f32) -> f32 {
    ((t * std::f32::consts::TAU).cos() + 1.0) * 0.5
}

pub fn glass_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card(ui, add_contents)
}

pub fn glow_button(ui: &mut egui::Ui, text: &str, _color: Color32) -> Response {
    button(ui, text, ButtonKind::Primary, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_kinds_use_contrasting_ink() {
        assert_eq!(ButtonKind::Success.ink(), INK);
        assert_eq!(ButtonKind::Danger.ink(), TEXT);
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
