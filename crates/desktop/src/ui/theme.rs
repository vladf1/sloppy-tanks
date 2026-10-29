//! The battle interface's look in egui (`src/style.css`): dark steel-glass plates with
//! a steel hairline, gold for what is selected, blue and red for the teams, Impact for
//! display type and the platform's UI face for the rest, like the page's system-ui.

use egui::{Color32, FontData, FontDefinitions, FontFamily, Stroke};
use sloppy_core::sim::Team;
use sloppy_render::system_fonts::{FontStyle, system_font};

pub const INK_0: Color32 = Color32::from_rgb(0x05, 0x0a, 0x14);
pub const INK_1: Color32 = Color32::from_rgb(0x0a, 0x15, 0x27);
pub const INK_2: Color32 = Color32::from_rgb(0x10, 0x23, 0x40);
pub const PLATE: Color32 = Color32::from_rgba_unmultiplied_const(0x10, 0x22, 0x40, 0xf2);
pub const PLATE_EDGE: Color32 = Color32::from_rgba_unmultiplied_const(0xa6, 0xd3, 0xff, 0x4d);
pub const GOLD: Color32 = Color32::from_rgb(0xff, 0xcf, 0x38);
pub const GOLD_HOT: Color32 = Color32::from_rgb(0xff, 0xe5, 0x8a);
pub const GOLD_DEEP: Color32 = Color32::from_rgb(0xff, 0x9d, 0x1c);
pub const BLUE: Color32 = Color32::from_rgb(0x3e, 0xa6, 0xff);
pub const BLUE_DEEP: Color32 = Color32::from_rgb(0x16, 0x63, 0xc7);
pub const RED: Color32 = Color32::from_rgb(0xff, 0x4d, 0x5e);
pub const TEXT: Color32 = Color32::from_rgb(0xf4, 0xf9, 0xff);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0xa9, 0xc0, 0xd8);
pub const TEXT_LABEL: Color32 = Color32::from_rgb(0x8e, 0xd4, 0xff);
/// The window behind the menus between rounds.
pub const BACKDROP: Color32 = INK_1;

/// The family of display type (Impact where the system has it).
pub fn display() -> FontFamily {
    FontFamily::Name("display".into())
}

/// Display type at `size` points.
pub fn display_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, display())
}

/// Bold UI type at `size` points.
pub fn bold_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name("bold".into()))
}

/// The colour of `team`.
pub fn team_color(team: Team) -> Color32 {
    match team {
        Team::Blue => BLUE,
        Team::Red => RED,
    }
}

/// Fonts and visuals for the whole interface.
pub fn install(context: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, family: &str, style: FontStyle| {
        if let Some((bytes, _)) = system_font(family, style) {
            let mut data = FontData::from_owned(bytes);
            for axis in data.variation_axes() {
                if axis.tag == egui::epaint::text::Tag::new(b"wght") {
                    data.tweak.coords.push(axis.tag, f32::from(style.weight));
                }
            }
            fonts.font_data.insert(name.into(), data.into());
            true
        } else {
            false
        }
    };
    let regular = FontStyle {
        weight: 400,
        italic: false,
    };
    let bold = FontStyle {
        weight: 700,
        italic: false,
    };
    let has_ui = add("ui", "system-ui", regular) || add("ui", "sans-serif", regular);
    let has_bold = add("ui-bold", "system-ui", bold) || add("ui-bold", "sans-serif", bold);
    let has_mono = add("ui-mono", "monospace", regular);
    let has_display = add("impact", "Impact", regular);
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    if has_ui {
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .expect("egui has a proportional family")
            .insert(0, "ui".into());
    }
    if has_mono {
        fonts
            .families
            .get_mut(&FontFamily::Monospace)
            .expect("egui has a monospace family")
            .insert(0, "ui-mono".into());
    }
    let with = |first: Option<&str>| {
        let mut family: Vec<String> = first.map(str::to_owned).into_iter().collect();
        family.extend(fallback.iter().cloned());
        family
    };
    fonts.families.insert(
        FontFamily::Name("bold".into()),
        with(has_bold.then_some("ui-bold")),
    );
    fonts.families.insert(
        display(),
        with(if has_display {
            Some("impact")
        } else {
            has_bold.then_some("ui-bold")
        }),
    );
    context.set_fonts(fonts);

    let mut style = (*context.global_style()).clone();
    let visuals = &mut style.visuals;
    *visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = PLATE;
    visuals.window_fill = PLATE;
    visuals.window_stroke = Stroke::new(1.0, PLATE_EDGE);
    visuals.window_corner_radius = 4.into();
    visuals.selection.bg_fill = GOLD_DEEP;
    visuals.selection.stroke = Stroke::new(1.0, INK_0);
    visuals.widgets.inactive.weak_bg_fill = INK_2;
    visuals.widgets.inactive.bg_fill = INK_2;
    visuals.widgets.hovered.weak_bg_fill = BLUE_DEEP;
    visuals.widgets.active.weak_bg_fill = GOLD_DEEP;
    visuals.hyperlink_color = TEXT_LABEL;
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    context.set_global_style(style);
}
