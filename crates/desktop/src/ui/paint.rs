//! Painting the page's CSS looks with egui shapes: vertical and horizontal gradients,
//! the armour plates (top-left and bottom-right corners cut, a steel hairline round the
//! cut), spaced type, key caps, timer rings, vignettes and the dialogs' buttons.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align2, Color32, FontId, Galley, Mesh, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2, pos2, vec2,
};

use super::icons::Art;
use super::theme::{self, GOLD, bold_font, display_font};

/// `#rrggbb`.
pub fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// `#rrggbb` at `alpha` (0 to 1).
pub fn hexa(rgb: u32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (rgb >> 16) as u8,
        (rgb >> 8) as u8,
        rgb as u8,
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// `color` at `alpha` times its own opacity.
pub fn fade(color: Color32, alpha: f32) -> Color32 {
    color.gamma_multiply(alpha.clamp(0.0, 1.0))
}

/// `a` mixed toward `b` by `t`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
}

/// A vertical gradient over `rect` through `stops` (share of the height, colour).
pub fn vgradient(rect: Rect, stops: &[(f32, Color32)]) -> Shape {
    let mut mesh = Mesh::default();
    for (index, &(share, color)) in stops.iter().enumerate() {
        let y = rect.top() + rect.height() * share;
        mesh.colored_vertex(pos2(rect.left(), y), color);
        mesh.colored_vertex(pos2(rect.right(), y), color);
        if index > 0 {
            let base = (index as u32 - 1) * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 3, base + 2);
        }
    }
    Shape::mesh(mesh)
}

/// A vertical gradient through `stops` filling `rect` with its corners rounded by
/// `radius` (north-west, north-east, south-west, south-east), as one mesh: a gradient
/// laid over a rounded fill leaves bands where the two differ near the corners.
pub fn rounded_vgradient(rect: Rect, radius: [f32; 4], stops: &[(f32, Color32)]) -> Shape {
    const ARC_STEPS: usize = 8;
    let [nw, ne, sw, se] = radius.map(|r| r.min(rect.width() / 2.0).min(rect.height() / 2.0));
    // How far a corner of radius `r` pulls the edge in at `depth` into its band.
    let inset = |r: f32, depth: f32| {
        if depth >= r {
            0.0
        } else {
            r - (r * r - (r - depth) * (r - depth)).max(0.0).sqrt()
        }
    };
    let color_at = |share: f32| {
        let mut previous = stops[0];
        for &stop in stops {
            if share <= stop.0 {
                let span = (stop.0 - previous.0).max(f32::EPSILON);
                return mix(previous.1, stop.1, (share - previous.0) / span);
            }
            previous = stop;
        }
        previous.1
    };
    let mut ys: Vec<f32> = stops
        .iter()
        .map(|(share, _)| rect.top() + rect.height() * share)
        .collect();
    let top_band = nw.max(ne);
    let bottom_band = sw.max(se);
    for step in 0..=ARC_STEPS {
        let t = step as f32 / ARC_STEPS as f32;
        ys.push(rect.top() + top_band * t);
        ys.push(rect.bottom() - bottom_band * t);
    }
    ys.retain(|y| (rect.top()..=rect.bottom()).contains(y));
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    let mut mesh = Mesh::default();
    for (index, &y) in ys.iter().enumerate() {
        let from_top = y - rect.top();
        let from_bottom = rect.bottom() - y;
        let left = rect.left() + inset(nw, from_top).max(inset(sw, from_bottom));
        let right = rect.right() - inset(ne, from_top).max(inset(se, from_bottom));
        let color = color_at((y - rect.top()) / rect.height().max(f32::EPSILON));
        mesh.colored_vertex(pos2(left, y), color);
        mesh.colored_vertex(pos2(right, y), color);
        if index > 0 {
            let base = (index as u32 - 1) * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 3, base + 2);
        }
    }
    Shape::mesh(mesh)
}

/// A horizontal gradient over `rect` through `stops` (share of the width, colour).
pub fn hgradient(rect: Rect, stops: &[(f32, Color32)]) -> Shape {
    let mut mesh = Mesh::default();
    for (index, &(share, color)) in stops.iter().enumerate() {
        let x = rect.left() + rect.width() * share;
        mesh.colored_vertex(pos2(x, rect.top()), color);
        mesh.colored_vertex(pos2(x, rect.bottom()), color);
        if index > 0 {
            let base = (index as u32 - 1) * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 3, base + 2);
        }
    }
    Shape::mesh(mesh)
}

