//! Painting the page's CSS looks with egui shapes: vertical and horizontal gradients,
//! the armour plates (top-left and bottom-right corners cut, a steel hairline round the
//! cut), spaced type, key caps, timer rings, vignettes and the dialogs' buttons.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use egui::scroll_area::{DragScroll, ScrollSource};
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Align2, Color32, FontId, Galley, Id, Mesh, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2, pos2, vec2,
};

use sloppy_web::hud::BOT_TAG;

use super::icons::Art;
use super::theme::{self, GOLD, bold_font, display_font};

/// A vertical scroll area that a finger drags from the first touch on a touch screen.
/// egui's default drag (`DragScroll::OnTouch`) waits for egui's first touch event, and
/// that first press is hit-tested against the frame before it, which had no drag area
/// yet. A phone's first drag of Battle Setup did nothing until the finger tried again.
pub fn vertical_scroll() -> egui::ScrollArea {
    let drag = if crate::touch::screen() {
        DragScroll::Always
    } else {
        DragScroll::OnTouch
    };
    egui::ScrollArea::vertical().scroll_source(ScrollSource {
        drag,
        ..ScrollSource::default()
    })
}

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

/// Meshes the screens build every frame from the same inputs (a stick's lit knob, a key
/// cap's rounded gradient), each by the inputs it was built from and the last pass that
/// drew it. A builder's mesh depends on its inputs alone, so a kept one is the mesh it
/// would build again.
#[derive(Default)]
struct MeshMemo {
    pass: u64,
    meshes: HashMap<Box<[u32]>, (Arc<Mesh>, u64)>,
    /// The key being looked up, kept to look up the next without allocating.
    key: Vec<u32>,
}

thread_local! {
    static MESHES: RefCell<MeshMemo> = RefCell::default();
}

/// A kind of mesh in [`memo_mesh`]'s keys, so two builders' inputs never meet.
#[derive(Clone, Copy)]
pub enum MeshKind {
    RoundedGradient = 1,
    Vignette,
    LitDisc,
    SetupSky,
    SetupLand,
    GradientPolygon,
    LoadedAmmoFace,
    LoadedAmmoGlow,
    VerticalGradient,
    HorizontalGradient,
    Radial,
    InnerEdge,
    Ring,
    Infinity,
    Ray,
}

/// The words of a mesh's key: a builder's float inputs by their bits and colours by
/// their bytes.
pub fn key_floats(values: impl IntoIterator<Item = f32>) -> impl Iterator<Item = u32> {
    values.into_iter().map(f32::to_bits)
}

pub fn key_stops(stops: &[(f32, Color32)]) -> impl Iterator<Item = u32> + '_ {
    stops
        .iter()
        .flat_map(|&(share, color)| [share.to_bits(), u32::from_le_bytes(color.to_array())])
}

/// The mesh `build` makes from the inputs `key` holds, built only when no pass drew it
/// last time: a mesh not drawn in a pass is let go at the next. `build` must not draw
/// kept meshes itself.
pub fn memo_mesh(
    context: &egui::Context,
    kind: MeshKind,
    key: impl IntoIterator<Item = u32>,
    build: impl FnOnce() -> Mesh,
) -> Shape {
    let pass = context.cumulative_pass_nr();
    let kept = MESHES.with_borrow_mut(|memo| {
        if memo.pass != pass {
            let last = memo.pass;
            memo.meshes.retain(|_, (_, used)| *used == last);
            memo.pass = pass;
        }
        memo.key.clear();
        memo.key.push(kind as u32);
        memo.key.extend(key);
        let (meshes, key) = (&mut memo.meshes, &memo.key);
        meshes.get_mut(key.as_slice()).map(|(mesh, used)| {
            *used = pass;
            Arc::clone(mesh)
        })
    });
    let mesh = kept.unwrap_or_else(|| {
        let mesh = Arc::new(build());
        MESHES.with_borrow_mut(|memo| {
            let key = memo.key.as_slice().into();
            memo.meshes.insert(key, (Arc::clone(&mesh), pass));
        });
        mesh
    });
    Shape::Mesh(mesh)
}

/// The meshes of the last pass's shapes, for [`frame_mesh`] to fill again: what
/// [`memo_mesh`] cannot keep, because its inputs change from frame to frame (a fading
/// wash, a rotating wedge, a held stick's knob) or because a key and a lookup would
/// cost more than building it (an icon's four vertices). Built from nothing, each was
/// three allocations a frame: its vertices, its indices and the shape's `Arc`.
///
/// A shape owns its mesh alone while its pass is laid out, so egui moves a mesh on a
/// zoomed or shrunk layer where it is (a mesh the pool still held was cloned to be
/// moved: three allocations again, and a copy). A frame loop runs its passes through
/// [`run_pass`], which keeps a finished pass's meshes before they are tessellated: each
/// is free once egui has dropped its shape.
#[derive(Default)]
struct MeshPool {
    /// The last pass's meshes, the first drawn on top: handed out in the order they
    /// were drawn, a screen that draws the same shapes each frame meets each in the
    /// mesh it filled before, already the right size.
    spare: Vec<Arc<Mesh>>,
    /// The layers with a transform, listed afresh each pass in a kept buffer.
    transformed: Vec<egui::LayerId>,
}

