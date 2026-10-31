//! The battle interface's look in egui (`src/style.css`): dark steel-glass plates with
//! a steel hairline, gold for what is selected, blue and red for the teams, Impact for
//! display type and the platform's UI face for the rest, like the page's system-ui. A
//! page cannot read the system's fonts, so there the build embeds its own
//! (`public/fonts/`): Anton (OFL), widened to Impact's width by `scripts/widen-anton.py`,
//! for Impact and Roboto (Apache 2.0) for the UI face.

use std::borrow::Cow;

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontTweak, Stroke};
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

/// Anton drawn in Impact's place, so text the layout measured in Impact fits: its
/// capitals scaled to Impact's as the native app draws them (measured on Battle Setup).
const ANTON_SCALE: f32 = 0.975;
/// Impact's line box (`hhea`, in em), which Anton takes at that scale: egui places text
/// by its line box, and Anton's own (1.176 em up, 0.329 down) put centred and
/// bottom-aligned text a quarter em off.
const IMPACT_ASCENT: f32 = 2066.0 / 2048.0;
const IMPACT_DESCENT: f32 = 432.0 / 2048.0;

/// The game's own face for `family` in `style`, where it has one (a page's, which
/// `index.html` fetches before the shell starts), with how to draw it in that family's
/// place.
fn embedded_font(family: &str, style: FontStyle) -> Option<(Vec<u8>, FontTweak)> {
    let (file, tweak) = match (family, style.weight >= 600) {
        ("system-ui" | "sans-serif", false) => ("fonts/Roboto-Regular.ttf", FontTweak::default()),
        ("system-ui" | "sans-serif", true) => ("fonts/Roboto-Bold.ttf", FontTweak::default()),
        ("Impact", _) => {
            let mut bytes = font_file("fonts/Anton-Wide.ttf")?;
            set_line_box(
                &mut bytes,
                IMPACT_ASCENT / ANTON_SCALE,
                IMPACT_DESCENT / ANTON_SCALE,
            )?;
            let tweak = FontTweak {
                scale: ANTON_SCALE,
                ..FontTweak::default()
            };
            return Some((bytes, tweak));
        }
        _ => return None,
    };
    Some((font_file(file)?, tweak))
}

fn font_file(path: &str) -> Option<Vec<u8>> {
    crate::assets::load(path).ready().map(Cow::into_owned)
}

/// Give the TrueType font in `bytes` a line box `ascent` em above the baseline and
/// `descent` em below it with no gap, in every table a reader may take it from (`hhea`,
/// and `OS/2`'s typographic and Windows metrics). `None` when a table is missing.
fn set_line_box(bytes: &mut [u8], ascent: f32, descent: f32) -> Option<()> {
    let read =
        |bytes: &[u8], at: usize| Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]));
    let table = |bytes: &[u8], tag: &[u8; 4]| {
        let count = usize::from(read(bytes, 4)?);
        (0..count).find_map(|index| {
            let entry = 12 + 16 * index;
            (bytes.get(entry..entry + 4)? == tag).then(|| {
                let offset = bytes.get(entry + 8..entry + 12)?;
                Some(u32::from_be_bytes(offset.try_into().ok()?) as usize)
            })?
        })
    };
    let units = f32::from(read(bytes, table(bytes, b"head")? + 18)?);
    let up = (ascent * units).round() as i16;
    let down = (descent * units).round() as i16;
    let hhea = table(bytes, b"hhea")?;
    let os2 = table(bytes, b"OS/2")?;
    let mut write = |at: usize, value: i16| -> Option<()> {
        bytes
            .get_mut(at..at + 2)?
            .copy_from_slice(&value.to_be_bytes());
        Some(())
    };
    write(hhea + 4, up)?;
    write(hhea + 6, -down)?;
    write(hhea + 8, 0)?;
    write(os2 + 68, up)?;
    write(os2 + 70, -down)?;
    write(os2 + 72, 0)?;
    // usWinAscent and usWinDescent are unsigned, the descent counted downwards.
    write(os2 + 74, up)?;
    write(os2 + 76, down)
}

/// Fonts and visuals for the whole interface.
pub fn install(context: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, family: &str, style: FontStyle| {
        if let Some((bytes, tweak)) = system_font(family, style)
            .map(|(bytes, _)| (bytes, FontTweak::default()))
            .or_else(|| embedded_font(family, style))
        {
            let mut data = FontData::from_owned(bytes).tweak(tweak);
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
    // Monospace falls back to the UI face: a page has no monospace face of its own
    // (Stats for nerds measures its columns in the face it gets).
    let monospace = fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .expect("egui has a monospace family");
    if has_ui {
        monospace.insert(0, "ui".into());
    }
    if has_mono {
        monospace.insert(0, "ui-mono".into());
    }
    // A glyph the bold or display face lacks comes from the UI face.
    let with = |first: Option<&str>| {
        let mut family: Vec<String> = first.map(str::to_owned).into_iter().collect();
        family.extend(has_ui.then(|| "ui".to_owned()));
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
    // A hand over every button, as the page's `button { cursor: pointer }`; the painted
    // ones show it through `paint::labelled`.
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    context.set_global_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page's Anton must lay out like Impact, which the screens were measured in:
    /// with its own line box, centred and bottom-aligned text sat a quarter em high.
    #[test]
    fn the_embedded_anton_takes_impacts_line_box() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../public/fonts/Anton-Wide.ttf");
        let mut bytes = std::fs::read(path).expect("public/fonts holds Anton Wide");
        set_line_box(
            &mut bytes,
            IMPACT_ASCENT / ANTON_SCALE,
            IMPACT_DESCENT / ANTON_SCALE,
        )
        .expect("Anton has head, hhea and OS/2 tables");
        let mut fonts = FontDefinitions::default();
        let tweak = FontTweak {
            scale: ANTON_SCALE,
            ..FontTweak::default()
        };
        fonts.font_data.insert(
            "anton".into(),
            FontData::from_owned(bytes).tweak(tweak).into(),
        );
        fonts.families.insert(display(), vec!["anton".into()]);
        let context = egui::Context::default();
        context.set_fonts(fonts);
        let size = 100.0;
        let mut row = 0.0;
        let mut output = context.run_pass(egui::RawInput::default(), |context| {
            row = context.fonts_mut(|fonts| fonts.row_height(&display_font(size)));
        });
        // No painter takes the font atlas here.
        output.textures_delta.clear();
        let impact = size * (IMPACT_ASCENT + IMPACT_DESCENT);
        assert!(
            (row - impact).abs() <= 1.0,
            "Anton's line is {row} points tall at {size}, Impact's {impact}"
        );
    }
}