/// A convex polygon filled with a vertical gradient from `top` to `bottom` of its
/// bounds (a fan from its centre, which interpolates a vertical ramp exactly).
pub fn gradient_polygon(points: &[Pos2], top: Color32, bottom: Color32) -> Shape {
    let bounds = Rect::from_points(points);
    let color = |y: f32| {
        let t = if bounds.height() > 0.0 {
            (y - bounds.top()) / bounds.height()
        } else {
            0.0
        };
        mix(top, bottom, t)
    };
    let mut mesh = Mesh::default();
    let center = points
        .iter()
        .fold(Vec2::ZERO, |sum, point| sum + point.to_vec2())
        / points.len().max(1) as f32;
    mesh.colored_vertex(center.to_pos2(), color(center.y));
    for point in points {
        mesh.colored_vertex(*point, color(point.y));
    }
    let count = points.len() as u32;
    for index in 0..count {
        mesh.add_triangle(0, 1 + index, 1 + (index + 1) % count);
    }
    Shape::mesh(mesh)
}

/// The armour plate's outline: `rect` with its top-left and bottom-right corners cut.
pub fn cut_corners(rect: Rect, cut: f32) -> Vec<Pos2> {
    vec![
        pos2(rect.left() + cut, rect.top()),
        rect.right_top(),
        pos2(rect.right(), rect.bottom() - cut),
        pos2(rect.right() - cut, rect.bottom()),
        rect.left_bottom(),
        pos2(rect.left(), rect.top() + cut),
    ]
}

/// The plates' steel glass (`--plate-fill`) and hairline (`--plate-edge`).
pub const PLATE_TOP: u32 = 0x142a4e;
pub const PLATE_BOTTOM: u32 = 0x060f24;

/// An armour plate over `rect`, faintly tinted from its lower-left corner by `tint`.
pub fn plate(painter: &Painter, rect: Rect, cut: f32, tint: Option<Color32>) {
    painter.extend(plate_shapes(rect, cut, tint));
}

/// The shapes of [`plate`], for a slot reserved under content laid out first.
pub fn plate_shapes(rect: Rect, cut: f32, tint: Option<Color32>) -> Vec<Shape> {
    let outline = cut_corners(rect, cut);
    let mut shapes = vec![gradient_polygon(
        &outline,
        hexa(PLATE_TOP, 0.97),
        hexa(PLATE_BOTTOM, 0.99),
    )];
    if let Some(tint) = tint {
        // A radial wash from the lower-left corner, as a fan fading outwards.
        let corner = rect.left_bottom();
        let reach = vec2(rect.width() * 0.7, rect.height() * 1.3);
        let mut mesh = Mesh::default();
        mesh.colored_vertex(corner, fade(tint, 0.22));
        let steps = 10;
        for step in 0..=steps {
            let angle = -std::f32::consts::FRAC_PI_2 * step as f32 / steps as f32;
            let point = corner + vec2(angle.cos() * reach.x, angle.sin() * reach.y);
            let clamped = pos2(point.x.min(rect.right()), point.y.max(rect.top()));
            mesh.colored_vertex(clamped, Color32::TRANSPARENT);
            if step > 0 {
                mesh.add_triangle(0, step, step + 1);
            }
        }
        shapes.push(Shape::mesh(mesh));
    }
    // The sheen along the top and the shade along the bottom, inside the hairline.
    shapes.push(Shape::line_segment(
        [
            pos2(rect.left() + cut, rect.top() + 1.5),
            pos2(rect.right() - 1.0, rect.top() + 1.5),
        ],
        Stroke::new(1.5, hexa(0xffffff, 0.08)),
    ));
    shapes.push(Shape::line_segment(
        [
            pos2(rect.left() + 1.0, rect.bottom() - 1.5),
            pos2(rect.right() - cut, rect.bottom() - 1.5),
        ],
        Stroke::new(1.5, hexa(0x000000, 0.35)),
    ));
    shapes.push(Shape::closed_line(
        outline,
        Stroke::new(1.0, theme::PLATE_EDGE),
    ));
    shapes
}

