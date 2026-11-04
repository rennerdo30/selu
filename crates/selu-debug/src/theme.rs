//! Selu Studio's look: a dark theme with one violet accent, roomier
//! spacing, rounded controls, and the system's UI and code fonts when
//! present (Segoe UI and Cascadia Mono on Windows; egui's bundled fonts
//! otherwise, so nothing is downloaded or shipped).

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle, Visuals,
};
use std::sync::Arc;

/// Colours used across the app.
pub mod palette {
    use eframe::egui::Color32;
    // Text colours meet WCAG AA (4.5:1) on every background below; plain
    // text is around 14:1.
    pub const BG: Color32 = Color32::from_rgb(0x15, 0x16, 0x1a);
    pub const PANEL: Color32 = Color32::from_rgb(0x1e, 0x1f, 0x24);
    pub const SIDEBAR: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x20);
    pub const RAISED: Color32 = Color32::from_rgb(0x26, 0x28, 0x2e);
    pub const HOVER: Color32 = Color32::from_rgb(0x2c, 0x2e, 0x36);
    pub const BORDER: Color32 = Color32::from_rgb(0x2a, 0x2c, 0x33);
    pub const TEXT: Color32 = Color32::from_rgb(0xec, 0xee, 0xf3);
    pub const MUTED: Color32 = Color32::from_rgb(0xb4, 0xb9, 0xc4);
    pub const FAINT: Color32 = Color32::from_rgb(0x8f, 0x95, 0xa1);
    pub const ACCENT: Color32 = Color32::from_rgb(0x8b, 0x6c, 0xff);
    pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x3a, 0x31, 0x6b);
    pub const ERROR: Color32 = Color32::from_rgb(0xf2, 0x6d, 0x6d);
    pub const WARNING: Color32 = Color32::from_rgb(0xe8, 0xb3, 0x4b);
    pub const OK: Color32 = Color32::from_rgb(0x6f, 0xcf, 0x8a);
    pub const CURRENT_LINE: Color32 = Color32::from_rgb(0x22, 0x24, 0x2b);
}

/// The icon font family name.
pub const ICONS: &str = "icons";

/// Icon code points in Segoe Fluent Icons / MDL2 Assets.
pub mod icon {
    pub const PLAY: &str = "\u{E768}";
    pub const PAUSE: &str = "\u{E769}";
    pub const REFRESH: &str = "\u{E72C}";
    pub const BUILD: &str = "\u{E90F}";
    pub const FILES: &str = "\u{E8A5}";
    pub const DOCUMENT: &str = "\u{E8A5}";
    pub const SETTINGS: &str = "\u{E713}";
    pub const FRAMES: &str = "\u{F0E2}";
    pub const PREVIEW: &str = "\u{E7F4}";
    pub const CAMERA: &str = "\u{E722}";
    pub const ERROR: &str = "\u{EA39}";
    pub const WARNING: &str = "\u{E7BA}";
    pub const CODE: &str = "\u{E943}";
    pub const CLOSE: &str = "\u{E711}";
    pub const CHEVRON_RIGHT: &str = "\u{E76C}";
    pub const CHEVRON_DOWN: &str = "\u{E70D}";
    pub const UP: &str = "\u{E70E}";
    pub const DOWN: &str = "\u{E70D}";
    pub const NEW_FILE: &str = "\u{E8A5}";
    pub const NEW_FOLDER: &str = "\u{E8F4}";
    pub const FOLDER: &str = "\u{E8B7}";
    pub const GAME: &str = "\u{E7FC}";
    pub const WIN_MINIMIZE: &str = "\u{E921}";
    pub const WIN_MAXIMIZE: &str = "\u{E922}";
    pub const WIN_RESTORE: &str = "\u{E923}";
    pub const WIN_CLOSE: &str = "\u{E8BB}";
}

/// A font id for icons at `size`.
pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS.into()))
}

/// Icon text for buttons and labels.
pub fn icon_text(code: &str, size: f32) -> egui::RichText {
    egui::RichText::new(code).font(icon_font(size))
}

