//! Branding — the embedded Aethel logo.
//!
//! The logo ships inside the binary (`include_bytes!`) so the launcher never
//! shows a placeholder: splash, title bar, hero panel and window icon all use
//! the real artwork, and cosmetic cards fall back to it rather than a broken
//! image.

use eframe::egui;
use egui::{ColorImage, TextureHandle, TextureOptions};
use std::sync::Arc;

/// The project logo, copied from the repository root at build time.
pub const LOGO_BYTES: &[u8] = include_bytes!("../assets/aethel-logo.jpg");

/// Decoded RGBA8 logo: `(width, height, pixels)`.
fn decode_logo() -> Option<(usize, usize, Vec<u8>)> {
    use image::GenericImageView;
    let image = image::load_from_memory(LOGO_BYTES).ok()?;
    let (width, height) = image.dimensions();
    let rgba = image.to_rgba8();
    Some((width as usize, height as usize, rgba.into_raw()))
}

/// Window / taskbar icon.
pub fn window_icon() -> Arc<egui::IconData> {
    match decode_logo() {
        Some((width, height, rgba)) => Arc::new(egui::IconData {
            rgba,
            width: width as u32,
            height: height as u32,
        }),
        // A 1×1 transparent icon is better than refusing to start.
        None => Arc::new(egui::IconData {
            rgba: vec![0, 0, 0, 0],
            width: 1,
            height: 1,
        }),
    }
}

/// Load (once) and return the logo texture.
///
/// egui caches textures by name, so calling this every frame is cheap — but we
/// memoise the `TextureHandle` anyway to avoid the lookup.
pub fn logo_texture(ctx: &egui::Context) -> Option<TextureHandle> {
    let id = egui::Id::new("aethel-logo-texture");
    if let Some(existing) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(existing);
    }

    let (width, height, rgba) = decode_logo()?;
    let image = ColorImage::from_rgba_unmultiplied([width, height], &rgba);
    let handle = ctx.load_texture("aethel-logo", image, TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, handle.clone()));
    Some(handle)
}

/// Draw the logo at a given size, preserving its aspect ratio.
///
/// Returns `None` when the texture could not be decoded, so callers can fall
/// back to a text wordmark instead of leaving a hole in the layout.
pub fn logo(ui: &mut egui::Ui, size: f32) -> Option<egui::Response> {
    let texture = logo_texture(ui.ctx())?;
    let image = egui::Image::new(&texture)
        .fit_to_exact_size(egui::vec2(size, size))
        .corner_radius(egui::CornerRadius::same((size * 0.18) as u8));
    Some(ui.add(image))
}

/// A dimmed version used as a thumbnail placeholder (cosmetics, news covers).
pub fn logo_placeholder(ui: &mut egui::Ui, size: egui::Vec2, tint: egui::Color32) -> bool {
    let Some(texture) = logo_texture(ui.ctx()) else {
        return false;
    };

    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(8), tint.gamma_multiply(0.14));
    ui.painter().image(
        texture.id(),
        rect.shrink(size.y * 0.18),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::from_white_alpha(90),
    );
    true
}

/// The brand lockup: logo + wordmark. Used in the title bar and splash.
pub fn wordmark(ui: &mut egui::Ui, logo_size: f32, text_size: f32) {
    ui.horizontal(|ui| {
        let _ = logo(ui, logo_size);
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("AETHEL")
                    .size(text_size)
                    .strong()
                    .color(egui::Color32::from_rgb(236, 239, 244)),
            );
            ui.label(
                egui::RichText::new("LAUNCHER")
                    .size(text_size * 0.5)
                    .color(egui::Color32::from_rgb(0, 210, 255)),
            );
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_logo_decodes() {
        let (width, height, rgba) = decode_logo().expect("logo must decode");
        assert!(width > 0 && height > 0, "logo has no dimensions");
        assert_eq!(width * height * 4, rgba.len(), "rgba buffer size mismatch");
    }

    #[test]
    fn window_icon_has_valid_dimensions() {
        let icon = window_icon();
        assert!(icon.width > 0 && icon.height > 0);
        assert_eq!(
            (icon.width * icon.height * 4) as usize,
            icon.rgba.len(),
            "icon buffer must be RGBA"
        );
    }
}