/// A radial gradient: an ellipse of `radii` round `center` through `stops` (share of
/// the radii, colour), the first stop at the centre.
pub fn radial(center: Pos2, radii: Vec2, stops: &[(f32, Color32)]) -> Shape {
    let segments = 48u32;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(
        center,
        stops.first().map_or(Color32::TRANSPARENT, |stop| stop.1),
    );
    for &(share, color) in &stops[1..] {
        for segment in 0..segments {
            let angle = segment as f32 / segments as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(
                center + vec2(angle.cos() * radii.x, angle.sin() * radii.y) * share,
                color,
            );
        }
    }
    for segment in 0..segments {
        mesh.add_triangle(0, 1 + segment, 1 + (segment + 1) % segments);
    }
    for ring in 0..stops.len().saturating_sub(2) as u32 {
        for segment in 0..segments {
            let a = 1 + ring * segments + segment;
            let b = 1 + ring * segments + (segment + 1) % segments;
            mesh.add_triangle(a, b, a + segments);
            mesh.add_triangle(b, b + segments, a + segments);
        }
    }
    Shape::mesh(mesh)
}

/// Text wrapped at `width`.
pub fn wrapped(
    painter: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> Arc<Galley> {
    painter.layout(text.to_owned(), font, color, width)
}

/// A rounded glass panel (`--glass`): the dialogs' wells and cards.
pub fn glass(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    top: Color32,
    bottom: Color32,
    edge: Color32,
) {
    painter.add(rounded_vgradient(
        rect,
        [radius; 4],
        &[(0.0, top), (1.0, bottom)],
    ));
    painter.rect_stroke(
        rect,
        radius.round() as u8,
        Stroke::new(1.0, edge),
        StrokeKind::Inside,
    );
}

/// Text laid out on one line, `spacing` points between letters, cut with an ellipsis
/// beyond `max_width`.
pub fn galley(
    painter: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    spacing: f32,
    max_width: f32,
) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font,
            color,
            extra_letter_spacing: spacing,
            ..Default::default()
        },
    );
    job.wrap = TextWrapping::truncate_at_width(max_width);
    painter.layout_job(job)
}

/// Text anchored at `pos` by `align`; returns where it went.
pub fn text(
    painter: &Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    spacing: f32,
) -> Rect {
    let galley = galley(painter, text, font, color, spacing, f32::INFINITY);
    let rect = align.anchor_size(pos, galley.size());
    painter.galley(rect.min, galley, color);
    rect
}

/// Text with a soft drop shadow under it, for type over the arena.
pub fn shadowed(
    painter: &Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    spacing: f32,
) -> Rect {
    let shadow = galley(
        painter,
        text,
        font.clone(),
        hexa(0x000000, 0.55),
        spacing,
        f32::INFINITY,
    );
    let rect = align.anchor_size(pos, shadow.size());
    painter.galley(rect.min + vec2(0.0, 1.5), shadow, Color32::BLACK);
    let galley = galley(painter, text, font, color, spacing, f32::INFINITY);
    painter.galley(rect.min, galley, color);
    rect
}

/// Text cut to `width` with an ellipsis, anchored at `pos`.
#[allow(clippy::too_many_arguments)]
pub fn text_fit(
    painter: &Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    spacing: f32,
    width: f32,
) -> Rect {
    let galley = galley(painter, text, font, color, spacing, width);
    let rect = align.anchor_size(pos, galley.size());
    painter.galley(rect.min, galley, color);
    rect
}

/// The width `text` takes.
pub fn text_width(painter: &Painter, text: &str, font: FontId, spacing: f32) -> f32 {
    galley(painter, text, font, Color32::WHITE, spacing, f32::INFINITY)
        .size()
        .x
}

/// A key cap (`kbd`) centred at `center`; returns its rect.
pub fn kbd(painter: &Painter, center: Pos2, label: &str, size: f32) -> Rect {
    let font = bold_font(size);
    let width = kbd_width(painter, label, size);
    let rect = Rect::from_center_size(center, vec2(width, size * 1.3 + 10.0));
    painter.rect_filled(rect.translate(vec2(0.0, 1.0)), 4, hex(0x050c18));
    painter.add(rounded_vgradient(
        rect.shrink(0.5),
        [4.0; 4],
        &[(0.0, hex(0x3d659e)), (1.0, hex(0x1f3b66))],
    ));
    painter.rect_stroke(
        rect,
        4,
        Stroke::new(1.0, hexa(0x8bb9dc, 0.4)),
        StrokeKind::Inside,
    );
    text(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font,
        Color32::WHITE,
        0.4,
    );
    rect
}

