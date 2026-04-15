//! Shared loader for the window icon used by every egui window.

use eframe::egui;

const ICON_PNG: &[u8] = include_bytes!("../assets/app_icon.png");

pub fn icon_data() -> Option<egui::IconData> {
    let img = image::load_from_memory(ICON_PNG).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    })
}