fn load(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let ui = load(&[
        &format!("{windir}\\Fonts\\segoeui.ttf"),
        "/System/Library/Fonts/SFNS.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]);
    let code = load(&[
        &format!("{windir}\\Fonts\\CascadiaMono.ttf"),
        &format!("{windir}\\Fonts\\consola.ttf"),
        "/System/Library/Fonts/Menlo.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ]);
    if let Some(data) = ui {
        fonts
            .font_data
            .insert("studio-ui".into(), Arc::new(FontData::from_owned(data)));
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "studio-ui".into());
    }
    if let Some(data) = code {
        fonts
            .font_data
            .insert("studio-code".into(), Arc::new(FontData::from_owned(data)));
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "studio-code".into());
    }
    // Symbols (◆ ⊗ ⚠ ...) fall back to Segoe UI Symbol.
    if let Some(data) = load(&[&format!("{windir}\\Fonts\\seguisym.ttf")]) {
        fonts.font_data.insert(
            "studio-symbols".into(),
            Arc::new(FontData::from_owned(data)),
        );
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("studio-symbols".into());
        }
    }
    // App icons: Segoe Fluent Icons (Windows 11) or MDL2 Assets (Windows
    // 10). The family always exists; without the font it falls back to the
    // UI font and icons show as boxes.
    let mut icon_fonts = Vec::new();
    if let Some(data) = load(&[
        &format!("{windir}\\Fonts\\SegoeIcons.ttf"),
        &format!("{windir}\\Fonts\\segmdl2.ttf"),
    ]) {
        fonts
            .font_data
            .insert("studio-icons".into(), Arc::new(FontData::from_owned(data)));
        icon_fonts.push("studio-icons".to_string());
    }
    icon_fonts.extend(
        fonts
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default(),
    );
    fonts
        .families
        .insert(FontFamily::Name(ICONS.into()), icon_fonts);
    ctx.set_fonts(fonts);

    use palette::*;
    let mut v = Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = RAISED;
    v.extreme_bg_color = BG;
    v.faint_bg_color = Color32::from_rgb(0x21, 0x23, 0x29);
    v.code_bg_color = BG;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.window_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(100),
    };
    v.selection.bg_fill = ACCENT_SOFT;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.error_fg_color = ERROR;
    v.warn_fg_color = WARNING;
    v.override_text_color = None;
    v.collapsing_header_frame = false;
    v.indent_has_left_vline = false;
    v.striped = false;
    let w = &mut v.widgets;
    for (state, fill, text) in [
        (&mut w.noninteractive, PANEL, TEXT),
        (&mut w.inactive, RAISED, TEXT),
        (&mut w.hovered, HOVER, Color32::WHITE),
        (&mut w.active, ACCENT_SOFT, Color32::WHITE),
        (&mut w.open, HOVER, Color32::WHITE),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.corner_radius = CornerRadius::same(5);
        state.fg_stroke = Stroke::new(1.0, text);
        state.expansion = 0.0;
    }
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    // Checkbox and radio fills must stand out from windows and panels.
    w.inactive.bg_fill = Color32::from_rgb(0x36, 0x39, 0x43);
    w.inactive.bg_stroke = Stroke::NONE;
    w.hovered.bg_stroke = Stroke::NONE;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    ctx.set_visuals(v);

    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 5.0);
        s.spacing.button_padding = egui::vec2(9.0, 4.0);
        s.spacing.menu_margin = Margin::same(6);
        s.spacing.window_margin = Margin::same(12);
        s.spacing.interact_size.y = 24.0;
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, FontId::proportional(13.5)),
            (TextStyle::Heading, FontId::proportional(17.0)),
            (TextStyle::Monospace, FontId::monospace(13.5)),
        ]
        .into();
    });
}

/// The Selu logo (docs/logo/selu.svg rendered at 256 px).
pub const LOGO_PNG: &[u8] = include_bytes!("../assets/selu-256.png");

/// The logo as an egui image, for the title bar.
pub fn logo_image() -> Option<egui::ColorImage> {
    let icon = eframe::icon_data::from_png_bytes(LOGO_PNG).ok()?;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [icon.width as usize, icon.height as usize],
        &icon.rgba,
    ))
}

/// A thin horizontal rule in the border colour.
pub fn rule(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, palette::BORDER);
}

/// An uppercase section caption, as in the side bar.
pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(11.0)
            .color(palette::MUTED)
            .strong(),
    );
}
