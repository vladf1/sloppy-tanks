//! Canvas 2D text and shapes natively: the [`CanvasOp`]s replayed with tiny-skia,
//! glyph outlines from the system's own fonts (the ones a browser on the same
//! machine would pick for the CSS fonts the scenery names), so the signs read the
//! same as on the page.

use ab_glyph::{Font, FontVec, OutlineCurve, PxScale, ScaleFont};
use sloppy_core::models::effects_scenery::{CanvasOp, CanvasTexture};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

use crate::gpu::{ImageData, image_data};
use crate::system_fonts::{FontStyle, system_font};

/// The slant a browser gives an italic it synthesizes from an upright face.
const SYNTHETIC_OBLIQUE: f32 = 0.2;

/// Replay the steps on a fresh, transparent canvas and return its RGBA rows (top
/// first, straight alpha).
pub fn draw_canvas(canvas: &CanvasTexture) -> Result<ImageData, String> {
    let mut pixmap = Pixmap::new(canvas.width, canvas.height).ok_or("empty canvas")?;
    for op in &canvas.ops {
        match op {
            CanvasOp::FillRect { color, rect } => {
                if let Some(rect) = rect_of(rect) {
                    pixmap.fill_rect(rect, &paint(color)?, Transform::identity(), None);
                }
            }
            CanvasOp::StrokeRect {
                color,
                line_width,
                rect,
            } => {
                if let Some(path) = rect_of(rect).map(PathBuilder::from_rect) {
                    let stroke = Stroke {
                        width: *line_width as f32,
                        ..Stroke::default()
                    };
                    pixmap.stroke_path(&path, &paint(color)?, &stroke, Transform::identity(), None);
                }
            }
            CanvasOp::FillPolygon { color, points } => {
                let mut path = PathBuilder::new();
                for (index, point) in points.iter().enumerate() {
                    let (x, y) = (point[0] as f32, point[1] as f32);
                    if index == 0 {
                        path.move_to(x, y);
                    } else {
                        path.line_to(x, y);
                    }
                }
                path.close();
                if let Some(path) = path.finish() {
                    pixmap.fill_path(
                        &path,
                        &paint(color)?,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
            CanvasOp::FillText {
                color,
                font,
                align,
                baseline,
                text,
                at,
                max_width,
            } => fill_text(
                &mut pixmap,
                &paint(color)?,
                font,
                align,
                baseline,
                text,
                *at,
                *max_width,
            )?,
        }
    }
    let pixels: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    image_data(canvas.width, canvas.height, &pixels)
}

fn rect_of(rect: &[f64; 4]) -> Option<Rect> {
    Rect::from_xywh(
        rect[0] as f32,
        rect[1] as f32,
        rect[2] as f32,
        rect[3] as f32,
    )
}

fn paint(color: &str) -> Result<Paint<'static>, String> {
    let [red, green, blue] = hex_color(color).ok_or_else(|| format!("color {color}"))?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(red, green, blue, 255);
    paint.anti_alias = true;
    Ok(paint)
}

/// `#rgb` or `#rrggbb`.
fn hex_color(color: &str) -> Option<[u8; 3]> {
    let digits = color.strip_prefix('#')?;
    let channel = |text: &str| u8::from_str_radix(text, 16).ok();
    match digits.len() {
        6 => Some([
            channel(&digits[0..2])?,
            channel(&digits[2..4])?,
            channel(&digits[4..6])?,
        ]),
        3 => {
            let short = |index: usize| channel(&digits[index..=index]).map(|value| value * 17);
            Some([short(0)?, short(1)?, short(2)?])
        }
        _ => None,
    }
}

/// A CSS font shorthand as the scenery writes it: `[italic] [bold|<weight>] <size>px
/// <family>`.
#[derive(Debug, PartialEq)]
struct CssFont<'a> {
    italic: bool,
    weight: u16,
    size: f32,
    family: &'a str,
}

fn parse_font(font: &str) -> Option<CssFont<'_>> {
    let mut parsed = CssFont {
        italic: false,
        weight: 400,
        size: 10.0,
        family: "",
    };
    let mut rest = font.trim();
    loop {
        let (word, tail) = rest.split_once(' ').unwrap_or((rest, ""));
        match word {
            "italic" | "oblique" => parsed.italic = true,
            "normal" => {}
            "bold" => parsed.weight = 700,
            _ if word.ends_with("px") => {
                parsed.size = word.trim_end_matches("px").parse().ok()?;
                parsed.family = tail.trim().trim_matches(['"', '\'']);
                return Some(parsed);
            }
            _ => parsed.weight = word.parse().ok()?,
        }
        rest = tail;
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_text(
    pixmap: &mut Pixmap,
    paint: &Paint,
    font: &str,
    align: &str,
    baseline: &str,
    text: &str,
    at: [f64; 2],
    max_width: Option<f64>,
) -> Result<(), String> {
    let css = parse_font(font).ok_or_else(|| format!("font {font}"))?;
    let style = FontStyle {
        weight: css.weight,
        italic: css.italic,
    };
    let (face, has_italic) =
        system_font(css.family, style).ok_or_else(|| format!("no system font for {font}"))?;
    let face = FontVec::try_from_vec(face).map_err(|error| format!("{font}: {error}"))?;
    let scaled = face.as_scaled(PxScale::from(css.size));
    let glyphs: Vec<_> = text.chars().map(|c| face.glyph_id(c)).collect();
    let mut advances = Vec::with_capacity(glyphs.len());
    let mut width = 0.0;
    for (index, &glyph) in glyphs.iter().enumerate() {
        if index > 0 {
            width += scaled.kern(glyphs[index - 1], glyph);
        }
        advances.push(width);
        width += scaled.h_advance(glyph);
    }
    // `maxWidth` squeezes the line horizontally, as browsers do.
    let squeeze = match max_width {
        Some(max) if f64::from(width) > max && width > 0.0 => max as f32 / width,
        _ => 1.0,
    };
    let left = match align {
        "center" => at[0] as f32 - width * squeeze / 2.0,
        "right" | "end" => at[0] as f32 - width * squeeze,
        _ => at[0] as f32,
    };
    let (ascent, descent) = (scaled.ascent(), scaled.descent());
    let baseline_y = at[1] as f32
        + match baseline {
            "middle" => (ascent + descent) / 2.0,
            "top" | "hanging" => ascent,
            "bottom" | "ideographic" => descent,
            _ => 0.0,
        };
    let slant = if css.italic && !has_italic {
        SYNTHETIC_OBLIQUE
    } else {
        0.0
    };
    let units = face.units_per_em().unwrap_or(1000.0);
    let scale = css.size / units;
    let mut path = PathBuilder::new();
    for (&glyph, &advance) in glyphs.iter().zip(&advances) {
        let Some(outline) = face.outline(glyph) else {
            continue;
        };
        // Font units, y up, to canvas pixels, y down, slanted about the baseline.
        let point = |p: ab_glyph::Point| {
            let x = advance + (p.x + p.y * slant) * scale;
            (left + x * squeeze, baseline_y - p.y * scale)
        };
        let mut last = None;
        for curve in &outline.curves {
            let (start, end) = match curve {
                OutlineCurve::Line(a, b)
                | OutlineCurve::Quad(a, _, b)
                | OutlineCurve::Cubic(a, _, _, b) => (*a, *b),
            };
            if last != Some(start) {
                if last.is_some() {
                    path.close();
                }
                let (x, y) = point(start);
                path.move_to(x, y);
            }
            match curve {
                OutlineCurve::Line(_, b) => {
                    let (x, y) = point(*b);
                    path.line_to(x, y);
                }
                OutlineCurve::Quad(_, c, b) => {
                    let ((cx, cy), (x, y)) = (point(*c), point(*b));
                    path.quad_to(cx, cy, x, y);
                }
                OutlineCurve::Cubic(_, c1, c2, b) => {
                    let ((ax, ay), (bx, by), (x, y)) = (point(*c1), point(*c2), point(*b));
                    path.cubic_to(ax, ay, bx, by, x, y);
                }
            }
            last = Some(end);
        }
        if last.is_some() {
            path.close();
        }
    }
    if let Some(path) = path.finish() {
        pixmap.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::models::effects_scenery::{
        HARBOR_LABEL_TEXTURE, QUARRY_SIGN_TEXTURE, VILLAGE_SIGN_TEXTURE, canvas_texture,
    };

    #[test]
    fn fonts_parse_like_css_shorthand() {
        assert_eq!(
            parse_font("italic 900 150px sans-serif"),
            Some(CssFont {
                italic: true,
                weight: 900,
                size: 150.0,
                family: "sans-serif"
            })
        );
        assert_eq!(
            parse_font("bold 48px Georgia").map(|font| (font.weight, font.family)),
            Some((700, "Georgia"))
        );
        assert_eq!(
            parse_font("24px sans-serif").map(|font| font.weight),
            Some(400)
        );
    }

    #[test]
    fn colors_parse_both_hex_forms() {
        assert_eq!(hex_color("#816b45"), Some([0x81, 0x6b, 0x45]));
        assert_eq!(hex_color("#fff"), Some([255, 255, 255]));
        assert_eq!(hex_color("red"), None);
    }

    /// The alpha of the pixel at `(x, y)`.
    fn alpha(image: &ImageData, x: u32, y: u32) -> u8 {
        image.pixels[((y * image.width() + x) * 4 + 3) as usize]
    }

    #[test]
    fn labels_draw_centered_text_on_each_row() {
        let Some(texture) = canvas_texture(HARBOR_LABEL_TEXTURE) else {
            unreachable!("the harbor labels are a canvas texture");
        };
        let image = draw_canvas(&texture).expect("labels draw");
        // Each 256-row band has ink around its middle and none at its edges or corners.
        for row in 0..texture.height / 256 {
            let middle = row * 256 + 128;
            let inked = (0..texture.width)
                .filter(|&x| alpha(&image, x, middle) > 0)
                .count();
            assert!(inked > 20, "row {row} has text");
            assert_eq!(alpha(&image, 2, row * 256 + 4), 0);
            let first = (0..texture.width)
                .find(|&x| alpha(&image, x, middle) > 0)
                .unwrap_or(0);
            let last = (0..texture.width)
                .rev()
                .find(|&x| alpha(&image, x, middle) > 0)
                .unwrap_or(0);
            let center = (first + last) as i64 / 2;
            assert!((center - 512).abs() < 40, "row {row} centered at {center}");
        }
    }

    #[test]
    fn signs_fill_their_boards() {
        for key in [VILLAGE_SIGN_TEXTURE, QUARRY_SIGN_TEXTURE] {
            let Some(texture) = canvas_texture(key) else {
                unreachable!("{key} is a canvas texture");
            };
            let image = draw_canvas(&texture).expect("sign draws");
            assert_eq!(alpha(&image, 1, 1), 255, "{key} board is opaque");
        }
    }
}