/// The most meshes the pool keeps, several times what the busiest screen draws.
const POOLED_MESHES: usize = 256;

/// The largest mesh the pool keeps, in vertices: an icon has 4 and a radial gradient a
/// few hundred. A larger one (a backdrop) is let go, not held to hand to an icon.
const POOLED_VERTICES: usize = 2048;

impl MeshPool {
    /// A mesh nothing else holds, empty and untextured, with the buffers it had.
    fn take(&mut self) -> Arc<Mesh> {
        while let Some(mut mesh) = self.spare.pop() {
            if let Some(free) = Arc::get_mut(&mut mesh) {
                // Not `Mesh::clear`, which gives the vertices' buffer back.
                free.indices.clear();
                free.vertices.clear();
                free.texture_id = egui::TextureId::default();
                return mesh;
            }
            // Still drawn (a pass kept untessellated, a shape someone holds): let go.
        }
        Arc::default()
    }

    /// Keep the meshes `shapes` alone hold, in place of those kept before: a mesh
    /// [`memo_mesh`] keeps or several shapes share is not the pool's to fill.
    fn keep(&mut self, shapes: &[egui::epaint::ClippedShape]) {
        self.spare.clear();
        for clipped in shapes.iter().rev() {
            self.keep_shape(&clipped.shape);
        }
    }

    fn keep_shape(&mut self, shape: &Shape) {
        match shape {
            Shape::Vec(shapes) => {
                for shape in shapes.iter().rev() {
                    self.keep_shape(shape);
                }
            }
            Shape::Mesh(mesh)
                if self.spare.len() < POOLED_MESHES
                    && Arc::strong_count(mesh) == 1
                    && Arc::weak_count(mesh) == 0
                    && mesh.vertices.capacity() <= POOLED_VERTICES =>
            {
                self.spare.push(Arc::clone(mesh));
            }
            _ => {}
        }
    }
}

thread_local! {
    static POOL: RefCell<MeshPool> = RefCell::default();
}

/// The shape of the mesh `fill` builds, in a mesh kept from the last pass when one is
/// free (see [`MeshPool`]). `fill` starts from an empty untextured mesh.
pub fn frame_mesh(fill: impl FnOnce(&mut Mesh)) -> Shape {
    let mut mesh = POOL.with_borrow_mut(MeshPool::take);
    fill(Arc::get_mut(&mut mesh).expect("a taken mesh is held by nothing else"));
    Shape::Mesh(mesh)
}

/// A pass of `build` for a frame loop to tessellate, as `Context::run_pass` runs one,
/// with the pool looked after: kept meshes on transformed layers are drawn from copies
/// ([`own_transformed_meshes`]) and the pass's meshes are kept for the next
/// ([`MeshPool`]). A loop that calls `Context::run_pass` itself only goes without.
pub fn run_pass(
    context: &egui::Context,
    input: egui::RawInput,
    mut build: impl FnMut(&egui::Context),
) -> egui::FullOutput {
    let output = context.run_pass(input, |context| {
        build(context);
        own_transformed_meshes(context);
    });
    POOL.with_borrow_mut(|pool| pool.keep(&output.shapes));
    output
}

/// As a pass ends, give every shape on a transformed layer (a zoomed HUD, Battle Setup
/// fitted to its window) a mesh it holds alone. egui moves such a layer's shapes where
/// they are, and clones a mesh something else holds to do it, as [`memo_mesh`] holds
/// every one it draws: three allocations a frame, and the copy. Here the copy goes
/// into a pooled mesh. Other layers' shapes keep drawing the kept mesh itself.
fn own_transformed_meshes(context: &egui::Context) {
    let mut layers = POOL.with_borrow_mut(|pool| std::mem::take(&mut pool.transformed));
    layers.clear();
    // egui keeps a layer's transform until it is set again, so some are not drawn now.
    context.memory(|memory| layers.extend(memory.to_global.keys().copied()));
    if !layers.is_empty() {
        context.graphics_mut(|graphics| {
            for &layer in &layers {
                let Some(shapes) = graphics.get_mut(layer) else {
                    continue;
                };
                for index in 0..shapes.next_idx().0 {
                    shapes.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                        // egui moves such a shape after tessellating it, not before.
                        if clipped.transform_after_tessellation
                            == egui::emath::TSTransform::IDENTITY
                        {
                            own_meshes(&mut clipped.shape);
                        }
                    });
                }
            }
        });
    }
    POOL.with_borrow_mut(|pool| pool.transformed = layers);
}

/// Replace each mesh of `shape` that something else holds with a pooled copy; one too
/// large for the pool stays, for egui to clone.
fn own_meshes(shape: &mut Shape) {
    match shape {
        Shape::Vec(shapes) => shapes.iter_mut().for_each(own_meshes),
        Shape::Mesh(kept)
            if (Arc::strong_count(kept) > 1 || Arc::weak_count(kept) > 0)
                && kept.vertices.len() <= POOLED_VERTICES =>
        {
            let copy = frame_mesh(|copy| {
                copy.texture_id = kept.texture_id;
                copy.indices.extend_from_slice(&kept.indices);
                copy.vertices.extend_from_slice(&kept.vertices);
            });
            *shape = copy;
        }
        _ => {}
    }
}

