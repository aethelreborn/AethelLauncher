use eframe::egui;
use std::sync::Arc;

pub const ICON_HOME: &str = "\u{e88a}";
pub const ICON_LIBRARY: &str = "\u{e04a}";
pub const ICON_STORE: &str = "\u{ea12}";
pub const ICON_MODS: &str = "\u{e87b}";
pub const ICON_DOWNLOADS: &str = "\u{e2c4}";
pub const ICON_SETTINGS: &str = "\u{e8b8}";
pub const ICON_ACCOUNT: &str = "\u{e7fd}";
pub const ICON_NEWSPAPER: &str = "\u{eb81}";
pub const ICON_CRASH: &str = "\u{ebf1}";

pub const ICON_PLAY: &str = "\u{e037}";
pub const ICON_STOP: &str = "\u{e047}";
pub const ICON_ADD: &str = "\u{e145}";
pub const ICON_CLOSE: &str = "\u{e5cd}";
pub const ICON_REFRESH: &str = "\u{e5d5}";
pub const ICON_EDIT: &str = "\u{e3c9}";
pub const ICON_DELETE: &str = "\u{e872}";
pub const ICON_UPLOAD: &str = "\u{e2c6}";
pub const ICON_FOLDER: &str = "\u{e2c7}";
pub const ICON_FOLDER_OPEN: &str = "\u{e2c8}";
pub const ICON_CREATE: &str = "\u{e150}";
pub const ICON_DROPDOWN: &str = "\u{e5c5}";
pub const ICON_COPY: &str = "\u{e14d}";
pub const ICON_OPEN_IN_NEW: &str = "\u{e89e}";
pub const ICON_MORE: &str = "\u{e5d4}";
pub const ICON_LAUNCH: &str = "\u{e895}";
pub const ICON_NOTE_ADD: &str = "\u{e89c}";
pub const ICON_BUILD: &str = "\u{e869}";
pub const ICON_CHECK: &str = "\u{e5ca}";
pub const ICON_SEARCH: &str = "\u{e8b6}";
pub const ICON_DOWNLOAD: &str = "\u{f090}";
pub const ICON_GLOBE: &str = "\u{e80b}";
pub const ICON_EXPLORE: &str = "\u{e87a}";

pub const ICON_PALETTE: &str = "\u{e40a}";
pub const ICON_AIRPLANE: &str = "\u{e195}";
pub const ICON_FLIGHT: &str = "\u{e539}";
pub const ICON_DASHBOARD: &str = "\u{e871}";
pub const ICON_FACE: &str = "\u{e87c}";
pub const ICON_DIAMOND: &str = "\u{ead5}";

pub fn strong_family() -> egui::FontFamily {
    egui::FontFamily::Name("inter-strong".into())
}

pub fn icons_family() -> egui::FontFamily {
    egui::FontFamily::Name("icons".into())
}

pub fn icon_id(size: f32) -> egui::FontId {
    egui::FontId::new(size, icons_family())
}

pub fn strong_id(size: f32) -> egui::FontId {
    egui::FontId::new(size, strong_family())
}

pub fn install(ctx: &egui::Context) {
    let key = egui::Id::new("aethel-fonts-installed");
    if ctx.data(|d| d.get_temp::<bool>(key)).unwrap_or(false) {
        return;
    }

    let mut defs = egui::FontDefinitions::default();
    defs.font_data.insert(
        "inter-regular".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    defs.font_data.insert(
        "inter-semibold".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/Inter-SemiBold.ttf"
        ))),
    );
    defs.font_data.insert(
        "material-icons".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/MaterialIcons-Regular.ttf"
        ))),
    );

    let mut fallbacks = Vec::new();
    if let Some(prop) = defs.families.get(&egui::FontFamily::Proportional) {
        prop.clone_into(&mut fallbacks);
        if let Some(prop) = defs.families.get_mut(&egui::FontFamily::Proportional) {
            prop.insert(0, "inter-regular".into());
        }
    }

    let mut strong = vec!["inter-semibold".to_owned()];
    strong.extend(fallbacks.iter().cloned());
    defs.families.insert(strong_family(), strong);

    let mut icons = vec!["material-icons".to_owned(), "inter-regular".to_owned()];
    icons.extend(fallbacks.iter().cloned());
    defs.families.insert(icons_family(), icons);

    ctx.set_fonts(defs);
    ctx.data_mut(|d| d.insert_temp(key, true));
}
