//! Painting the page's CSS looks with egui shapes: vertical and horizontal gradients,
//! the armour plates (top-left and bottom-right corners cut, a steel hairline round the
//! cut), spaced type, key caps, timer rings, vignettes and the dialogs' buttons.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align2, Color32, FontId, Galley, Mesh, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2, pos2, vec2,
};

use sloppy_web::hud::BOT_TAG;

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
/// laid over a rounded fill leaves bands where the two differ near the corners. Its
/// rounded corners fade out over a physical pixel, as egui feathers its own fills, where
/// unfeathered triangles leave stairs; its level and upright edges stay crisp, as
/// [`gradient_polygon`]'s do, so rects that share one (the HUD's plates and clock) leave
/// no seam between them.
pub fn rounded_vgradient(
    painter: &Painter,
    rect: Rect,
    radius: [f32; 4],
    stops: &[(f32, Color32)],
) {
    let feather = 1.0 / painter.pixels_per_point();
    painter.add(rounded_vgradient_mesh(rect, radius, stops, feather));
}

/// The mesh of [`rounded_vgradient`], its corners faded out over `feather` points.
fn rounded_vgradient_mesh(
    rect: Rect,
    radius: [f32; 4],
    stops: &[(f32, Color32)],
    feather: f32,
) -> Mesh {
    // Rows per corner, turning the arc by equal angles so its chords stay short where
    // it meets a level edge.
    const ARC_STEPS: usize = 12;
    // A corner too small to fade inside itself is drawn square.
    let [nw, ne, sw, se] = radius.map(|r| {
        let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0);
        if r > feather { r } else { 0.0 }
    });
    // How far a corner of radius `r` pulls the edge in at `depth` into its band.
    let inset = |r: f32, depth: f32| {
        if depth >= r {
            0.0
        } else {
            r - (r * r - (r - depth) * (r - depth)).max(0.0).sqrt()
        }
    };
    let color_at = |y: f32| {
        let share = (y - rect.top()) / rect.height().max(f32::EPSILON);
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
    for step in 0..=ARC_STEPS {
        let angle = std::f32::consts::FRAC_PI_2 * step as f32 / ARC_STEPS as f32;
        let depth = 1.0 - angle.cos();
        ys.extend([nw, ne].map(|r| rect.top() + r * depth));
        ys.extend([sw, se].map(|r| rect.bottom() - r * depth));
    }
    ys.retain(|y| (rect.top()..=rect.bottom()).contains(y));
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    let rows: Vec<(Pos2, Pos2)> = ys
        .iter()
        .map(|&y| {
            let from_top = y - rect.top();
            let from_bottom = rect.bottom() - y;
            let left = rect.left() + inset(nw, from_top).max(inset(sw, from_bottom));
            let right = rect.right() - inset(ne, from_top).max(inset(se, from_bottom));
            (pos2(left, y), pos2(right, y))
        })
        .collect();
    // The outline runs down the left side and back up the right.
    let outline: Vec<Pos2> = rows
        .iter()
        .map(|row| row.0)
        .chain(rows.iter().rev().map(|row| row.1))
        .collect();
    let shifts = curve_feather(&outline, feather);
    let count = rows.len();
    let mut mesh = Mesh::default();
    // Each row as its inner left and right vertices in the gradient's colour, then the
    // two faded out beyond them.
    for (index, &(left, right)) in rows.iter().enumerate() {
        let left_shift = shifts[index];
        let right_shift = shifts[2 * count - 1 - index];
        let inner_left = left - left_shift;
        let inner_right = right - right_shift;
        mesh.colored_vertex(inner_left, color_at(inner_left.y));
        mesh.colored_vertex(inner_right, color_at(inner_right.y));
        mesh.colored_vertex(left + left_shift, Color32::TRANSPARENT);
        mesh.colored_vertex(right + right_shift, Color32::TRANSPARENT);
    }
    for index in 1..count as u32 {
        let (above, below) = (4 * (index - 1), 4 * index);
        mesh.add_triangle(above, above + 1, below);
        mesh.add_triangle(above + 1, below + 1, below);
        // The fades down the left side (inner, outer) and the right.
        mesh.add_triangle(above, above + 2, below + 2);
        mesh.add_triangle(above, below + 2, below);
        mesh.add_triangle(above + 1, below + 1, below + 3);
        mesh.add_triangle(above + 1, below + 3, above + 3);
    }
    // Across the top and bottom rows, where a pill's arcs meet.
    let last = 4 * (count as u32).saturating_sub(1);
    for row in [0, last] {
        mesh.add_triangle(row, row + 2, row + 3);
        mesh.add_triangle(row, row + 3, row + 1);
    }
    mesh
}