/// A textured rectangle, as `Painter::image` draws one, in a pooled mesh.
pub fn image(texture: egui::TextureId, rect: Rect, uv: Rect, tint: Color32) -> Shape {
    frame_mesh(|mesh| {
        mesh.texture_id = texture;
        mesh.add_rect_with_uv(rect, uv, tint);
    })
}

/// `shape` moved by `offset`. A mesh [`memo_mesh`] keeps is copied into a pooled one,
/// where `Shape::translate` would clone it to move it; one the shape holds alone moves
/// where it is.
pub fn moved(mut shape: Shape, offset: Vec2) -> Shape {
    if let Shape::Mesh(kept) = &shape
        && Arc::strong_count(kept) > 1
    {
        return frame_mesh(|mesh| {
            // As `Shape::translate` moves a mesh's vertices.
            let transform = egui::emath::TSTransform::from_translation(offset);
            mesh.texture_id = kept.texture_id;
            mesh.indices.extend_from_slice(&kept.indices);
            mesh.vertices
                .extend(kept.vertices.iter().map(|vertex| egui::epaint::Vertex {
                    pos: transform * vertex.pos,
                    ..*vertex
                }));
        });
    }
    shape.translate(offset);
    shape
}

/// An untextured mesh with room for `vertices` and `triangles`, so building it never
/// grows its buffers.
pub fn mesh_with_capacity(vertices: usize, triangles: usize) -> Mesh {
    let mut mesh = Mesh::default();
    reserve(&mut mesh, vertices, triangles);
    mesh
}

/// Room in `mesh` for `vertices` and `triangles` more.
pub fn reserve(mesh: &mut Mesh, vertices: usize, triangles: usize) {
    mesh.reserve_vertices(vertices);
    mesh.reserve_triangles(triangles);
}

/// A vertical gradient over `rect` through `stops` (share of the height, colour).
pub fn vgradient(context: &egui::Context, rect: Rect, stops: &[(f32, Color32)]) -> Shape {
    let key = key_floats([rect.min.x, rect.min.y, rect.max.x, rect.max.y]).chain(key_stops(stops));
    memo_mesh(context, MeshKind::VerticalGradient, key, || {
        vgradient_mesh(rect, stops)
    })
}

