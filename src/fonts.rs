use eframe::egui;

/// Prepend Segoe UI + CJK + Hangul to egui's font fallback chain so we
/// don't render tofu for fullwidth / CJK / Korean glyphs (and missing
/// symbol glyphs like `●` aren't covered by the default Ubuntu-Light).
pub fn install_broad_unicode_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates = [
        ("segoe_ui", r"C:\Windows\Fonts\segoeui.ttf"),
        ("ms_yahei", r"C:\Windows\Fonts\msyh.ttc"),
        ("malgun", r"C:\Windows\Fonts\malgun.ttf"),
    ];
    for (name, path) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(name.into(), egui::FontData::from_owned(bytes));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().insert(0, name.into());
            }
        }
    }
    ctx.set_fonts(fonts);
}