/// The width a key cap for `label` takes.
pub fn kbd_width(painter: &Painter, label: &str, size: f32) -> f32 {
    (text_width(painter, label, bold_font(size), 0.4) + 16.0).max(24.0)
}

/// An arc of `radius` from `start` (radians clockwise from straight up) over `sweep`.
pub fn arc(painter: &Painter, center: Pos2, radius: f32, start: f32, sweep: f32, stroke: Stroke) {
    if sweep.abs() < 1e-3 {
        return;
    }
    let steps = ((sweep.abs() / std::f32::consts::TAU) * 64.0)
        .ceil()
        .max(2.0) as usize;
    let points = (0..=steps)
        .map(|step| {
            let angle = start + sweep * step as f32 / steps as f32;
            center + vec2(angle.sin(), -angle.cos()) * radius
        })
        .collect();
    painter.add(Shape::line(points, stroke));
}

/// A timer ring: a dark track and the share left drawn clockwise from the top.
pub fn timer_ring(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    width: f32,
    share: f32,
    color: Color32,
) {
    painter.circle_stroke(center, radius, Stroke::new(width, hexa(0x02060f, 0.9)));
    arc(
        painter,
        center,
        radius,
        0.0,
        std::f32::consts::TAU * share.clamp(0.0, 1.0),
        Stroke::new(width, color),
    );
}

/// An elliptical vignette over `rect`: the radial gradient `stops` (share of the
/// ellipse's radii `radii`, a share of the rect's size, and colour), at `opacity`.
pub fn vignette(
    painter: &Painter,
    rect: Rect,
    radii: Vec2,
    stops: &[(f32, Color32)],
    opacity: f32,
) {
    vignette_at(painter, rect, rect.center(), radii, stops, opacity);
}

/// A vignette with an offset optical centre, as the cockpit's CSS uses.
pub fn vignette_at(
    painter: &Painter,
    rect: Rect,
    center: Pos2,
    radii: Vec2,
    stops: &[(f32, Color32)],
    opacity: f32,
) {
    if opacity <= 0.0 {
        return;
    }
    let radius = vec2(rect.width() * radii.x, rect.height() * radii.y);
    let segments = 48;
    let mut mesh = Mesh::default();
    // The last stop's colour carries on past the corners.
    let mut rings: Vec<(f32, Color32)> = stops.to_vec();
    if let Some(&(_, last)) = stops.last() {
        rings.push((3.0, last));
    }
    for &(share, color) in &rings {
        for segment in 0..segments {
            let angle = segment as f32 / segments as f32 * std::f32::consts::TAU;
            let point = center + vec2(angle.cos() * radius.x, angle.sin() * radius.y) * share;
            mesh.colored_vertex(point, fade(color, opacity));
        }
    }
    for ring in 0..rings.len() as u32 - 1 {
        for segment in 0..segments {
            let a = ring * segments + segment;
            let b = ring * segments + (segment + 1) % segments;
            let c = a + segments;
            let d = b + segments;
            mesh.add_triangle(a, b, c);
            mesh.add_triangle(b, d, c);
        }
    }
    painter.with_clip_rect(rect).add(Shape::mesh(mesh));
}

/// How a dialog button looks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// The one gold action.
    Primary,
    /// Glass with a light rim.
    Secondary,
    /// Glass with a red rim: throws progress away.
    Danger,
}

