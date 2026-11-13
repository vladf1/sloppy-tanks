//! A true-to-scale plan of the Neighborhood (Senda) as the layout builds it, and a table
//! of its measured dimensions, for comparing the level with its reference.
//! `cargo run --example neighborhood_plan -- <out dir>` writes `plan.svg` (10 px per
//! metre, +x right, +z down) and `dimensions.md` there.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use glam::{DMat4, DVec2, DVec3};
use sloppy_core::geometry::node_bounds;
use sloppy_core::models::{TREE_FAMILIES, TreeDetail, TreeShape, tree_model};
use sloppy_core::sim::arena::{CoverDef, box_turn, spawn_positions};
use sloppy_core::sim::house_layout::{Plan, style_design, wall_frames};
use sloppy_core::sim::model_data::ARENA;
use sloppy_core::sim::senda_layout::{
    CUL_DE_SAC_ISLAND, CUL_DE_SAC_RADIUS, LAMP_HEIGHT, Line, MEDIAN_HALF, PARKWAY, Row,
    SENDA_FENCE_COLOR, SENDA_PICKUPS, SENDA_WALL_COLOR, SIDEWALK, STREET_LAMP_COLORS, SendaHouse,
    Street, TRASH_BIN_COLORS, WAY_LAMP_HEIGHT, WAY_PARKWAY, WAY_PATH, bend_park, greenbelt_line,
    in_median_opening, inside_boundary, lamp_sites, left, outer_way_tree_sites, past_sidewalk,
    senda_boundary, senda_design_layout, senda_houses, splitter_island,
};
use sloppy_core::sim::senda_yards::{
    LOGGIA_COLOR, PERGOLA_WOOD, YARD_GATE_COLOR, YARD_PERIMETER_COLOR, YARD_VIEW_COLOR,
    YARD_WALL_COLOR, greenbelt_lane, house_back, senda_lots,
};
use sloppy_core::sim::tank_dimensions::tank_hull;
use sloppy_core::sim::tree_proportions::tree_proportions;
use sloppy_core::sim::{CoverKind, Team, VehicleKind};

const PX: f64 = 10.0;
/// A spawn pad's radius (`SPAWN_PAD`, private to the layout).
const SPAWN_PAD: f64 = 2.3;
/// How much wider a driveway is than its garage door (`DRIVEWAY_PAST_DOOR`, private to
/// the scenery).
const DRIVEWAY_PAST_DOOR: f64 = 0.15;
const REAL_M1_WIDTH: f64 = 3.66;

struct Svg {
    body: String,
    x0: f64,
    z0: f64,
}

impl Svg {
    fn p(&self, v: DVec2) -> (f64, f64) {
        ((v.x - self.x0) * PX, (v.y - self.z0) * PX)
    }
    fn points(&self, pts: &[DVec2]) -> String {
        pts.iter()
            .map(|&v| {
                let (x, y) = self.p(v);
                format!("{x:.1},{y:.1}")
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn polygon(&mut self, pts: &[DVec2], style: &str) {
        let points = self.points(pts);
        writeln!(self.body, r#"<polygon points="{points}" {style}/>"#).unwrap();
    }
    fn polyline(&mut self, pts: &[DVec2], style: &str) {
        let points = self.points(pts);
        writeln!(
            self.body,
            r#"<polyline points="{points}" fill="none" {style}/>"#
        )
        .unwrap();
    }
    fn circle(&mut self, at: DVec2, r: f64, style: &str) {
        let (x, y) = self.p(at);
        writeln!(
            self.body,
            r#"<circle cx="{x:.1}" cy="{y:.1}" r="{:.1}" {style}/>"#,
            r * PX
        )
        .unwrap();
    }
    fn text(&mut self, at: DVec2, size: f64, text: &str, style: &str) {
        let (x, y) = self.p(at);
        writeln!(
            self.body,
            r#"<text x="{x:.1}" y="{y:.1}" font-size="{size}" font-family="Helvetica,Arial,sans-serif" {style}>{text}</text>"#
        )
        .unwrap();
    }
}

/// Turns a box-frame offset into the world as a cover's turn about +y does: its x to
/// (cos, -sin).
fn turned(v: DVec2, turn: f64) -> DVec2 {
    let (sin, cos) = turn.sin_cos();
    DVec2::new(v.x * cos + v.y * sin, -v.x * sin + v.y * cos)
}

fn cover_corners(c: &CoverDef) -> [DVec2; 4] {
    let turn = box_turn(c.kind, c.house_plan);
    let (hw, hd) = (c.w / 2.0, c.d / 2.0);
    [(-hw, -hd), (hw, -hd), (hw, hd), (-hw, hd)]
        .map(|(u, v)| DVec2::new(c.x, c.z) + turned(DVec2::new(u, v), turn))
}

/// A house's wall corners and its front wall's middle and outward direction, in the world.
fn house_frame(house: &SendaHouse) -> ([DVec2; 4], DVec2, DVec2) {
    let design = house.design();
    let at = DVec2::new(house.x, house.z);
    let world = |v: DVec3| {
        let w = design.to_world(v);
        DVec2::new(w.x, w.z)
    };
    let (hw, hd) = (design.w / 2.0, design.d / 2.0);
    let corners = [(-hw, -hd), (hw, -hd), (hw, hd), (-hw, hd)]
        .map(|(u, v)| at + world(DVec3::new(u, 0.0, v)));
    let (centre, out, _, _) = wall_frames(design.w, design.d)[design.door_wall];
    (corners, at + world(centre), world(out).normalize())
}

fn band(line: &Line, from: f64, to: f64) -> (Vec<DVec2>, f64) {
    (line.offset((from + to) / 2.0), (to - from).abs())
}

fn plan_letter(plan: Plan) -> &'static str {
    match plan {
        Plan::Small => "S",
        Plan::Medium => "M",
        Plan::Large => "L",
    }
}

fn row_tag(row: Row) -> &'static str {
    match row {
        Row::RumboNorth => "RN",
        Row::RumboSouth => "RS",
        Row::GaviotaNorth => "GN",
        Row::NeblinaWest => "NW",
        Row::GaviotaSouth => "GS",
    }
}

fn row_street(row: Row) -> Street {
    match row {
        Row::RumboNorth | Row::RumboSouth => Street::Rumbo,
        _ => Street::NeblinaGaviota,
    }
}

/// Separating-axis gap between two convex quads: negative when they overlap (by the
/// least penetration), else the largest separation along an edge normal (a lower bound
/// of the true distance, exact for parallel boxes).
fn quad_gap(a: &[DVec2; 4], b: &[DVec2; 4]) -> f64 {
    let mut best = f64::NEG_INFINITY;
    for quad in [a, b] {
        for k in 0..4 {
            let edge = quad[(k + 1) % 4] - quad[k];
            let axis = DVec2::new(-edge.y, edge.x).normalize();
            let span = |q: &[DVec2; 4]| {
                q.iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), c| {
                        (lo.min(c.dot(axis)), hi.max(c.dot(axis)))
                    })
            };
            let ((a0, a1), (b0, b1)) = (span(a), span(b));
            best = best.max((b0 - a1).max(a0 - b1));
        }
    }
    best
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let u = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
    p.distance(a + (b - a) * u)
}

