//! The battle interface's look in egui (`src/style.css`): dark steel-glass plates with
//! a steel hairline, gold for what is selected, blue and red for the teams, Impact for
//! display type and the platform's UI face for the rest, like the page's system-ui. A
//! page cannot read the system's fonts, so there the build embeds its own
//! (`public/fonts/interface.ttc`, made by `scripts/build-fonts.py`): Anton (OFL), widened
//! to Impact's width, for Impact and Roboto (Apache 2.0) for the UI face.

use std::sync::Arc;

use egui::epaint::text::Blob;
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

/// The page's faces, in one collection file (`scripts/build-fonts.py`) at these indices.
const FONT_COLLECTION: &str = "fonts/interface.ttc";
const ANTON_FACE: u32 = 0;
const ROBOTO_FACE: u32 = 1;
const ROBOTO_BOLD_FACE: u32 = 2;

/// The game's own collection (a page's, which `index.html` fetches before the shell
/// starts), with Anton given Impact's line box. Its faces share these bytes.
fn embedded_collection() -> Option<Blob> {
    font_collection(crate::assets::load(FONT_COLLECTION).ready()?.into_owned())
}

/// The collection file in `bytes` with Anton given Impact's line box.
fn font_collection(mut bytes: Vec<u8>) -> Option<Blob> {
    set_line_box(
        &mut bytes,
        ANTON_FACE,
        IMPACT_ASCENT / ANTON_SCALE,
        IMPACT_DESCENT / ANTON_SCALE,
    )?;
    Some(Arc::new(bytes))
}

/// The collection's face for `family` in `style`, where it has one, with how to draw it
/// in that family's place.
fn embedded_font(collection: &Blob, family: &str, style: FontStyle) -> Option<FontData> {
    let face = |index| FontData::from_blob(Arc::clone(collection), index);
    match (family, style.weight >= 600) {
        ("system-ui" | "sans-serif", false) => Some(face(ROBOTO_FACE)),
        ("system-ui" | "sans-serif", true) => Some(face(ROBOTO_BOLD_FACE)),
        ("Impact", _) => Some(face(ANTON_FACE).tweak(FontTweak {
            scale: ANTON_SCALE,
            ..FontTweak::default()
        })),
        _ => None,
    }
}

/// Give face `face` of the TrueType collection in `bytes` a line box `ascent` em above
/// the baseline and `descent` em below it with no gap, in every table a reader may take
/// it from (`hhea`, and `OS/2`'s typographic and Windows metrics). `None` when a table
/// is missing. The tables are the face's own: no other face of the collection shares
/// Anton's metrics.
fn set_line_box(bytes: &mut [u8], face: u32, ascent: f32, descent: f32) -> Option<()> {
    let read =
        |bytes: &[u8], at: usize| Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]));
    let read_u32 = |bytes: &[u8], at: usize| {
        Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize)
    };
    // The collection header lists each face's table directory, whose offsets count from
    // the start of the file.
    if bytes.get(..4)? != b"ttcf" {
        return None;
    }
    let directory = read_u32(bytes, 12 + 4 * face as usize)?;
    let table = |bytes: &[u8], tag: &[u8; 4]| {
        let count = usize::from(read(bytes, directory + 4)?);
        (0..count).find_map(|index| {
            let entry = directory + 12 + 16 * index;
            (bytes.get(entry..entry + 4)? == tag).then(|| read_u32(bytes, entry + 8))?
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
    let collection = embedded_collection();
    let mut add = |name: &str, family: &str, style: FontStyle| {
        if let Some(mut data) = system_font(family, style)
            .map(|(bytes, _)| FontData::from_owned(bytes))
            .or_else(|| embedded_font(collection.as_ref()?, family, style))
        {
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
    // Glyph coverage as rasterised: the dark theme's boost thickened light text past
    // the page's.
    visuals.text_options.color_transfer_function = egui::epaint::FontColorTransferFunction::Off;
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
            .join("../../public")
            .join(FONT_COLLECTION);
        let bytes = std::fs::read(path).expect("public/fonts holds the collection");
        let collection = font_collection(bytes).expect("the collection has Anton's tables");
        let style = FontStyle {
            weight: 400,
            italic: false,
        };
        let anton =
            embedded_font(&collection, "Impact", style).expect("the collection has a display face");
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert("anton".into(), anton.into());
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