/// How far each point of a closed, convex, smooth `outline` moves out for its outer
/// fade (and in for its solid edge): half of `feather` along its normal where the
/// outline curves (both its edges slanted), as egui feathers a path, and nothing on a
/// level or upright edge or where one meets a curve, so those stay crisp.
fn curve_feather(outline: &[Pos2], feather: f32) -> Vec<Vec2> {
    let count = outline.len();
    let clockwise = (0..count)
        .map(|index| {
            let a = outline[index] - outline[0];
            let b = outline[(index + 1) % count] - outline[0];
            a.x * b.y - b.x * a.y
        })
        .sum::<f32>()
        >= 0.0;
    // The edge from `index` to the next point that differs from it, `step` (1 or
    // `count - 1`) points at a time: a pill's level edge has no length.
    let edge = |index: usize, step: usize| {
        (1..count)
            .map(|offset| outline[(index + offset * step) % count] - outline[index])
            .find(|along| along.length_sq() > 1e-8)
            .unwrap_or(Vec2::ZERO)
    };
    let outward = |along: Vec2| {
        let normal = vec2(along.y, -along.x).normalized();
        if clockwise { normal } else { -normal }
    };
    let slanted = |along: Vec2| along.x.abs() > 0.01 && along.y.abs() > 0.01;
    (0..count)
        .map(|index| {
            let after = edge(index, 1);
            let before = -edge(index, count - 1);
            if !slanted(before) || !slanted(after) {
                return Vec2::ZERO;
            }
            // A miter, as egui's paths have: the edges' mean normal, lengthened so
            // each edge's fade keeps its width.
            let normal = (outward(before) + outward(after)) / 2.0;
            normal / normal.length_sq().max(0.5) * (feather / 2.0)
        })
        .collect()
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

/// How wide a slanted edge of [`gradient_polygon`] fades out, centred on the edge: a
/// physical pixel on the 2× screens most players have, as egui feathers its own shapes.
const EDGE_FEATHER: f32 = 0.5;

/// A convex polygon filled with a vertical gradient from `top` to `bottom` of its
/// bounds (a fan from its centre, which interpolates a vertical ramp exactly). Its
/// slanted edges fade out over [`EDGE_FEATHER`], as a browser smooths a `clip-path`,
/// where unfeathered triangles leave stairs; its level and upright edges stay crisp, so
/// shapes that share one (a plate and its band, the HUD's plates and clock) leave no
/// seam between them.
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
    let count = points.len();
    if count < 3 {
        return Shape::Noop;
    }
    // Each edge's outward normal and how far its fade reaches each way.
    let clockwise = (0..count)
        .map(|index| {
            let a = points[index] - points[0];
            let b = points[(index + 1) % count] - points[0];
            a.x * b.y - b.x * a.y
        })
        .sum::<f32>()
        >= 0.0;
    let edges: Vec<(Vec2, f32)> = (0..count)
        .map(|index| {
            let along = points[(index + 1) % count] - points[index];
            let normal = vec2(along.y, -along.x).normalized();
            let normal = if clockwise { normal } else { -normal };
            let slanted = along.x.abs() > 0.01 && along.y.abs() > 0.01;
            (normal, if slanted { EDGE_FEATHER / 2.0 } else { 0.0 })
        })
        .collect();
    // A corner moves to where its two edges' offset lines cross, so the fade keeps
    // each edge's own width and a crisp edge stays put.
    let offset = |index: usize| {
        let (before, reach_before) = edges[(index + count - 1) % count];
        let (after, reach_after) = edges[index];
        let determinant = before.x * after.y - before.y * after.x;
        if determinant.abs() < 1e-3 {
            return before * reach_before.max(reach_after);
        }
        let shift = vec2(
            (reach_before * after.y - reach_after * before.y) / determinant,
            (before.x * reach_after - after.x * reach_before) / determinant,
        );
        // A sharp corner's crossing runs far out; keep it near the corner.
        let most = 4.0 * EDGE_FEATHER;
        if shift.length() > most {
            shift.normalized() * most
        } else {
            shift
        }
    };
    let mut mesh = Mesh::default();
    let center = points
        .iter()
        .fold(Vec2::ZERO, |sum, point| sum + point.to_vec2())
        / count.max(1) as f32;
    mesh.colored_vertex(center.to_pos2(), color(center.y));
    // Each corner as an inner vertex in the gradient's colour and an outer one faded out.
    for (index, point) in points.iter().enumerate() {
        let shift = offset(index);
        let inner = *point - shift;
        mesh.colored_vertex(inner, color(inner.y));
        mesh.colored_vertex(*point + shift, Color32::TRANSPARENT);
    }
    let count = count as u32;
    for index in 0..count {
        let next = (index + 1) % count;
        let (inner, outer) = (1 + 2 * index, 2 + 2 * index);
        let (next_inner, next_outer) = (1 + 2 * next, 2 + 2 * next);
        mesh.add_triangle(0, inner, next_inner);
        mesh.add_triangle(inner, outer, next_outer);
        mesh.add_triangle(inner, next_outer, next_inner);
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
    rounded_vgradient(painter, rect, [radius; 4], &[(0.0, top), (1.0, bottom)]);
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

/// The width to wrap `text` at in a box `width` wide: as a page's text, never narrower
/// than its longest word, which overflows rather than breaks.
pub fn wrap_width(painter: &Painter, text: &str, font: FontId, width: f32) -> f32 {
    text.split_whitespace()
        .map(|word| text_width(painter, word, font.clone(), 0.0))
        .fold(width, f32::max)
}

/// The width `text` takes.
pub fn text_width(painter: &Painter, text: &str, font: FontId, spacing: f32) -> f32 {
    galley(painter, text, font, Color32::WHITE, spacing, f32::INFINITY)
        .size()
        .x
}

/// The room on each side of a bot tag's letters, and between a name and its tag.
const BOT_TAG_PADDING: f32 = 4.0;
pub const BOT_TAG_GAP: f32 = 5.0;

/// The width of the tag beside a room's bot's name ([`bot_tag`]) in `size` point type.
pub fn bot_tag_width(painter: &Painter, size: f32) -> f32 {
    text_width(painter, BOT_TAG, bold_font(size), 0.8) + 2.0 * BOT_TAG_PADDING
}

/// The muted chip beside a room's bot's name (`BOT`, from the engine): grey capitals
/// on a faint plate, no team colour, with its left end at `left_center`, `alpha`
/// fading it with its row. Returns its rect.
pub fn bot_tag(painter: &Painter, left_center: Pos2, size: f32, alpha: f32) -> Rect {
    let rect = Rect::from_min_size(
        pos2(left_center.x, left_center.y - (size + 5.0) / 2.0),
        vec2(bot_tag_width(painter, size), size + 5.0),
    );
    painter.rect_filled(rect, 3, hexa(0xc9d6e2, 0.14 * alpha));
    text(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        BOT_TAG,
        bold_font(size),
        hexa(0xb4c2cf, alpha),
        0.8,
    );
    rect
}

/// A key cap (`kbd`) centred at `center`; returns its rect.
pub fn kbd(painter: &Painter, center: Pos2, label: &str, size: f32) -> Rect {
    let width = kbd_width(painter, label, size);
    let rect = Rect::from_center_size(center, vec2(width, size * 1.3 + 10.0));
    kbd_in(painter, rect, label, size);
    rect
}

/// A key cap filling `rect`, for a key smaller than `kbd`'s (the first-person dial's).
pub fn kbd_in(painter: &Painter, rect: Rect, label: &str, size: f32) {
    let font = bold_font(size);
    painter.rect_filled(rect.translate(vec2(0.0, 1.0)), 4, hex(0x050c18));
    rounded_vgradient(
        painter,
        rect.shrink(0.5),
        [4.0; 4],
        &[(0.0, hex(0x3d659e)), (1.0, hex(0x1f3b66))],
    );
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

/// A painter on `layer` over the whole screen: egui's own clip to the safe area, so on
/// an iPad a fill would stop short of the home indicator's strip.
pub fn full_screen(context: &egui::Context, layer: egui::LayerId) -> Painter {
    let mut painter = context.layer_painter(layer);
    painter.set_clip_rect(context.viewport_rect());
    painter
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

/// Name a painted button for assistive technology and the page's checks, which find
/// it by `label` in egui's AccessKit tree (`automation.rs`), and show the hand over it
/// while it can be clicked.
pub fn labelled(response: &Response, enabled: bool, label: &str) {
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Button, enabled, label));
    point_at(response, enabled);
}

/// A card, tab or switch that is on or off, named like [`labelled`].
pub fn labelled_choice(response: &Response, enabled: bool, selected: bool, label: &str) {
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::accesskit::Role::Button, enabled, selected, label)
    });
    point_at(response, enabled);
}