/// The least distance between two convex quads' outlines (0 when they overlap).
fn quad_distance(a: &[DVec2; 4], b: &[DVec2; 4]) -> f64 {
    if quad_gap(a, b) < 0.0 {
        return 0.0;
    }
    let mut best = f64::INFINITY;
    for (p, q) in [(a, b), (b, a)] {
        for &c in p {
            for k in 0..4 {
                best = best.min(segment_distance(c, q[k], q[(k + 1) % 4]));
            }
        }
    }
    best
}

fn stats(values: &mut [f64]) -> String {
    if values.is_empty() {
        return "–".into();
    }
    values.sort_by(f64::total_cmp);
    let median = values[values.len() / 2];
    format!(
        "{:.2} / {:.2} / {:.2}",
        values[0],
        median,
        values[values.len() - 1]
    )
}

fn range(values: &[f64]) -> String {
    if values.is_empty() {
        return "–".into();
    }
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if (hi - lo).abs() < 0.005 {
        format!("{lo:.2}")
    } else {
        format!("{lo:.2}–{hi:.2}")
    }
}

/// A cover tree's kind, trunk radius, crown radius, height, footprint and family.
type TreeRow = (TreeKind, f64, f64, f64, f64, &'static str);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum TreeKind {
    Street,
    Median,
    ParkwayNorth,
    ParkwaySouth,
    Greenbelt,
    Island,
    Bank,
    SlopeBand,
}

impl TreeKind {
    fn color(self) -> &'static str {
        match self {
            TreeKind::Street => "#3c9a3c",
            TreeKind::Median | TreeKind::ParkwayNorth | TreeKind::ParkwaySouth => "#1f7a7a",
            TreeKind::Greenbelt => "#2e5e1e",
            TreeKind::Island => "#8a9a20",
            TreeKind::Bank | TreeKind::SlopeBand => "#6b4f1d",
        }
    }
}