/// A dialog button with an optional leading icon, `size` wide and tall.
pub fn button(
    ui: &mut Ui,
    art: &Art,
    label: &str,
    icon: Option<&str>,
    style: ButtonStyle,
    size: Vec2,
    enabled: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = enabled && response.hovered();
    let pressed = enabled && response.is_pointer_button_down_on();
    let rect = rect.translate(vec2(
        0.0,
        if pressed {
            1.0
        } else if hovered {
            -1.0
        } else {
            0.0
        },
    ));
    let painter = ui.painter();
    let ink = match style {
        ButtonStyle::Primary => hex(0x1a2233),
        ButtonStyle::Secondary => {
            if hovered {
                Color32::WHITE
            } else {
                hex(0xe6f1f9)
            }
        }
        ButtonStyle::Danger => hex(0xffb2a7),
    };
    match style {
        ButtonStyle::Primary => {
            if hovered {
                painter.rect_filled(rect.expand(3.0), 12, hexa(0xffb21c, 0.25));
            }
            painter.rect_filled(rect, 10, hex(0xf4a91a));
            let top = if enabled {
                hex(0xffe46e)
            } else {
                hex(0x9a8a50)
            };
            let middle = if enabled { GOLD } else { hex(0x8c7a3c) };
            let bottom = if enabled {
                hex(0xf4a91a)
            } else {
                hex(0x6e5a24)
            };
            painter.add(rounded_vgradient(
                rect,
                [10.0; 4],
                &[
                    (0.0, top),
                    (0.48, middle),
                    (0.91, bottom),
                    (0.92, hex(0xc97d0e)),
                    (1.0, hex(0xc97d0e)),
                ],
            ));
        }
        ButtonStyle::Secondary | ButtonStyle::Danger => {
            painter.rect_filled(rect, 10, hexa(0xffffff, if hovered { 0.09 } else { 0.035 }));
            let rim = match style {
                ButtonStyle::Danger => hexa(0xff6f5e, 0.6),
                _ if hovered => hex(0xcfe8fa),
                _ => hexa(0x8bc2ec, 0.4),
            };
            painter.rect_stroke(rect, 10, Stroke::new(1.5, rim), StrokeKind::Inside);
        }
    }
    let font = if style == ButtonStyle::Primary && size.y >= 52.0 {
        display_font(size.y * 0.5)
    } else {
        bold_font(15.0)
    };
    let spacing = if style == ButtonStyle::Primary && size.y >= 52.0 {
        1.0
    } else {
        1.5
    };
    let label_width = text_width(painter, label, font.clone(), spacing);
    let icon_size = 16.0;
    let total = label_width + icon.map_or(0.0, |_| icon_size + 8.0);
    let mut x = rect.center().x - total / 2.0;
    if let Some(icon) = icon {
        art.icon(
            painter,
            icon,
            Rect::from_center_size(
                pos2(x + icon_size / 2.0, rect.center().y),
                Vec2::splat(icon_size),
            ),
            ink,
        );
        x += icon_size + 8.0;
    }
    text(
        painter,
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        ink,
        spacing,
    );
    response
}

/// A round icon button (the dialogs' corner X).
pub fn close_button(ui: &mut Ui, art: &Art, rect: Rect, id: egui::Id) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 8, hexa(0xffffff, 0.08));
    }
    let tint = if response.hovered() {
        Color32::WHITE
    } else {
        hex(0xc4d5e7)
    };
    art.icon(painter, "close", rect.shrink(rect.width() * 0.22), tint);
    response
}

/// A section label with its gold accent bar (`.setup-label`).
pub fn section_label(painter: &Painter, pos: Pos2, label: &str) -> Rect {
    let bar = Rect::from_min_size(pos + vec2(0.0, 2.0), vec2(4.0, 15.0));
    painter.rect_filled(bar.expand(2.0), 2, hexa(0xffcf38, 0.15));
    painter.rect_filled(bar, 1, GOLD);
    shadowed(
        painter,
        pos + vec2(13.0, 0.0),
        Align2::LEFT_TOP,
        label,
        display_font(19.0),
        hex(0xeef5fc),
        1.2,
    )
}

/// The eyebrow over a dialog's title: small, spaced, light blue capitals.
pub fn eyebrow(painter: &Painter, pos: Pos2, label: &str) -> Rect {
    text(
        painter,
        pos,
        Align2::LEFT_TOP,
        label,
        bold_font(12.0),
        theme::TEXT_LABEL,
        2.4,
    )
}

/// A dialog button placed in `rect`.
pub fn button_at(
    ui: &mut Ui,
    art: &Art,
    rect: Rect,
    label: &str,
    icon: Option<&str>,
    style: ButtonStyle,
    enabled: bool,
) -> Response {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    button(&mut child, art, label, icon, style, rect.size(), enabled)
}