/// The mesh of [`vgradient`].
pub fn vgradient_mesh(rect: Rect, stops: &[(f32, Color32)]) -> Mesh {
    let mut mesh = mesh_with_capacity(2 * stops.len(), 2 * stops.len().saturating_sub(1));
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
    mesh
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
    let key = key_floats([rect.min.x, rect.min.y, rect.max.x, rect.max.y, feather])
        .chain(key_floats(radius))
        .chain(key_stops(stops));
    painter.add(memo_mesh(
        painter.ctx(),
        MeshKind::RoundedGradient,
        key,
        || rounded_vgradient_mesh(rect, radius, stops, feather),
    ));
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
    let mut ys: Vec<f32> = Vec::with_capacity(stops.len() + 4 * (ARC_STEPS + 1));
    ys.extend(
        stops
            .iter()
            .map(|(share, _)| rect.top() + rect.height() * share),
    );
    // Two corners of one radius along an edge give the same rows, which the dedup below
    // would drop anyway: leave them out before sorting.
    let top = if nw == ne { &[nw][..] } else { &[nw, ne][..] };
    let bottom = if sw == se { &[sw][..] } else { &[sw, se][..] };
    for step in 0..=ARC_STEPS {
        let angle = std::f32::consts::FRAC_PI_2 * step as f32 / ARC_STEPS as f32;
        let depth = 1.0 - angle.cos();
        ys.extend(top.iter().map(|r| rect.top() + r * depth));
        ys.extend(bottom.iter().map(|r| rect.bottom() - r * depth));
    }
    ys.retain(|y| (rect.top()..=rect.bottom()).contains(y));
    // Values equal in this order are the same bits, so an unstable sort leaves them as
    // a stable one would, without its scratch buffer.
    ys.sort_unstable_by(f32::total_cmp);
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
    let mut mesh = mesh_with_capacity(4 * count, 6 * count.saturating_sub(1) + 4);
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
pub fn hgradient(context: &egui::Context, rect: Rect, stops: &[(f32, Color32)]) -> Shape {
    let key = key_floats([rect.min.x, rect.min.y, rect.max.x, rect.max.y]).chain(key_stops(stops));
    memo_mesh(context, MeshKind::HorizontalGradient, key, || {
        hgradient_mesh(rect, stops)
    })
}

/// The mesh of [`hgradient`].
fn hgradient_mesh(rect: Rect, stops: &[(f32, Color32)]) -> Mesh {
    let mut mesh = mesh_with_capacity(2 * stops.len(), 2 * stops.len().saturating_sub(1));
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
    mesh
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
/// Built once while its points and colours stay ([`memo_mesh`]).
pub fn gradient_polygon(
    context: &egui::Context,
    points: &[Pos2],
    top: Color32,
    bottom: Color32,
) -> Shape {
    if points.len() < 3 {
        return Shape::Noop;
    }
    let colors = [top, bottom].map(|color| u32::from_le_bytes(color.to_array()));
    let corners = key_floats(points.iter().flat_map(|point| [point.x, point.y]));
    memo_mesh(
        context,
        MeshKind::GradientPolygon,
        colors.into_iter().chain(corners),
        || gradient_polygon_mesh(points, top, bottom),
    )
}

/// [`gradient_polygon`]'s mesh, for three points or more.
fn gradient_polygon_mesh(points: &[Pos2], top: Color32, bottom: Color32) -> Mesh {
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
    let mut mesh = mesh_with_capacity(1 + 2 * count, 3 * count);
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
    mesh
}

/// The armour plate's outline: `rect` with its top-left and bottom-right corners cut.
pub fn cut_corners(rect: Rect, cut: f32) -> [Pos2; 6] {
    [
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
    each_plate_shape(painter.ctx(), rect, cut, tint, |shape| {
        painter.add(shape);
    });
}

/// The shapes of [`plate`], for a slot reserved under content laid out first.
pub fn plate_shapes(
    context: &egui::Context,
    rect: Rect,
    cut: f32,
    tint: Option<Color32>,
) -> Vec<Shape> {
    let mut shapes = Vec::with_capacity(5);
    each_plate_shape(context, rect, cut, tint, |shape| shapes.push(shape));
    shapes
}

/// [`plate`]'s shapes in the order they draw.
fn each_plate_shape(
    context: &egui::Context,
    rect: Rect,
    cut: f32,
    tint: Option<Color32>,
    mut add: impl FnMut(Shape),
) {
    let outline = cut_corners(rect, cut);
    add(gradient_polygon(
        context,
        &outline,
        hexa(PLATE_TOP, 0.97),
        hexa(PLATE_BOTTOM, 0.99),
    ));
    if let Some(tint) = tint {
        // A radial wash from the lower-left corner, as a fan fading outwards.
        let corner = rect.left_bottom();
        let reach = vec2(rect.width() * 0.7, rect.height() * 1.3);
        let steps = 10;
        add(frame_mesh(|mesh| {
            reserve(mesh, steps as usize + 2, steps as usize);
            mesh.colored_vertex(corner, fade(tint, 0.22));
            for step in 0..=steps {
                let angle = -std::f32::consts::FRAC_PI_2 * step as f32 / steps as f32;
                let point = corner + vec2(angle.cos() * reach.x, angle.sin() * reach.y);
                let clamped = pos2(point.x.min(rect.right()), point.y.max(rect.top()));
                mesh.colored_vertex(clamped, Color32::TRANSPARENT);
                if step > 0 {
                    mesh.add_triangle(0, step, step + 1);
                }
            }
        }));
    }
    // The sheen along the top and the shade along the bottom, inside the hairline.
    add(Shape::line_segment(
        [
            pos2(rect.left() + cut, rect.top() + 1.5),
            pos2(rect.right() - 1.0, rect.top() + 1.5),
        ],
        Stroke::new(1.5, hexa(0xffffff, 0.08)),
    ));
    add(Shape::line_segment(
        [
            pos2(rect.left() + 1.0, rect.bottom() - 1.5),
            pos2(rect.right() - cut, rect.bottom() - 1.5),
        ],
        Stroke::new(1.5, hexa(0x000000, 0.35)),
    ));
    add(Shape::closed_line(
        outline.to_vec(),
        Stroke::new(1.0, theme::PLATE_EDGE),
    ));
}

/// A radial gradient: an ellipse of `radii` round `center` through `stops` (share of
/// the radii, colour), the first stop at the centre.
pub fn radial(
    context: &egui::Context,
    center: Pos2,
    radii: Vec2,
    stops: &[(f32, Color32)],
) -> Shape {
    let key = key_floats([center.x, center.y, radii.x, radii.y]).chain(key_stops(stops));
    memo_mesh(context, MeshKind::Radial, key, || {
        radial_mesh(center, radii, stops)
    })
}

/// [`radial`] for a gradient that changes every frame (a fading glow), built in a
/// pooled mesh, not kept.
pub fn frame_radial(center: Pos2, radii: Vec2, stops: &[(f32, Color32)]) -> Shape {
    frame_mesh(|mesh| fill_radial(mesh, center, radii, stops))
}

/// The mesh of [`radial`].
pub fn radial_mesh(center: Pos2, radii: Vec2, stops: &[(f32, Color32)]) -> Mesh {
    let mut mesh = Mesh::default();
    fill_radial(&mut mesh, center, radii, stops);
    mesh
}

fn fill_radial(mesh: &mut Mesh, center: Pos2, radii: Vec2, stops: &[(f32, Color32)]) {
    let segments = 48u32;
    let rings = stops.len().saturating_sub(1);
    reserve(
        mesh,
        1 + rings * segments as usize,
        (2 * rings).saturating_sub(1) * segments as usize,
    );
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
}

/// Text wrapped at `width`.
pub fn wrapped(
    painter: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> Arc<Galley> {
    wrapped_section(painter, text, TextFormat::simple(font, color), width)
}

/// `text` in one `format`, wrapped at `max_width`, kept between passes as [`galley`]
/// keeps one line.
pub fn wrapped_section(
    painter: &Painter,
    text: &str,
    format: TextFormat,
    max_width: f32,
) -> Arc<Galley> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    "wrapped section".hash(&mut hasher);
    text.hash(&mut hasher);
    format.hash(&mut hasher);
    for value in [max_width, painter.ctx().pixels_per_point()] {
        value.to_bits().hash(&mut hasher);
    }
    kept_galley(painter, hasher.finish(), text, || {
        let mut job = LayoutJob::single_section(text.to_owned(), format);
        job.wrap.max_width = max_width;
        painter.layout_job(job)
    })
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

/// The bytes [`ShortText`] holds on the stack: a number, a clock, a name in capitals.
const SHORT_TEXT: usize = 64;

/// A line the screens write every frame from values that rarely change (a score, the
/// clock, the hull's points, a name in capitals), written on the stack: with [`galley`]
/// keeping its layout, building the text was all that line still cost. Longer text
/// spills to the heap.
pub struct ShortText {
    bytes: [u8; SHORT_TEXT],
    len: usize,
    spilled: Option<String>,
}

impl Default for ShortText {
    fn default() -> Self {
        ShortText {
            bytes: [0; SHORT_TEXT],
            len: 0,
            spilled: None,
        }
    }
}

impl std::fmt::Write for ShortText {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        if let Some(spilled) = &mut self.spilled {
            spilled.push_str(text);
            return Ok(());
        }
        let end = self.len + text.len();
        if end <= SHORT_TEXT {
            self.bytes[self.len..end].copy_from_slice(text.as_bytes());
            self.len = end;
        } else {
            let mut spilled = String::with_capacity(end);
            spilled.push_str(self.inline());
            spilled.push_str(text);
            self.spilled = Some(spilled);
        }
        Ok(())
    }
}

impl ShortText {
    /// The text while it fits on the stack: whole `&str`s only, so always UTF-8.
    fn inline(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or_default()
    }

    /// `text` in capitals after what this holds.
    pub fn push_upper(&mut self, text: &str) {
        for letter in text.chars().flat_map(char::to_uppercase) {
            // Writing to a `ShortText` never fails.
            let _ = std::fmt::Write::write_char(self, letter);
        }
    }
}

impl std::ops::Deref for ShortText {
    type Target = str;

    fn deref(&self) -> &str {
        self.spilled.as_deref().unwrap_or_else(|| self.inline())
    }
}

/// `format!` into a [`ShortText`].
pub fn short(arguments: std::fmt::Arguments) -> ShortText {
    let mut text = ShortText::default();
    // Writing to a `ShortText` never fails.
    let _ = std::fmt::Write::write_fmt(&mut text, arguments);
    text
}

/// `text.to_uppercase()` into a [`ShortText`]: capitals char by char, as `str` makes
/// them.
pub fn upper(text: &str) -> ShortText {
    let mut capitals = ShortText::default();
    capitals.push_upper(text);
    capitals
}

/// `response.on_hover_text(text())`, writing the tip only when it can show: while its
/// tooltip is open, or when AccessKit describes the widget by it.
pub fn on_hover_text_with(response: Response, text: impl FnOnce() -> String) -> Response {
    if response.ctx.has_accesskit_node(response.id) {
        return response.on_hover_text(text());
    }
    response.on_hover_ui(|ui| {
        // As `on_hover_text` lays its tip out.
        ui.set_max_width(ui.spacing().tooltip_width);
        ui.add(egui::Label::new(text()));
    })
}

/// The one-line texts [`galley`] laid out, each by a hash of what it was laid out from
/// and the last pass that drew it, so drawing the same text again copies no string into
/// a layout job (egui's own cache finds a galley only from a job). A galley holds places
/// in egui's glyph atlas, so they all go when the fonts change, the atlas is cleared
/// (egui does when it fills) or another context draws.
#[derive(Default)]
struct GalleyMemo {
    /// The context the galleys belong to ([`context_token`]) and the pass that drew last.
    context: u64,
    pass: u64,
    /// A text laid out through egui's cache every pass: egui keeps it while it is used,
    /// so a different one back means egui let its galleys go.
    probe: Option<Arc<Galley>>,
    galleys: HashMap<u64, (Arc<Galley>, u64)>,
}

thread_local! {
    static GALLEYS: RefCell<GalleyMemo> = RefCell::default();
}

/// A number naming `context` for as long as its memory lasts.
fn context_token(context: &egui::Context) -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    static ID: std::sync::LazyLock<Id> =
        std::sync::LazyLock::new(|| Id::unique("paint galley memo"));
    context.data_mut(|data| {
        *data.get_temp_mut_or_insert_with(*ID, || {
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        })
    })
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
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    text.hash(&mut hasher);
    font.hash(&mut hasher);
    color.hash(&mut hasher);
    for value in [spacing, max_width, painter.ctx().pixels_per_point()] {
        value.to_bits().hash(&mut hasher);
    }
    kept_galley(painter, hasher.finish(), text, || {
        layout_galley(painter, text, font, color, spacing, max_width)
    })
}

/// The galley `layout` lays `text` out in, kept by `key`, a hash of everything the
/// layout depends on besides `text` (the pixel size too), for as long as a pass draws
/// it ([`GalleyMemo`]).
pub fn kept_galley(
    painter: &Painter,
    key: u64,
    text: &str,
    layout: impl FnOnce() -> Arc<Galley>,
) -> Arc<Galley> {
    let context = painter.ctx();
    let pass = context.cumulative_pass_nr();
    let token = context_token(context);
    let kept = GALLEYS.with_borrow_mut(|memo| {
        if memo.context != token || memo.pass != pass {
            let probe = context.fonts_mut(|fonts| {
                fonts.layout_no_wrap(String::new(), FontId::default(), Color32::PLACEHOLDER)
            });
            let same_fonts = memo.context == token
                && memo
                    .probe
                    .as_ref()
                    .is_some_and(|kept| Arc::ptr_eq(kept, &probe));
            if same_fonts {
                let last = memo.pass;
                memo.galleys.retain(|_, (_, used)| *used == last);
            } else {
                memo.galleys.clear();
            }
            (memo.context, memo.pass, memo.probe) = (token, pass, Some(probe));
        }
        match memo.galleys.get_mut(&key) {
            // The text compared too, should two keys' hashes meet.
            Some((galley, used)) if galley.job.text == text => {
                *used = pass;
                Some(Arc::clone(galley))
            }
            _ => None,
        }
    });
    kept.unwrap_or_else(|| {
        let galley = layout();
        GALLEYS.with_borrow_mut(|memo| {
            memo.galleys.insert(key, (Arc::clone(&galley), pass));
        });
        galley
    })
}

/// [`galley`]'s layout through egui's own cache.
fn layout_galley(
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

/// [`galley`] in no colour of its own: drawn in the colour its painter passes. egui
/// keeps a laid-out text by everything in its job, colour too, so text whose colour
/// changes every frame (a fading or brightening row) would be shaped again each frame;
/// this one is laid out once, and once for its width, its shadow and itself alike.
fn uncoloured(
    painter: &Painter,
    text: &str,
    font: FontId,
    spacing: f32,
    max_width: f32,
) -> Arc<Galley> {
    galley(
        painter,
        text,
        font,
        Color32::PLACEHOLDER,
        spacing,
        max_width,
    )
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
    let galley = uncoloured(painter, text, font, spacing, f32::INFINITY);
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
    let galley = uncoloured(painter, text, font, spacing, f32::INFINITY);
    let rect = align.anchor_size(pos, galley.size());
    painter.galley(
        rect.min + vec2(0.0, 1.5),
        galley.clone(),
        hexa(0x000000, 0.55),
    );
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
    let galley = uncoloured(painter, text, font, spacing, width);
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
    uncoloured(painter, text, font, spacing, f32::INFINITY)
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
/// A whole ring (a ready key's rim) stays the same from frame to frame, so its stroke is
/// kept; a part one is mostly a countdown, different every frame.
pub fn arc(painter: &Painter, center: Pos2, radius: f32, start: f32, sweep: f32, stroke: Stroke) {
    if sweep.abs() < 1e-3 {
        return;
    }
    let points = move || {
        let steps = ((sweep.abs() / std::f32::consts::TAU) * 64.0)
            .ceil()
            .max(2.0) as usize;
        (0..=steps).map(move |step| {
            let angle = start + sweep * step as f32 / steps as f32;
            center + vec2(angle.sin(), -angle.cos()) * radius
        })
    };
    if sweep.abs() < std::f32::consts::TAU {
        painter.add(Shape::line(points().collect(), stroke));
        return;
    }
    let feathering = feathering(painter);
    let key = key_floats([
        center.x,
        center.y,
        radius,
        start,
        sweep,
        stroke.width,
        feathering,
    ])
    .chain([u32::from_le_bytes(stroke.color.to_array())]);
    painter.add(memo_mesh(painter.ctx(), MeshKind::Ring, key, || {
        line_mesh(&points().collect::<Vec<_>>(), false, stroke, feathering)
    }));
}

/// The width of the soft edge egui's tessellator gives a line drawn with `painter`.
pub fn feathering(painter: &Painter) -> f32 {
    painter.ctx().tessellation_options(|options| {
        if options.feathering {
            options.feathering_size_in_pixels * (1.0 / painter.pixels_per_point())
        } else {
            0.0
        }
    })
}

/// The mesh egui's tessellator makes of a line through `points` (`closed` back to the
/// first) with a soft edge of `feathering`, the same triangles a `Shape::line` or
/// `Shape::closed_line` draws, for a line kept by [`memo_mesh`].
pub fn line_mesh(points: &[Pos2], closed: bool, stroke: Stroke, feathering: f32) -> Mesh {
    let mut path = egui::epaint::tessellator::Path::default();
    let mut mesh = Mesh::default();
    if closed {
        path.add_line_loop(points);
        path.fill_and_stroke(feathering, Color32::TRANSPARENT, &stroke.into(), &mut mesh);
    } else {
        path.add_open_points(points);
        path.stroke_open(feathering, &stroke.into(), &mut mesh);
    }
    mesh
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
    let key = key_floats([center.x, center.y, radius.x, radius.y, opacity]).chain(key_stops(stops));
    let mesh = memo_mesh(painter.ctx(), MeshKind::Vignette, key, || {
        vignette_mesh(center, radius, stops, opacity)
    });
    painter.with_clip_rect(rect).add(mesh);
}

/// The mesh of [`vignette_at`]: rings of `radius` round `center`.
fn vignette_mesh(center: Pos2, radius: Vec2, stops: &[(f32, Color32)], opacity: f32) -> Mesh {
    let segments = 48;
    // The last stop's colour carries on past the corners.
    let mut rings: Vec<(f32, Color32)> = Vec::with_capacity(stops.len() + 1);
    rings.extend_from_slice(stops);
    if let Some(&(_, last)) = stops.last() {
        rings.push((3.0, last));
    }
    let mut mesh = mesh_with_capacity(
        rings.len() * segments as usize,
        rings.len().saturating_sub(1) * 2 * segments as usize,
    );
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
    mesh
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
    frame_mesh(|mesh| {
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
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_writes_as_format_and_capitals_as_str_does() {
        for name in ["Swift Falcon", "Rookie", "straße ǆ ﬁ", "BIG RIG", ""] {
            assert_eq!(&*upper(name), name.to_uppercase());
        }
        assert_eq!(&*short(format_args!("/ {}", 140.0_f64)), "/ 140");
        let long = "x".repeat(3 * SHORT_TEXT);
        assert_eq!(&*short(format_args!("{long}·{}", 7)), format!("{long}·7"));
        let mut eyebrow = short(format_args!("TEAM BATTLE · "));
        eyebrow.push_upper("Pine Village");
        assert_eq!(&*eyebrow, "TEAM BATTLE · PINE VILLAGE");
    }

    /// The galley `text` draws in one pass of `context`.
    fn drawn(context: &egui::Context, text: &str) -> Arc<Galley> {
        let mut galley = None;
        let mut output = context.run_pass(egui::RawInput::default(), |context| {
            let painter = context.layer_painter(egui::LayerId::background());
            galley = Some(uncoloured(&painter, text, FontId::default(), 0.0, 100.0));
        });
        // No painter takes the font atlas here.
        output.textures_delta.clear();
        galley.expect("a pass ran")
    }

    /// A shape as [`frame_mesh`] draws one from `pool`: a vertex at `x`.
    fn pooled(pool: &mut MeshPool, x: f32, texture: egui::TextureId) -> Shape {
        let mut mesh = pool.take();
        let fill = Arc::get_mut(&mut mesh).expect("free");
        fill.texture_id = texture;
        fill.vertices.push(egui::epaint::Vertex {
            pos: pos2(x, 0.0),
            uv: Pos2::ZERO,
            color: Color32::WHITE,
        });
        Shape::Mesh(mesh)
    }

    fn clipped(shapes: impl IntoIterator<Item = Shape>) -> Vec<egui::epaint::ClippedShape> {
        shapes
            .into_iter()
            .map(|shape| egui::epaint::ClippedShape::new(Rect::EVERYTHING, shape))
            .collect()
    }

    fn mesh_of(shape: &Shape) -> &Arc<Mesh> {
        match shape {
            Shape::Mesh(mesh) => mesh,
            _ => panic!("a mesh shape"),
        }
    }

    #[test]
    fn a_pass_s_meshes_are_filled_again_in_the_order_they_were_drawn() {
        let mut pool = MeshPool::default();
        let pass = clipped([
            pooled(&mut pool, 1.0, egui::TextureId::User(7)),
            // A group's meshes are kept too.
            Shape::Vec(vec![
                pooled(&mut pool, 2.0, egui::TextureId::default()),
                Shape::Noop,
            ]),
        ]);
        // While its pass is laid out a shape holds its mesh alone, so egui moves it in
        // place on a transformed layer.
        assert_eq!(Arc::strong_count(mesh_of(&pass[0].shape)), 1);
        let first = {
            let mesh = mesh_of(&pass[0].shape);
            (Arc::as_ptr(mesh), mesh.vertices.as_ptr())
        };
        pool.keep(&pass);
        assert_eq!(pool.spare.len(), 2);
        // Tessellated: egui drops the pass's shapes.
        drop(pass);
        let again = pooled(&mut pool, 3.0, egui::TextureId::default());
        let mesh = mesh_of(&again);
        assert_eq!((Arc::as_ptr(mesh), mesh.vertices.as_ptr()), first);
        assert_eq!(mesh.vertices.len(), 1);
        assert_eq!(mesh.vertices[0].pos.x, 3.0);
        assert_eq!(mesh.texture_id, egui::TextureId::default());
        assert_eq!(pool.spare.len(), 1);
    }

    #[test]
    fn a_mesh_something_else_holds_is_never_filled_again() {
        let mut pool = MeshPool::default();
        // Kept elsewhere (as `memo_mesh` keeps one) or shared by two shapes: not pooled.
        let kept = Arc::new(Mesh::default());
        let shared = pooled(&mut pool, 1.0, egui::TextureId::default());
        let big = Shape::mesh(mesh_with_capacity(POOLED_VERTICES + 1, 1));
        let pass = clipped([Shape::Mesh(Arc::clone(&kept)), shared.clone(), shared, big]);
        pool.keep(&pass);
        assert!(pool.spare.is_empty());
        // A pass kept untessellated holds its meshes: the next gets ones of its own.
        let held = clipped([pooled(&mut pool, 2.0, egui::TextureId::default())]);
        pool.keep(&held);
        assert_eq!(pool.spare.len(), 1);
        let other = pooled(&mut pool, 3.0, egui::TextureId::default());
        assert!(!Arc::ptr_eq(mesh_of(&other), mesh_of(&held[0].shape)));
        assert_eq!(mesh_of(&held[0].shape).vertices[0].pos.x, 2.0);
        // And the pool holds no more than its bound, nor a pass's meshes past the next.
        let many = clipped(
            (0..POOLED_MESHES + 3).map(|x| pooled(&mut pool, x as f32, Default::default())),
        );
        pool.keep(&many);
        assert_eq!(pool.spare.len(), POOLED_MESHES);
        pool.keep(&[]);
        assert!(pool.spare.is_empty());
    }

    #[test]
    fn a_kept_mesh_is_copied_only_onto_a_layer_drawn_through_a_transform() {
        use egui::{LayerId, Order};
        let context = egui::Context::default();
        let stops = [(0.0, Color32::RED), (1.0, Color32::BLUE)];
        let kept = Arc::new(radial_mesh(pos2(50.0, 40.0), vec2(10.0, 8.0), &stops));
        let moved_layer = LayerId::new(Order::Middle, Id::unique("moved"));
        let still_layer = LayerId::new(Order::Middle, Id::unique("still"));
        let transform = egui::emath::TSTransform::new(vec2(7.0, -3.0), 1.5);
        let meshes = |output: &egui::FullOutput| -> Vec<Arc<Mesh>> {
            output
                .shapes
                .iter()
                .map(|clipped| Arc::clone(mesh_of(&clipped.shape)))
                .collect()
        };
        // Run a pass of `build` and leave no texture for a GPU to take.
        let pass = |build: &dyn Fn(&egui::Context)| {
            let mut output = run_pass(&context, egui::RawInput::default(), build);
            output.textures_delta.clear();
            output
        };
        let output = pass(&|context| {
            context.set_transform_layer(moved_layer, transform);
            for layer in [moved_layer, still_layer] {
                context
                    .layer_painter(layer)
                    .add(Shape::Mesh(Arc::clone(&kept)));
            }
        });
        let drawn = meshes(&output);
        let (still, moved): (Vec<_>, Vec<_>) =
            drawn.iter().partition(|mesh| Arc::ptr_eq(mesh, &kept));
        // The still layer draws the kept mesh itself; the moved layer a copy, moved as
        // egui moves a mesh, with the kept one as it was.
        assert_eq!((still.len(), moved.len()), (1, 1));
        let mut expected = (*kept).clone();
        expected.transform(transform);
        assert_eq!(**moved[0], expected);
        assert_eq!(
            *kept,
            radial_mesh(pos2(50.0, 40.0), vec2(10.0, 8.0), &stops)
        );
        drop((still, moved));
        drop((drawn, output));
        // egui remembers the layer's transform, but a pass that does not draw on it
        // copies nothing.
        let output = pass(&|context| {
            context
                .layer_painter(still_layer)
                .add(Shape::Mesh(Arc::clone(&kept)));
        });
        assert!(context.layer_transform_to_global(moved_layer).is_some());
        let drawn = meshes(&output);
        assert_eq!(drawn.len(), 1);
        assert!(Arc::ptr_eq(&drawn[0], &kept));
    }

    #[test]
    fn pooled_meshes_draw_what_egui_and_the_kept_builders_do() {
        let rect = Rect::from_min_size(pos2(3.0, 4.0), vec2(20.0, 10.0));
        let stops = [
            (0.0, Color32::RED),
            (0.4, Color32::GREEN),
            (1.0, Color32::BLUE),
        ];
        let Shape::Mesh(drawn) = frame_radial(rect.center(), rect.size(), &stops) else {
            panic!("a radial gradient is a mesh");
        };
        assert_eq!(*drawn, radial_mesh(rect.center(), rect.size(), &stops));
        let uv = Rect::from_min_max(pos2(1.0, 0.0), pos2(0.0, 1.0));
        let texture = egui::TextureId::User(3);
        let Shape::Mesh(drawn) = image(texture, rect, uv, Color32::GOLD) else {
            panic!("an image is a mesh");
        };
        let Shape::Mesh(egui_draws) = Shape::image(texture, rect, uv, Color32::GOLD) else {
            panic!("egui's image is a mesh");
        };
        assert_eq!(*drawn, *egui_draws);
        // A kept mesh moved is the mesh egui's own translation gives, and stays as kept.
        let kept = Arc::new(radial_mesh(rect.center(), rect.size(), &stops));
        let offset = vec2(17.5, -3.25);
        let mut translated = Shape::Mesh(Arc::new((*kept).clone()));
        translated.translate(offset);
        let Shape::Mesh(translated) = translated else {
            panic!("a translated mesh is a mesh");
        };
        let Shape::Mesh(drawn) = moved(Shape::Mesh(Arc::clone(&kept)), offset) else {
            panic!("a moved mesh is a mesh");
        };
        assert_eq!(*drawn, *translated);
        assert_eq!(*kept, radial_mesh(rect.center(), rect.size(), &stops));
    }

    #[test]
    fn galleys_are_kept_while_drawn_and_never_outlive_their_atlas() {
        let first = drawn(&egui::Context::default(), "KILLS");
        // Another context draws at the same pass number, with an atlas of its own.
        let other = egui::Context::default();
        let kept = drawn(&other, "KILLS");
        assert!(!Arc::ptr_eq(&first, &kept));
        assert!(Arc::ptr_eq(&kept, &drawn(&other, "KILLS")));
        // New fonts put their glyphs in a new atlas.
        theme::install(&other);
        assert!(!Arc::ptr_eq(&kept, &drawn(&other, "KILLS")));
    }
}