fn main() {
    let out = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "artifacts/performance/neighborhood-redo/current".into()),
    );
    std::fs::create_dir_all(&out).unwrap();

    let covers = senda_design_layout();
    let houses = senda_houses();
    let lots = senda_lots(&houses);
    let boundary = senda_boundary();
    let lines: Vec<(Street, Line)> = Street::ALL.map(|s| (s, s.line())).into();
    let way = Street::SendaParkway.line();
    let greenbelt = Line(greenbelt_line(0.0));
    // Which side of Senda Parkway (left of its trace, +1, or right) is north (-z).
    let (_, way_along) = way.at(way.length() / 2.0);
    let north_side = if left(way_along).y < 0.0 { 1.0 } else { -1.0 };

    // The drawing's extent: the arena, the wall, every cover and the Way's outer trees.
    let mut lo = DVec2::splat(-ARENA);
    let mut hi = DVec2::splat(ARENA);
    for p in boundary
        .iter()
        .copied()
        .chain(covers.iter().flat_map(cover_corners))
        .chain(outer_way_tree_sites())
    {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let margin = 14.0;
    let (x0, z0) = ((lo.x - margin).floor(), (lo.y - margin).floor());
    let (x1, z1) = ((hi.x + margin).ceil(), (hi.y + margin + 18.0).ceil());
    let (width, height) = ((x1 - x0) * PX, (z1 - z0) * PX);
    let mut svg = Svg {
        body: String::new(),
        x0,
        z0,
    };
    writeln!(
        svg.body,
        r#"<defs><clipPath id="sheet"><rect x="0" y="0" width="{width}" height="{height}"/></clipPath></defs>"#
    )
    .unwrap();
    writeln!(
        svg.body,
        r##"<rect x="0" y="0" width="{width}" height="{height}" fill="#f4f1e8"/>"##
    )
    .unwrap();
    writeln!(svg.body, r#"<g clip-path="url(#sheet)">"#).unwrap();

    // Streets, outermost band first: sidewalks, parkway strips, asphalt, median.
    for (street, line) in &lines {
        let half = street.half();
        let (north, south) = if *street == Street::SendaParkway {
            ((PARKWAY, SIDEWALK), (WAY_PARKWAY, WAY_PATH))
        } else {
            ((PARKWAY, SIDEWALK), (PARKWAY, SIDEWALK))
        };
        for (side, (strip, walk)) in [(north_side, north), (-north_side, south)] {
            let (pts, w) = band(line, side * (half + strip), side * (half + strip + walk));
            svg.polyline(
                &pts,
                &format!(
                    r##"stroke="#c9c4b8" stroke-width="{:.1}" stroke-linejoin="round""##,
                    w * PX
                ),
            );
        }
        for (side, (strip, _)) in [(north_side, north), (-north_side, south)] {
            let (pts, w) = band(line, side * half, side * (half + strip));
            svg.polyline(
                &pts,
                &format!(
                    r##"stroke="#b5cf8e" stroke-width="{:.1}" stroke-linejoin="round""##,
                    w * PX
                ),
            );
        }
    }
    for (street, line) in &lines {
        if let Some(middle) = street.cul_de_sac() {
            svg.circle(
                middle,
                CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK,
                r##"fill="#c9c4b8""##,
            );
            svg.circle(middle, CUL_DE_SAC_RADIUS + PARKWAY, r##"fill="#b5cf8e""##);
        }
        let _ = line;
    }
    for (street, line) in &lines {
        svg.polyline(
            &line.0,
            &format!(
                r##"stroke="#6d6d6d" stroke-width="{:.1}" stroke-linejoin="round""##,
                2.0 * street.half() * PX
            ),
        );
        if let Some(middle) = street.cul_de_sac() {
            svg.circle(middle, CUL_DE_SAC_RADIUS, r##"fill="#6d6d6d""##);
            svg.circle(
                middle,
                CUL_DE_SAC_ISLAND,
                r##"fill="#7fae52" stroke="#eee" stroke-width="1.5""##,
            );
        }
    }
    // The median, open across Neblina's mouth.
    let mut run: Vec<DVec2> = Vec::new();
    let mut s = 0.0;
    while s <= way.length() {
        let p = way.at(s).0;
        if in_median_opening(p, 0.0) {
            if run.len() > 1 {
                svg.polyline(
                    &run,
                    &format!(
                        r##"stroke="#7fae52" stroke-width="{:.1}""##,
                        2.0 * MEDIAN_HALF * PX
                    ),
                );
            }
            run.clear();
        } else {
            run.push(p);
        }
        s += 0.5;
    }
    if run.len() > 1 {
        svg.polyline(
            &run,
            &format!(
                r##"stroke="#7fae52" stroke-width="{:.1}""##,
                2.0 * MEDIAN_HALF * PX
            ),
        );
    }
    // The splitter island in Neblina's mouth: round end 1.2 m wide tapering to a point.
    let neblina = Street::NeblinaGaviota.line();
    let (from, to) = splitter_island();
    let (a, along) = neblina.at(from);
    let tip = neblina.at(to).0;
    let across = left(along) * 0.6;
    svg.polygon(
        &[a + across, tip, a - across],
        r##"fill="#7fae52" stroke="#eee" stroke-width="1""##,
    );
    // Centre lines.
    for (_, line) in &lines {
        svg.polyline(
            &line.0,
            r##"stroke="#f2e27a" stroke-width="1" stroke-dasharray="6,6""##,
        );
    }
    // The greenbelt's lane and the bend park.
    svg.polyline(
        &greenbelt_lane(),
        r##"stroke="#2e5e1e" stroke-width="1.5" stroke-dasharray="3,5""##,
    );
    svg.circle(
        bend_park(),
        0.6,
        r##"fill="none" stroke="#2e5e1e" stroke-width="2""##,
    );
    svg.text(
        bend_park() + DVec2::new(1.0, 0.4),
        11.0,
        "bend park",
        r##"fill="#2e5e1e""##,
    );

    // 10 m grid, the arena square and its axes.
    let mut g = (x0 / 10.0).ceil() * 10.0;
    while g <= x1 {
        let (x, _) = svg.p(DVec2::new(g, 0.0));
        let major = g == 0.0;
        writeln!(
            svg.body,
            r##"<line x1="{x:.1}" y1="0" x2="{x:.1}" y2="{height}" stroke="#000" stroke-opacity="{}" stroke-width="1"/>"##,
            if major { 0.35 } else { 0.12 }
        )
        .unwrap();
        svg.text(
            DVec2::new(g + 0.3, z0 + 2.0),
            11.0,
            &format!("x {g}"),
            r##"fill="#444""##,
        );
        g += 10.0;
    }
    let mut g = (z0 / 10.0).ceil() * 10.0;
    while g <= z1 {
        let (_, y) = svg.p(DVec2::new(0.0, g));
        let major = g == 0.0;
        writeln!(
            svg.body,
            r##"<line x1="0" y1="{y:.1}" x2="{width}" y2="{y:.1}" stroke="#000" stroke-opacity="{}" stroke-width="1"/>"##,
            if major { 0.35 } else { 0.12 }
        )
        .unwrap();
        svg.text(
            DVec2::new(x0 + 0.3, g - 0.3),
            11.0,
            &format!("z {g}"),
            r##"fill="#444""##,
        );
        g += 10.0;
    }
    let square = [
        DVec2::new(-ARENA, -ARENA),
        DVec2::new(ARENA, -ARENA),
        DVec2::new(ARENA, ARENA),
        DVec2::new(-ARENA, ARENA),
    ];
    svg.polygon(
        &square,
        r##"fill="none" stroke="#000" stroke-width="2" stroke-dasharray="12,6""##,
    );

    // Covers by what they are.
    let mut tree_rows: Vec<TreeRow> = Vec::new();
    let mut tree_kinds: Vec<(DVec2, TreeKind)> = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut wall_heights: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    let mut lamp_heights: Vec<f64> = Vec::new();
    for c in &covers {
        let at = DVec2::new(c.x, c.z);
        let corners = cover_corners(c);
        let label = match c.kind {
            CoverKind::House => "house",
            CoverKind::Tree => "tree",
            CoverKind::Drum if TRASH_BIN_COLORS.contains(&c.color) => "wheelie bin",
            CoverKind::Boundary if c.color == SENDA_FENCE_COLOR => "level wall (view fence)",
            CoverKind::Boundary if c.color == SENDA_WALL_COLOR => "level wall (block)",
            CoverKind::Concrete if STREET_LAMP_COLORS[0] == c.color => "street lamp (row)",
            CoverKind::Concrete if STREET_LAMP_COLORS[1] == c.color => "street lamp (tall, Way)",
            CoverKind::Concrete if c.color == LOGGIA_COLOR => "patio loggia",
            CoverKind::Concrete if c.color == PERGOLA_WOOD => "patio pergola",
            CoverKind::Concrete if c.color == YARD_WALL_COLOR => "lot wall (block)",
            CoverKind::Concrete if c.color == YARD_VIEW_COLOR => "lot wall (view fence)",
            CoverKind::Concrete if c.color == YARD_GATE_COLOR => "lot wall (gate return)",
            CoverKind::Concrete if c.color == YARD_PERIMETER_COLOR => "lot wall (perimeter block)",
            _ => "other",
        };
        let key = if label == "other" {
            format!("other {:?} #{:06x}", c.kind, c.color)
        } else {
            label.to_string()
        };
        *counts.entry(key).or_default() += 1;
        match label {
            "house" | "tree" => {}
            l if l.starts_with("level wall") => {
                wall_heights.entry("level wall").or_default().push(c.h);
                let fill = if c.color == SENDA_FENCE_COLOR {
                    "#222"
                } else {
                    "#a8916a"
                };
                svg.polygon(
                    &corners,
                    &format!(r##"fill="{fill}" fill-opacity="0.55" stroke="none""##),
                );
            }
            l if l.starts_with("lot wall") => {
                wall_heights.entry("lot wall").or_default().push(c.h);
                let fill = match c.color {
                    YARD_VIEW_COLOR => "#222",
                    YARD_GATE_COLOR => "#8b5a2b",
                    YARD_PERIMETER_COLOR => "#4a3020",
                    _ => "#8f7762",
                };
                svg.polygon(&corners, &format!(r##"fill="{fill}" stroke="none""##));
            }
            l if l.starts_with("street lamp") => {
                lamp_heights.push(c.h);
                svg.circle(
                    at,
                    0.45,
                    r##"fill="#f0c020" stroke="#000" stroke-width="1""##,
                );
            }
            "wheelie bin" => {
                svg.polygon(
                    &corners,
                    r##"fill="#2d5c8a" stroke="#000" stroke-width="0.5""##,
                );
            }
            "patio loggia" | "patio pergola" => {
                svg.polygon(
                    &corners,
                    r##"fill="#d9c9a7" fill-opacity="0.8" stroke="#8a6c50" stroke-width="1""##,
                );
            }
            _ => {
                svg.polygon(
                    &corners,
                    r##"fill="#ff00ff" stroke="#000" stroke-width="1""##,
                );
            }
        }
        if c.kind == CoverKind::Tree {
            let in_parkway = [Street::Rumbo, Street::NeblinaGaviota]
                .into_iter()
                .any(|street| street.line().distance(at) < street.half() + PARKWAY + 0.2);
            let kind = if in_parkway || c.w < 1.0 {
                TreeKind::Street
            } else if (c.w - 1.4).abs() < 1e-6 {
                let off = way.signed_distance(at);
                if off.abs() < 1.0 {
                    TreeKind::Median
                } else if off * north_side > 0.0 {
                    TreeKind::ParkwayNorth
                } else {
                    TreeKind::ParkwaySouth
                }
            } else if Street::ALL
                .into_iter()
                .filter_map(Street::cul_de_sac)
                .any(|m| m.distance(at) < CUL_DE_SAC_RADIUS)
            {
                TreeKind::Island
            } else if greenbelt.0.len() > 1 && greenbelt.distance(at) < 6.0 {
                TreeKind::Greenbelt
            } else if c.z < 0.0 {
                TreeKind::Bank
            } else {
                TreeKind::SlopeBand
            };
            let proportions = tree_proportions(c.x, c.z, c.w, c.d, c.h);
            let model = tree_model(
                &TreeShape {
                    x: c.x,
                    z: c.z,
                    w: c.w,
                    d: c.d,
                    h: c.h,
                },
                TreeDetail::Full,
            );
            let size = node_bounds(&model, DMat4::IDENTITY).size();
            let crown = size.x.max(size.z) / 2.0;
            tree_rows.push((
                kind,
                proportions.radius,
                crown,
                size.y,
                c.w,
                TREE_FAMILIES[proportions.family as usize],
            ));
            tree_kinds.push((at, kind));
            svg.circle(
                at,
                crown,
                &format!(
                    r##"fill="{}" fill-opacity="0.35" stroke="{}" stroke-width="1""##,
                    kind.color(),
                    kind.color()
                ),
            );
            svg.polygon(&corners, r##"fill="none" stroke="#000" stroke-opacity="0.4" stroke-width="0.6" stroke-dasharray="2,2""##);
            svg.circle(at, proportions.radius.max(0.12), r##"fill="#3b2a1a""##);
        }
    }
    // The Way's trees the scenery draws outside the level, and lamps that are not cover.
    for at in outer_way_tree_sites() {
        svg.circle(
            at,
            1.4 * 0.5,
            r##"fill="none" stroke="#1f7a7a" stroke-width="1" stroke-dasharray="2,2""##,
        );
    }
    for (at, tall) in lamp_sites() {
        if !covers
            .iter()
            .any(|c| DVec2::new(c.x, c.z).distance(at) < 0.02)
        {
            svg.circle(
                at,
                0.45,
                r##"fill="none" stroke="#c09000" stroke-width="1""##,
            );
        }
        let _ = tall;
    }

    // Houses: walls, eaves dashed, driveway, front tick and label.
    let mut row_lot: BTreeMap<&str, usize> = BTreeMap::new();
    let mut house_labels: Vec<String> = Vec::new();
    for house in &houses {
        let design = house.design();
        let (corners, front, out) = house_frame(house);
        let roof = design.roof();
        let at = DVec2::new(house.x, house.z);
        let world = |u: f64, v: f64| {
            let w = design.to_world(DVec3::new(u, 0.0, v));
            at + DVec2::new(w.x, w.z)
        };
        let eaves = [
            world(-roof.eave_x, -roof.rake_z),
            world(roof.eave_x, -roof.rake_z),
            world(roof.eave_x, roof.rake_z),
            world(-roof.eave_x, roof.rake_z),
        ];
        let plan = design.wall_plan(design.door_wall);
        let (fc, _, along, _) = wall_frames(design.w, design.d)[design.door_wall];
        if let Some(garage) = plan.garage() {
            let width = plan.garage_width() + DRIVEWAY_PAST_DOOR;
            let length = house.setback + SIDEWALK + PARKWAY;
            let mid = world(fc.x + along.x * garage, fc.z + along.z * garage);
            let side = left(out) * width / 2.0;
            svg.polygon(
                &[
                    mid - side,
                    mid + side,
                    mid + side + out * length,
                    mid - side + out * length,
                ],
                r##"fill="#d8d0c0" stroke="#a09888" stroke-width="0.6""##,
            );
        }
        svg.polygon(
            &eaves,
            r##"fill="none" stroke="#b04a2a" stroke-width="0.8" stroke-dasharray="3,2""##,
        );
        svg.polygon(
            &corners,
            r##"fill="#e8b48a" stroke="#7a3a1a" stroke-width="1.5""##,
        );
        svg.polyline(
            &[front, front + out * 1.2],
            r##"stroke="#7a3a1a" stroke-width="3""##,
        );
        let tag = row_tag(house.row);
        let lot = row_lot.entry(tag).or_default();
        let name = format!(
            "{tag}{lot} {}",
            plan_letter(style_design(house.plan.style).0)
        );
        house_labels.push(name.clone());
        *lot += 1;
        svg.text(
            at + DVec2::new(-1.6, 0.4),
            11.0,
            &name,
            r##"fill="#3a1a0a" font-weight="bold""##,
        );
    }

    // Pickups and spawn pads.
    for p in SENDA_PICKUPS.iter() {
        let at = DVec2::new(p.x, p.z);
        svg.circle(
            at,
            0.9,
            r##"fill="#ffffff" stroke="#c000c0" stroke-width="2.5""##,
        );
        svg.text(
            at + DVec2::new(1.1, 0.4),
            11.0,
            &format!("{:?}", p.kind),
            r##"fill="#900090""##,
        );
    }
    for team in [Team::Blue, Team::Red] {
        let color = if team == Team::Blue {
            "#1060ff"
        } else {
            "#e02030"
        };
        for spawn in spawn_positions(team, 1.0) {
            let at = DVec2::new(spawn.x, spawn.z);
            svg.circle(
                at,
                SPAWN_PAD,
                &format!(
                    r##"fill="{color}" fill-opacity="0.3" stroke="{color}" stroke-width="2""##
                ),
            );
        }
    }
    // The level's wall line (inside face) over everything.
    svg.polygon(
        &boundary,
        r##"fill="none" stroke="#b02020" stroke-width="2""##,
    );
    writeln!(svg.body, "</g>").unwrap();

    // Scale references: a Bruiser hull and a 10 m bar, bottom left.
    let hull = tank_hull(VehicleKind::Balanced).size;
    let corner = DVec2::new(x0 + 4.0, z1 - 12.0);
    svg.polygon(
        &[
            corner,
            corner + DVec2::new(hull.z, 0.0),
            corner + DVec2::new(hull.z, hull.x),
            corner + DVec2::new(0.0, hull.x),
        ],
        r##"fill="#556b2f" stroke="#000" stroke-width="1""##,
    );
    svg.text(
        corner + DVec2::new(hull.z + 0.8, hull.x * 0.7),
        12.0,
        &format!(
            "Bruiser hull {:.2} × {:.2} m (length × width)",
            hull.z, hull.x
        ),
        r##"fill="#000""##,
    );
    let bar = DVec2::new(x0 + 4.0, z1 - 6.0);
    for k in 0..10 {
        let a = bar + DVec2::new(f64::from(k), 0.0);
        svg.polygon(
            &[
                a,
                a + DVec2::new(1.0, 0.0),
                a + DVec2::new(1.0, 0.6),
                a + DVec2::new(0.0, 0.6),
            ],
            if k % 2 == 0 {
                r##"fill="#000""##
            } else {
                r##"fill="#fff" stroke="#000" stroke-width="0.5""##
            },
        );
    }
    svg.text(
        bar + DVec2::new(10.6, 0.6),
        12.0,
        "10 m (10 px per metre)",
        r##"fill="#000""##,
    );
    svg.text(
        DVec2::new(x0 + 4.0, z1 - 2.0),
        13.0,
        "+x points right (east), +z points down (south). Origin = arena centre. Dashed black square = arena edge (±60 m); red line = level wall's inside face.",
        r##"fill="#000""##,
    );

    // Legend, top right.
    let legend: &[(&str, &str)] = &[
        ("#6d6d6d", "asphalt, kerb to kerb (cul-de-sacs too)"),
        (
            "#b5cf8e",
            "parkway / planting strip (Way south: groundcover strip)",
        ),
        ("#c9c4b8", "sidewalk (Way south: wide path)"),
        ("#7fae52", "median, splitter and cul-de-sac islands"),
        ("#e8b48a", "house walls (tick = front; dashed = eaves)"),
        ("#d8d0c0", "driveway (garage door + 0.15 m, to kerb)"),
        ("#8f7762", "lot wall: block"),
        ("#4a3020", "lot wall: perimeter block"),
        ("#222222", "view fence (lot or level wall)"),
        ("#8b5a2b", "lot wall: gate return"),
        ("#a8916a", "level wall cover box (block)"),
        ("#b02020", "level wall inside face (line)"),
        ("#d9c9a7", "patio loggia / pergola"),
        ("#3c9a3c", "street (parkway) tree, crown"),
        (
            "#1f7a7a",
            "Senda Parkway tree (median/strips; dashed = outside level)",
        ),
        ("#2e5e1e", "greenbelt tree; dashed line = greenbelt lane"),
        ("#8a9a20", "cul-de-sac island tree"),
        ("#6b4f1d", "bank / slope-band oak"),
        ("#f0c020", "street lamp (hollow = scenery only)"),
        ("#2d5c8a", "wheelie bins"),
        ("#c000c0", "pickup"),
        ("#1060ff", "blue spawn pad (r 2.3 m)"),
        ("#e02030", "red spawn pad"),
    ];
    // Bottom right, below the arena.
    let lx = width - 470.0;
    let ly = height - (legend.len() * 18 + 34) as f64 - 10.0;
    writeln!(
        svg.body,
        r##"<rect x="{:.0}" y="{ly:.0}" width="460" height="{}" fill="#fff" fill-opacity="0.92" stroke="#000"/>"##,
        lx - 10.0,
        legend.len() * 18 + 34
    )
    .unwrap();
    writeln!(
        svg.body,
        r##"<text x="{lx}" y="{:.0}" font-size="14" font-weight="bold" font-family="Helvetica,Arial,sans-serif">Neighborhood (Senda) as built — 10 px/m</text>"##,
        ly + 20.0
    )
    .unwrap();
    for (k, (color, label)) in legend.iter().enumerate() {
        let y = ly as usize + 38 + k * 18;
        writeln!(
            svg.body,
            r##"<rect x="{lx}" y="{}" width="22" height="12" fill="{color}" stroke="#000" stroke-width="0.5"/><text x="{}" y="{}" font-size="12" font-family="Helvetica,Arial,sans-serif">{label}</text>"##,
            y - 10,
            lx + 30.0,
            y
        )
        .unwrap();
    }
    let svg_text = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<!-- 10 px per metre; world x = px / 10 + {x0}, world z = py / 10 + {z0}; +x right (east), +z down (south) -->
{}</svg>
"#,
        svg.body
    );
    std::fs::write(out.join("plan.svg"), svg_text).unwrap();

    // ---- Measurements ----
    let mut md = String::new();
    let w = &mut md;
    writeln!(w, "# Neighborhood (Senda) — dimensions as built\n").unwrap();
    writeln!(
        w,
        "Measured by `crates/core/examples/neighborhood_plan.rs` from `senda_design_layout()`, `senda_houses()`, `senda_lots()`, `senda_boundary()` and the street lines. The level's design metres throughout (it stands `SENDA_SCALE` times larger in the game); +x east, +z south; origin the arena's centre.\n"
    )
    .unwrap();

    // Arena.
    let area = {
        let n = boundary.len();
        (0..n)
            .map(|i| boundary[i].perp_dot(boundary[(i + 1) % n]))
            .sum::<f64>()
            .abs()
            / 2.0
    };
    let (blo, bhi) = boundary.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), &p| (lo.min(p), hi.max(p)),
    );
    writeln!(w, "## Arena\n").unwrap();
    writeln!(w, "| | value |\n|---|---|").unwrap();
    writeln!(w, "| arena square (`ARENA` half side) | x and z −{ARENA:.0} to +{ARENA:.0} m (120 × 120 m, 14,400 m²) |").unwrap();
    writeln!(
        w,
        "| playable area = inside face of the level wall, x | {:.2} to {:.2} m |",
        blo.x, bhi.x
    )
    .unwrap();
    writeln!(w, "| playable area, z | {:.2} to {:.2} m |", blo.y, bhi.y).unwrap();
    writeln!(
        w,
        "| playable area (polygon, {} corners) | {:.0} m² ({:.0}% of the square) |",
        boundary.len(),
        area,
        area / 144.0
    )
    .unwrap();
    writeln!(
        w,
        "| drawing extent (plan.svg) | x {x0} to {x1}, z {z0} to {z1} |\n"
    )
    .unwrap();

    // Streets.
    writeln!(w, "## Streets\n").unwrap();
    writeln!(w, "| street | trace length | length inside arena square | length inside level wall | kerb to kerb | strip each side | sidewalk each side | cul-de-sac kerb radius / island radius |").unwrap();
    writeln!(w, "|---|---|---|---|---|---|---|---|").unwrap();
    for (street, line) in &lines {
        let (mut in_square, mut in_wall) = (0.0, 0.0);
        let step = 0.05;
        let mut s = step / 2.0;
        while s < line.length() {
            let p = line.at(s).0;
            if p.x.abs() <= ARENA && p.y.abs() <= ARENA {
                in_square += step;
            }
            if inside_boundary(&boundary, p) {
                in_wall += step;
            }
            s += step;
        }
        let (strip, walk) = if *street == Street::SendaParkway {
            (
                format!("N {PARKWAY:.1} / S {WAY_PARKWAY:.1} (groundcover)"),
                format!("N {SIDEWALK:.1} / S {WAY_PATH:.1} (wide path)"),
            )
        } else {
            (format!("{PARKWAY:.1}"), format!("{SIDEWALK:.1}"))
        };
        let bulb = street.cul_de_sac().map_or("–".to_string(), |_| {
            format!(
                "{CUL_DE_SAC_RADIUS:.1} / {CUL_DE_SAC_ISLAND:.1} (sidewalk ring {:.1}–{:.1})",
                CUL_DE_SAC_RADIUS + PARKWAY,
                CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK
            )
        });
        writeln!(
            w,
            "| {:?} | {:.1} m | {:.1} m | {:.1} m | {:.1} m | {strip} m | {walk} m | {bulb} |",
            street,
            line.length(),
            in_square,
            in_wall,
            2.0 * street.half()
        )
        .unwrap();
    }
    let neblina_len = neblina.length();
    let _ = neblina_len;
    writeln!(w).unwrap();
    writeln!(w, "Senda Parkway: carriageway {:.1} m per direction ({:.1} m kerb to kerb including the median), median {:.1} m wide, open across Neblina's mouth; splitter island in Neblina's mouth {:.1} m long, 1.2 m wide at its round end. Its north side is {} of the trace.\n",
        Street::SendaParkway.half() - MEDIAN_HALF,
        2.0 * Street::SendaParkway.half(),
        2.0 * MEDIAN_HALF,
        to - from,
        if north_side > 0.0 { "left" } else { "right" }).unwrap();
    writeln!(w, "Street widths, strips and sidewalks are the layout's constants: the paving is drawn from them by the render-only scenery, so there is no separate built value to differ.\n").unwrap();

    // Houses per row.
    writeln!(w, "## House rows\n").unwrap();
    writeln!(w, "Pitch = centre to centre of consecutive houses in the row. Side gap = least distance between neighbours' wall rectangles. Front setback = front-wall middle to its own street's kerb (or the cul-de-sac kerb if nearer). Back yard = house back wall to the lot's back wall line (`Lot::rear_v`); min / median / max.\n").unwrap();
    writeln!(w, "| row | houses | pitch (m) | side-wall gap (m) | kerb → front wall (m) | back of sidewalk → front wall (m) | back yard depth (m) | kerb → back wall (m) |").unwrap();
    writeln!(w, "|---|---|---|---|---|---|---|---|").unwrap();
    let mut surprises: Vec<String> = Vec::new();
    let mut csv = String::from(
        "house,garage,x,z,front_faces_bearing_deg,front_w,depth,wall_top,ridge,kerb_to_front,back_yard,setback_field,pitch_to_prev,gap_to_prev\n",
    );
    for row in Row::ALL {
        let members: Vec<usize> = (0..houses.len())
            .filter(|&k| houses[k].row == row)
            .collect();
        let street = row_street(row);
        let line = street.line();
        let (mut pitch, mut gap, mut kerb, mut walk, mut yard, mut depth) =
            (vec![], vec![], vec![], vec![], vec![], vec![]);
        let label_of = |k: usize| house_labels[k].clone();
        for (n, &k) in members.iter().enumerate() {
            let house = &houses[k];
            let (corners, front, _) = house_frame(house);
            let mut to_kerb = line.distance(front) - street.half();
            if let Some(middle) = street.cul_de_sac() {
                to_kerb = to_kerb.min(middle.distance(front) - CUL_DE_SAC_RADIUS);
            }
            kerb.push(to_kerb);
            walk.push(to_kerb - PARKWAY - SIDEWALK);
            let lot = &lots[k];
            yard.push(house_back(lot) - lot.rear_v());
            depth.push(to_kerb + lot.half_depth - lot.rear_v());
            let design = house.design();
            let frames = wall_frames(design.w, design.d);
            let roof = design.roof();
            let (mut row_pitch, mut row_gap) = (f64::NAN, f64::NAN);
            if n > 0 {
                let prev = &houses[members[n - 1]];
                row_pitch = DVec2::new(prev.x, prev.z).distance(DVec2::new(house.x, house.z));
                row_gap = quad_distance(&house_frame(prev).0, &corners);
                pitch.push(row_pitch);
                gap.push(row_gap);
            }
            writeln!(
                csv,
                "{},{:?},{:.2},{:.2},{:.1},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}",
                label_of(k),
                house.plan.garage,
                house.x,
                house.z,
                {
                    let out = house_frame(house).2;
                    out.x.atan2(-out.y).to_degrees().rem_euclid(360.0)
                },
                2.0 * frames[design.door_wall].3,
                2.0 * frames[if design.door_wall == 0 { 2 } else { 0 }].3,
                roof.wall,
                roof.ridge,
                to_kerb,
                house_back(lot) - lot.rear_v(),
                house.setback,
                row_pitch,
                row_gap
            )
            .unwrap();
        }
        writeln!(
            w,
            "| {:?} | {} | {} | {} | {} | {} | {} | {} |",
            row,
            members.len(),
            stats(&mut pitch),
            stats(&mut gap),
            stats(&mut kerb),
            stats(&mut walk),
            stats(&mut yard),
            stats(&mut depth)
        )
        .unwrap();
    }
    writeln!(w).unwrap();

    // Garage doors, turning circles and corner lots.
    writeln!(w, "## Garage doors, turning circles and corners\n").unwrap();
    for street in Street::ALL {
        if let Some(middle) = street.cul_de_sac() {
            let facing: Vec<String> = (0..houses.len())
                .filter(|&k| {
                    let (_, front, out) = house_frame(&houses[k]);
                    (middle - front).normalize().dot(out) > 0.97
                        && middle.distance(front) < CUL_DE_SAC_RADIUS + 5.0
                })
                .map(|k| house_labels[k].clone())
                .collect();
            writeln!(
                w,
                "- {street:?} turning circle: middle ({:.2}, {:.2}), kerb radius {CUL_DE_SAC_RADIUS:.1}, island {CUL_DE_SAC_ISLAND:.1}; homes facing it: {}",
                middle.x,
                middle.y,
                facing.join(", ")
            )
            .unwrap();
        }
    }
    writeln!(w).unwrap();
    writeln!(w, "Garage door = middle of the garage door to the kerb it faces (street or circle). Side street = least distance from any wall corner to another street's sidewalk back edge (Senda Parkway's south side: its wide path's).\n").unwrap();
    writeln!(
        w,
        "| house | garage door → kerb (m) | walls → other street's sidewalk (m) |"
    )
    .unwrap();
    writeln!(w, "|---|---|---|").unwrap();
    let mut doors = Vec::new();
    for (k, house) in houses.iter().enumerate() {
        let design = house.design();
        let (centre, out, along, _) = wall_frames(design.w, design.d)[design.door_wall];
        let plan = design.wall_plan(design.door_wall);
        let Some(garage) = plan.garage() else {
            continue;
        };
        let door = design.to_world(centre + along * garage);
        let door = DVec2::new(house.x + door.x, house.z + door.z);
        let out = design.to_world(out);
        let out = DVec2::new(out.x, out.z).normalize();
        // March out from the door to the first kerb.
        let paved = |p: DVec2| {
            Street::ALL.into_iter().any(|street| {
                street.line().distance(p) < street.half()
                    || street
                        .cul_de_sac()
                        .is_some_and(|m| m.distance(p) < CUL_DE_SAC_RADIUS)
            })
        };
        let mut t = 0.0;
        while t < 20.0 && !paved(door + out * t) {
            t += 0.01;
        }
        doors.push(t);
        let own = row_street(house.row);
        let side = Street::ALL
            .into_iter()
            .filter(|&street| street != own)
            .flat_map(|street| {
                house_frame(house)
                    .0
                    .map(|corner| past_sidewalk(street, corner))
            })
            .fold(f64::INFINITY, f64::min);
        writeln!(w, "| {} | {t:.2} | {side:.2} |", house_labels[k]).unwrap();
    }
    writeln!(
        w,
        "\nGarage door → kerb over all homes: {}\n",
        stats(&mut doors)
    )
    .unwrap();

    // Plans.
    writeln!(w, "## House plans\n").unwrap();
    writeln!(w, "Footprint = front wall length × depth of the walls (eaves overhang beyond). Heights from `HouseDesign::roof()`: wall top (eave line on the walls) and ridge, above ground.\n").unwrap();
    writeln!(
        w,
        "| plan | count | footprints w × d (m) | wall top (m) | ridge (m) | eave overhang (m) |"
    )
    .unwrap();
    writeln!(w, "|---|---|---|---|---|---|").unwrap();
    for plan in [Plan::Small, Plan::Medium, Plan::Large] {
        let mut sizes: BTreeMap<String, usize> = BTreeMap::new();
        let (mut walls, mut ridges, mut overhangs) = (vec![], vec![], vec![]);
        let mut count = 0;
        for house in houses
            .iter()
            .filter(|h| style_design(h.plan.style).0 == plan)
        {
            let design = house.design();
            let frames = wall_frames(design.w, design.d);
            let front = 2.0 * frames[design.door_wall].3;
            let depth = 2.0 * frames[if design.door_wall == 0 { 2 } else { 0 }].3;
            *sizes.entry(format!("{front:.1} × {depth:.1}")).or_default() += 1;
            let roof = design.roof();
            walls.push(roof.wall);
            ridges.push(roof.ridge);
            overhangs.push(roof.overhang);
            count += 1;
        }
        let sizes = sizes
            .iter()
            .map(|(s, n)| format!("{s} (×{n})"))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            w,
            "| {:?} | {count} | {sizes} | {} | {} | {} |",
            plan,
            range(&walls),
            range(&ridges),
            range(&overhangs)
        )
        .unwrap();
    }
    writeln!(w).unwrap();

    // Trees.
    writeln!(w, "## Trees (cover trees inside the level)\n").unwrap();
    writeln!(w, "Trunk radius = `tree_proportions().radius` (the stump collider is 1.25× it). Crown radius = half the larger horizontal side of the built `tree_model(Full)` bounds (whole model, so it includes any trunk lean). Height = model bounds' height. Cover footprint = the cover box's side.\n").unwrap();
    writeln!(w, "| kind | count | cover footprint (m) | trunk radius (m) | crown radius (m) | height (m) | families |").unwrap();
    writeln!(w, "|---|---|---|---|---|---|---|").unwrap();
    let mut kinds: BTreeMap<TreeKind, Vec<&TreeRow>> = BTreeMap::new();
    for row in &tree_rows {
        kinds.entry(row.0).or_default().push(row);
    }
    for (kind, rows) in &kinds {
        let fams: std::collections::BTreeSet<&str> = rows.iter().map(|r| r.5).collect();
        writeln!(
            w,
            "| {:?} | {} | {} | {} | {} | {} | {} |",
            kind,
            rows.len(),
            range(&rows.iter().map(|r| r.4).collect::<Vec<_>>()),
            range(&rows.iter().map(|r| r.1).collect::<Vec<_>>()),
            range(&rows.iter().map(|r| r.2).collect::<Vec<_>>()),
            range(&rows.iter().map(|r| r.3).collect::<Vec<_>>()),
            fams.into_iter().collect::<Vec<_>>().join(", ")
        )
        .unwrap();
    }
    writeln!(
        w,
        "\nScenery-only Senda Parkway trees outside the level (not cover): {}.\n",
        outer_way_tree_sites().len()
    )
    .unwrap();

    // Walls and lamps.
    writeln!(w, "## Walls and lamps\n").unwrap();
    writeln!(w, "| | height (m) | count |\n|---|---|---|").unwrap();
    for (name, heights) in &wall_heights {
        writeln!(
            w,
            "| {name} runs | {} | {} |",
            range(heights),
            heights.len()
        )
        .unwrap();
    }
    writeln!(
        w,
        "| street lamps (cover) | {} (row {LAMP_HEIGHT}, tall Way {WAY_LAMP_HEIGHT}) | {} |",
        range(&lamp_heights),
        lamp_heights.len()
    )
    .unwrap();
    writeln!(
        w,
        "| lamp sites incl. scenery-only | – | {} |\n",
        lamp_sites().len()
    )
    .unwrap();

    // Tank.
    let scale = 1.95 / REAL_M1_WIDTH;
    writeln!(w, "## Scale reference: Bruiser\n").unwrap();
    writeln!(
        w,
        "| | game m | real-equivalent at the implied scale |\n|---|---|---|"
    )
    .unwrap();
    writeln!(
        w,
        "| hull width (incl. tracks, model bounds x) | {:.2} | {:.2} |",
        hull.x,
        hull.x / scale
    )
    .unwrap();
    writeln!(
        w,
        "| hull length (model bounds z) | {:.2} | {:.2} |",
        hull.z,
        hull.z / scale
    )
    .unwrap();
    writeln!(
        w,
        "| hull height (model bounds y) | {:.2} | {:.2} |",
        hull.y,
        hull.y / scale
    )
    .unwrap();
    writeln!(w, "\nImplied vehicle scale: 1.95 m game width / 3.66 m real M1 width = **{scale:.3}** game m per real m (1 game m = {:.2} real m). The houses, streets and lots are not built at this scale (see the comments on `HOUSE_WIDTH` and the trace's `ALONG_PER_PIXEL`/`ACROSS_PER_PIXEL`).\n", 1.0 / scale).unwrap();

    // Counts.
    let destructible = covers.iter().filter(|c| c.hp.is_finite()).count();
    writeln!(w, "## Totals\n").unwrap();
    writeln!(w, "| | count |\n|---|---|").unwrap();
    writeln!(w, "| houses | {} |", houses.len()).unwrap();
    writeln!(w, "| covers (all) | {} |", covers.len()).unwrap();
    writeln!(w, "| destructible covers (finite hp) | {destructible} |").unwrap();
    writeln!(w, "| cover trees | {} |", tree_rows.len()).unwrap();
    writeln!(w, "| pickups | {} |", SENDA_PICKUPS.len()).unwrap();
    writeln!(
        w,
        "| spawn pads | 10 (5 per team, x = ∓53, z = 0, ±23, ±46) |"
    )
    .unwrap();
    for (name, n) in &counts {
        writeln!(w, "| covers: {name} | {n} |").unwrap();
    }
    writeln!(w).unwrap();

    // Checks for the report.
    let house_quads: Vec<[DVec2; 4]> = houses.iter().map(|h| house_frame(h).0).collect();
    for i in 0..houses.len() {
        for j in i + 1..houses.len() {
            let d = quad_distance(&house_quads[i], &house_quads[j]);
            if d < 1.0 {
                surprises.push(format!(
                    "houses {} and {} stand {d:.2} m apart",
                    house_labels[i], house_labels[j]
                ));
            }
        }
        for (street, line) in &lines {
            let paved = street.half() + PARKWAY + SIDEWALK;
            let near = house_quads[i]
                .iter()
                .map(|&c| line.distance(c))
                .fold(f64::INFINITY, f64::min);
            if near < paved {
                surprises.push(format!(
                    "house {} reaches {:.2} m from {:?}'s centre line (paving to {paved:.1})",
                    house_labels[i], near, street
                ));
            }
        }
    }
    for c in &covers {
        let at = DVec2::new(c.x, c.z);
        if c.kind != CoverKind::Boundary && !inside_boundary(&boundary, at) {
            surprises.push(format!(
                "{:?} cover #{:06x} at ({:.1}, {:.1}) outside the level wall",
                c.kind, c.color, c.x, c.z
            ));
        }
        if c.kind != CoverKind::Boundary && (c.x.abs() > ARENA || c.z.abs() > ARENA) {
            surprises.push(format!(
                "{:?} cover at ({:.1}, {:.1}) outside the arena square",
                c.kind, c.x, c.z
            ));
        }
        if c.kind == CoverKind::Tree {
            for (k, q) in house_quads.iter().enumerate() {
                if quad_gap(&cover_corners(c), q) < 0.0 {
                    surprises.push(format!(
                        "tree at ({:.1}, {:.1}) footprint overlaps house {}",
                        c.x, c.z, house_labels[k]
                    ));
                }
            }
        }
    }
    for p in SENDA_PICKUPS.iter() {
        let at = DVec2::new(p.x, p.z);
        if !inside_boundary(&boundary, at) {
            surprises.push(format!(
                "pickup {:?} at ({}, {}) outside the level wall",
                p.kind, p.x, p.z
            ));
        }
    }
    for (at, kind) in &tree_kinds {
        let _ = (at, kind);
    }
    writeln!(w, "## Automatic checks\n").unwrap();
    if surprises.is_empty() {
        writeln!(w, "No houses closer than 1 m, none on paving, no cover outside the wall or the arena, no tree in a house.\n").unwrap();
    } else {
        for s in &surprises {
            writeln!(w, "- {s}").unwrap();
        }
        writeln!(w).unwrap();
    }
    std::fs::write(out.join("dimensions.md"), md).unwrap();
    std::fs::write(out.join("houses.csv"), csv).unwrap();
    println!(
        "wrote {} ({width}×{height} px)",
        out.join("plan.svg").display()
    );
    println!("wrote {}", out.join("dimensions.md").display());
}