/// The hand over a widget the pointer can click (`button { cursor: pointer }`); a
/// caller may still show another cursor after (a locked card's).
fn point_at(response: &Response, enabled: bool) {
    if enabled && response.sense.senses_click() && response.hovered() {
        response.ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

/// The type on a button: a dialog's bold capitals (`.primary`, 15 px with 1.5 px between
/// letters), or a Battle Setup action's display face at its own size and spacing
/// (`.start #start` 40 px by 4, `.room-action .primary` 26 px by 2).
#[derive(Clone, Copy, PartialEq)]
pub enum Lettering {
    Dialog,
    Display { size: f32, spacing: f32 },
}

impl Lettering {
    /// The face and the room between letters.
    fn font(self) -> (FontId, f32) {
        match self {
            Lettering::Dialog => (bold_font(15.0), 1.5),
            Lettering::Display { size, spacing } => (display_font(size), spacing),
        }
    }
}

/// A button's leading icon (`.action-icon`) and the room after it.
const BUTTON_ICON: f32 = 16.0;
const BUTTON_ICON_GAP: f32 = 8.0;

/// The width a dialog button's label takes, its icon (`icon`) included: a button sized
/// to its content adds its padding to this.
pub fn button_content_width(painter: &Painter, label: &str, icon: bool) -> f32 {
    content_width(painter, label, icon, Lettering::Dialog)
}

fn content_width(painter: &Painter, label: &str, icon: bool, lettering: Lettering) -> f32 {
    let (font, spacing) = lettering.font();
    let icon_width = if icon {
        BUTTON_ICON + BUTTON_ICON_GAP
    } else {
        0.0
    };
    text_width(painter, label, font, spacing) + icon_width
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
    lettered_button(
        ui,
        art,
        label,
        icon,
        style,
        size,
        enabled,
        Lettering::Dialog,
    )
}

/// A gold action filling `rect` with its label in `lettering` (GO, CREATE ROOM).
pub fn action_button(
    ui: &mut Ui,
    art: &Art,
    rect: Rect,
    label: &str,
    lettering: Lettering,
    enabled: bool,
) -> Response {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    lettered_button(
        &mut child,
        art,
        label,
        None,
        ButtonStyle::Primary,
        rect.size(),
        enabled,
        lettering,
    )
}

#[allow(clippy::too_many_arguments)]
fn lettered_button(
    ui: &mut Ui,
    art: &Art,
    label: &str,
    icon: Option<&str>,
    style: ButtonStyle,
    size: Vec2,
    enabled: bool,
    lettering: Lettering,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    labelled(&response, enabled, label);
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
    let glint = (style == ButtonStyle::Primary).then(|| glint_progress(ui, &response, hovered));
    let painter = ui.painter();
    let ink = match style {
        ButtonStyle::Primary => brighten(hex(0x1a2233), hovered),
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
            let glow = if pressed {
                PRIMARY_GLOW_PRESSED
            } else if hovered {
                PRIMARY_GLOW_HOVER
            } else {
                PRIMARY_GLOW
            };
            painter.add(glow.as_shape(rect, 10));
            let (top, middle, bottom) = if enabled {
                (hex(0xffe46e), GOLD, hex(0xf4a91a))
            } else {
                (hex(0x9a8a50), hex(0x8c7a3c), hex(0x6e5a24))
            };
            let [top, middle, bottom, ledge] =
                [top, middle, bottom, hex(0xc97d0e)].map(|color| brighten(color, hovered));
            painter.rect_filled(rect, 10, bottom);
            // `inset 0 1px #fff9` over the top and `inset 0 -3px #c97d0e` (1 px pressed)
            // under the foot, each meeting the gradient over half a pixel.
            let height = rect.height().max(1.0);
            let highlight = mix(top, Color32::WHITE, 0.6);
            let ledge_top = 1.0 - if pressed { 1.0 } else { 3.0 } / height;
            let edge = 0.5 / height;
            rounded_vgradient(
                painter,
                rect,
                [10.0; 4],
                &[
                    (0.0, highlight),
                    (1.0 / height, highlight),
                    (1.0 / height + edge, top),
                    (0.48, middle),
                    (ledge_top - edge, bottom),
                    (ledge_top, ledge),
                    (1.0, ledge),
                ],
            );
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
    let (font, spacing) = lettering.font();
    let total = content_width(painter, label, icon.is_some(), lettering);
    let mut x = rect.center().x - total / 2.0;
    if let Some(icon) = icon {
        art.icon(
            painter,
            icon,
            Rect::from_center_size(
                pos2(x + BUTTON_ICON / 2.0, rect.center().y),
                Vec2::splat(BUTTON_ICON),
            ),
            ink,
        );
        x += BUTTON_ICON + BUTTON_ICON_GAP;
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
    if let Some(Some(progress)) = glint {
        painter.add(glint_shape(rect, 10.0, progress));
    }
    response
}

/// The gold button's warm glow (`.primary`'s outer `box-shadow`): at rest, under the
/// pointer and pressed. egui fades a blur linearly, so each is half again the CSS blur.
const PRIMARY_GLOW: egui::epaint::Shadow = egui::epaint::Shadow {
    offset: [0, 8],
    blur: 36,
    spread: 0,
    color: Color32::from_rgba_unmultiplied_const(0xff, 0xb2, 0x1c, 0x33),
};
const PRIMARY_GLOW_HOVER: egui::epaint::Shadow = egui::epaint::Shadow {
    offset: [0, 10],
    blur: 45,
    spread: 0,
    color: Color32::from_rgba_unmultiplied_const(0xff, 0xb2, 0x1c, 0x66),
};
const PRIMARY_GLOW_PRESSED: egui::epaint::Shadow = egui::epaint::Shadow {
    offset: [0, 4],
    blur: 18,
    spread: 0,
    color: Color32::from_rgba_unmultiplied_const(0xff, 0xb2, 0x1c, 0x44),
};
/// The gold under the pointer (`filter: brightness(1.07)`).
const PRIMARY_HOVER_BRIGHTNESS: f32 = 1.07;
/// The glint's one sweep across a gold button the pointer came onto (`.primary::after`,
/// `transition: transform 0.6s ease`).
const GLINT_SECONDS: f64 = 0.6;
/// The glint band: its width and its start and end left edges, as shares of the
/// button's width (`left: -40%; width: 30%`, moved from `-120%` to `560%` of itself),
/// its lean (`skewX(-20deg)`) and its brightest white (`#ffffff8c`).
const GLINT_WIDTH: f32 = 0.3;
const GLINT_FROM: f32 = -0.4 - 1.2 * GLINT_WIDTH;
const GLINT_TO: f32 = -0.4 + 5.6 * GLINT_WIDTH;
const GLINT_LEAN: f32 = 0.364;
const GLINT_PEAK: f32 = 0x8c as f32 / 255.0;

/// `color` brightened like the gold under the pointer, when `hovered`.
fn brighten(color: Color32, hovered: bool) -> Color32 {
    if !hovered {
        return color;
    }
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let up = |channel: u8| (f32::from(channel) * PRIMARY_HOVER_BRIGHTNESS).min(255.0) as u8;
    Color32::from_rgba_unmultiplied(up(r), up(g), up(b), a)
}

/// How far the glint has crossed a gold button (0 to 1) since the pointer came onto it,
/// while it is crossing; `None` once it has gone or off the button. The sweep runs once
/// per arrival, at one fixed rate.
fn glint_progress(ui: &Ui, response: &Response, hovered: bool) -> Option<f32> {
    let id = response.id.with("glint");
    let context = ui.ctx();
    if !hovered {
        context.data_mut(|data| data.remove::<f64>(id));
        return None;
    }
    let now = ui.input(|input| input.time);
    let start = context.data_mut(|data| *data.get_temp_mut_or(id, now));
    let progress = (now - start) / GLINT_SECONDS;
    if progress >= 1.0 {
        return None;
    }
    context.request_repaint();
    Some(egui::emath::easing::quadratic_out(progress as f32))
}

/// The glint `progress` of the way across `rect` (corners rounded by `radius`): a band of
/// white leaning right, brightest at its middle, drawn in thin columns cut to the
/// rounded face as `overflow: hidden` cuts it.
fn glint_shape(rect: Rect, radius: f32, progress: f32) -> Shape {
    const COLUMN: f32 = 2.0;
    let width = rect.width();
    let band = width * GLINT_WIDTH;
    let left = width * (GLINT_FROM + (GLINT_TO - GLINT_FROM) * progress);
    let lean = GLINT_LEAN * rect.height() / 2.0;
    // The band's white at `x` across the unleant band (0 to `band` from its left edge).
    let white = |x: f32| {
        let share = (x - left) / band;
        if (0.0..=1.0).contains(&share) {
            GLINT_PEAK * (1.0 - (share * 2.0 - 1.0).abs())
        } else {
            0.0
        }
    };
    let radius = radius.min(width / 2.0).min(rect.height() / 2.0);
    let inset = |depth: f32| {
        if depth >= radius {
            0.0
        } else {
            radius
                - (radius * radius - (radius - depth) * (radius - depth))
                    .max(0.0)
                    .sqrt()
        }
    };
    let from = (left - lean).max(0.0);
    let to = (left + band + lean).min(width);
    let mut mesh = Mesh::default();
    let mut x = from;
    while x < to + COLUMN {
        let x_clamped = x.min(to);
        let cut = inset(x_clamped.min(width - x_clamped));
        // The band leans right, so its top row is the band's unleant self further left.
        let top = pos2(rect.left() + x_clamped, rect.top() + cut);
        let bottom = pos2(rect.left() + x_clamped, rect.bottom() - cut);
        let lean_at = |y: f32| GLINT_LEAN * (y - rect.center().y);
        mesh.colored_vertex(top, fade(Color32::WHITE, white(x_clamped + lean_at(top.y))));
        mesh.colored_vertex(
            bottom,
            fade(Color32::WHITE, white(x_clamped + lean_at(bottom.y))),
        );
        let count = mesh.vertices.len() as u32;
        if count >= 4 {
            mesh.add_triangle(count - 4, count - 3, count - 2);
            mesh.add_triangle(count - 3, count - 1, count - 2);
        }
        if x_clamped >= to {
            break;
        }
        x += COLUMN;
    }
    Shape::mesh(mesh)
}

/// The dialogs' corner X (`.dialog-close`): a small steel key, a faint fill in a
/// hairline ring, that lights up under the pointer.
pub fn close_button(ui: &mut Ui, art: &Art, rect: Rect, id: egui::Id) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    labelled(&response, true, "Close");
    let painter = ui.painter();
    let hovered = response.hovered();
    painter.rect_filled(rect, 7, hexa(0xffffff, if hovered { 0.13 } else { 0.06 }));
    painter.rect_stroke(
        rect,
        7,
        Stroke::new(
            1.0,
            if hovered {
                hex(0xcfe8fa)
            } else {
                hexa(0x9fd0ff, 0.2)
            },
        ),
        StrokeKind::Inside,
    );
    let tint = if hovered {
        Color32::WHITE
    } else {
        hex(0xc9dcee)
    };
    art.icon(
        painter,
        "close",
        Rect::from_center_size(rect.center(), Vec2::splat(18.0)),
        tint,
    );
    response
}

/// What a layer drawn smaller to fit a small screen (a phone) keeps of its shapes: the
/// screen as that layer sees it, at down to a third of its size.
pub fn shrunk_clip(screen: Rect) -> Rect {
    screen.expand2(screen.size() * 3.0)
}

/// Draw `layer` at `scale` about `anchor`, which stays put: a part of the interface
/// shrinks to fit a small screen rather than run off it. Its widgets still click.
pub fn shrink_about(context: &egui::Context, layer: egui::LayerId, anchor: Pos2, scale: f32) {
    context.set_transform_layer(
        layer,
        egui::emath::TSTransform::new(anchor.to_vec2() * (1.0 - scale), scale),
    );
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
