//! The Neighborhood's reference map: its streets by name, its rows and every house by
//! row and lot number, drawn from the level's own layout so it shows the level as built.
//! `docs/neighborhood-map.svg` is its output; after a layout change, write it again:
//!
//! ```sh
//! cargo build --release --workspace --exclude sloppy-desktop --example neighborhood_map
//! target/release/examples/neighborhood_map docs/neighborhood-map.svg
//! ```
//!
//! North is up (+x east, +z south). `neighborhood_plan` draws the detailed plan with
//! every wall, lamp and pickup; this one is for finding a house and saying which it is.

use std::fmt::Write as _;

use glam::DVec2;
use sloppy_core::sim::arena::spawn_positions;
use sloppy_core::sim::house_layout::{Exterior, Plan, REFERENCE_HOUSE, SENDA_STYLES, style_design};
use sloppy_core::sim::senda_layout::{
    CUL_DE_SAC_ISLAND, CUL_DE_SAC_RADIUS, Line, MEDIAN_HALF, PARKWAY, Row, SENDA_MAP, SIDEWALK,
    SendaHouse, Street, bend_park, neblina_bend, senda_boundary, senda_houses,
};
use sloppy_core::sim::senda_yards::{greenbelt_lane, senda_lots};
use sloppy_core::sim::{CoverKind, Team};

/// Pixels per metre, and the world rectangle the map shows (m).
const PX: f64 = 9.0;
const WEST: f64 = -70.0;
const EAST: f64 = 70.0;
const NORTH: f64 = -78.0;
const SOUTH: f64 = 66.0;
/// The legend's panel, right of the map (px).
const PANEL: f64 = 330.0;
/// A spawn pad's radius (m).
const SPAWN_PAD: f64 = 2.3;

const LAND: &str = "#d9d2b4";
const LEVEL: &str = "#c9cf9f";
const ASPHALT: &str = "#5b5d60";
const WALK: &str = "#cfc9bb";
const PLANTING: &str = "#8fae6a";
const WALL: &str = "#7a5a40";
const INK: &str = "#1c1c1c";
const GOLD: &str = "#ffcf2e";
const FONT: &str = "Helvetica,Arial,sans-serif";

fn px(v: DVec2) -> (f64, f64) {
    ((v.x - WEST) * PX, (v.y - NORTH) * PX)
}

fn points(line: &[DVec2]) -> String {
    line.iter()
        .map(|&v| {
            let (x, y) = px(v);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A stroke `width` metres wide along `line`.
fn band(out: &mut String, line: &[DVec2], width: f64, color: &str) {
    writeln!(
        out,
        r#"<polyline points="{}" fill="none" stroke="{color}" stroke-width="{:.1}" stroke-linejoin="round" stroke-linecap="butt"/>"#,
        points(line),
        width * PX
    )
    .unwrap();
}

fn disc(out: &mut String, at: DVec2, radius: f64, style: &str) {
    let (x, y) = px(at);
    writeln!(
        out,
        r#"<circle cx="{x:.1}" cy="{y:.1}" r="{:.1}" {style}/>"#,
        radius * PX
    )
    .unwrap();
}

/// Text centred at `at` (map px), light with a dark outline or dark with a light one.
fn label(out: &mut String, (x, y): (f64, f64), size: f64, text: &str, light: bool, extra: &str) {
    let (fill, halo) = if light {
        ("#ffffff", INK)
    } else {
        (INK, "#ffffff")
    };
    writeln!(
        out,
        r#"<text x="{x:.1}" y="{y:.1}" font-size="{size}" font-family="{FONT}" font-weight="700" text-anchor="middle" dominant-baseline="central" fill="{fill}" stroke="{halo}" stroke-width="{:.1}" stroke-linejoin="round" paint-order="stroke" {extra}>{text}</text>"#,
        size * 0.22
    )
    .unwrap();
}

/// `text` written along `line`, centred between `from` and `to` metres along it and
/// turned to the street's heading there, reading left to right.
fn along(out: &mut String, line: &Line, (from, to): (f64, f64), size: f64, text: &str) {
    let (at, heading) = line.at((from + to) / 2.0);
    let heading = if heading.x < 0.0 { -heading } else { heading };
    let (x, y) = px(at);
    label(
        out,
        (x, y),
        size,
        text,
        true,
        &format!(
            r#"letter-spacing="{:.1}" transform="rotate({:.1} {x:.1} {y:.1})""#,
            size * 0.16,
            heading.y.atan2(heading.x).to_degrees()
        ),
    );
}

fn row_code(row: Row) -> &'static str {
    match row {
        Row::RumboNorth => "RN",
        Row::RumboSouth => "RS",
        Row::GaviotaNorth => "GN",
        Row::NeblinaWest => "NW",
        Row::GaviotaSouth => "GS",
    }
}

fn row_name(row: Row) -> &'static str {
    match row {
        Row::RumboNorth => "Rumbo north",
        Row::RumboSouth => "Rumbo south",
        Row::GaviotaNorth => "Gaviota north",
        Row::NeblinaWest => "Neblina west",
        Row::GaviotaSouth => "Gaviota south",
    }
}

/// A house's design in short: its plan's size and its exterior's style.
fn design_code(style: u8) -> String {
    let (plan, exterior) = style_design(style);
    let size = match plan {
        Plan::Small => "S",
        Plan::Medium => "M",
        Plan::Large => "L",
    };
    let look = match exterior {
        Exterior::Spanish => "Sp",
        Exterior::Craftsman => "Cr",
        Exterior::Monterey => "Mo",
    };
    format!("{size}·{look}")
}

/// A house's wall corners (front left, front right, back right, back left seen from its
/// street) and the way its front faces.
fn house_corners(house: &SendaHouse) -> ([DVec2; 4], DVec2) {
    let facing = house.facing();
    let across = DVec2::new(facing.y, -facing.x);
    let centre = DVec2::new(house.x, house.z);
    let (hw, hd) = (house.w / 2.0, house.d / 2.0);
    (
        [(-hw, hd), (hw, hd), (hw, -hd), (-hw, -hd)].map(|(u, v)| centre + across * u + facing * v),
        facing,
    )
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: neighborhood_map <out.svg>");
    let houses = senda_houses();
    let lots = senda_lots(&houses);
    let boundary = senda_boundary();
    let (width, height) = ((EAST - WEST) * PX, (SOUTH - NORTH) * PX);
    let mut svg = String::new();
    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{0:.0}" height="{height:.0}" viewBox="0 0 {0:.0} {height:.0}">"#,
        width + PANEL
    )
    .unwrap();
    writeln!(
        svg,
        "<!-- The Neighborhood's streets, rows and lots. Written by crates/core/examples/neighborhood_map.rs from the level's layout: do not edit by hand. {PX} px per metre, north up. -->"
    )
    .unwrap();
    writeln!(
        svg,
        r##"<defs><clipPath id="map"><rect x="0" y="0" width="{width:.0}" height="{height:.0}"/></clipPath></defs>
<rect x="0" y="0" width="{:.0}" height="{height:.0}" fill="#fbfaf5"/>
<rect x="0" y="0" width="{width:.0}" height="{height:.0}" fill="{LAND}"/>
<g clip-path="url(#map)">"##,
        width + PANEL
    )
    .unwrap();

    // The level inside its wall.
    writeln!(
        svg,
        r#"<polygon points="{}" fill="{LEVEL}" stroke="none"/>"#,
        points(&boundary)
    )
    .unwrap();

    // The streets: sidewalks and planting under the asphalt, the turning circles.
    let way = Street::SendaParkway.line();
    for street in Street::ALL {
        let line = street.line();
        let verge = if street == Street::SendaParkway {
            2.0
        } else {
            PARKWAY + SIDEWALK
        };
        band(&mut svg, &line.0, (street.half() + verge) * 2.0, WALK);
        if let Some(middle) = street.cul_de_sac() {
            disc(
                &mut svg,
                middle,
                CUL_DE_SAC_RADIUS + PARKWAY + SIDEWALK,
                &format!(r#"fill="{WALK}""#),
            );
        }
    }
    for street in Street::ALL {
        let line = street.line();
        band(&mut svg, &line.0, street.half() * 2.0, ASPHALT);
        if let Some(middle) = street.cul_de_sac() {
            disc(
                &mut svg,
                middle,
                CUL_DE_SAC_RADIUS,
                &format!(r#"fill="{ASPHALT}""#),
            );
            disc(
                &mut svg,
                middle,
                CUL_DE_SAC_ISLAND,
                &format!(r#"fill="{PLANTING}""#),
            );
        }
    }
    band(&mut svg, &way.0, MEDIAN_HALF * 2.0, PLANTING);

    // The greenbelt's lane between Rumbo's south row and Gaviota's north row.
    let lane = Line(greenbelt_lane());
    writeln!(
        svg,
        r##"<polyline points="{}" fill="none" stroke="#4f7a3a" stroke-width="2.5" stroke-dasharray="9 7"/>"##,
        points(&lane.0)
    )
    .unwrap();

    // The lots, then the cover trees over them.
    for lot in &lots {
        let hw = lot.half_width;
        let (front, rear) = (lot.half_depth + lot.house.setback, lot.rear_v());
        let corners =
            [(-hw, front), (hw, front), (hw, rear), (-hw, rear)].map(|(u, v)| lot.at(u, v));
        writeln!(
            svg,
            r##"<polygon points="{}" fill="#e6dfc8" fill-opacity="0.75" stroke="#8d8672" stroke-width="1"/>"##,
            points(&corners)
        )
        .unwrap();
    }
    for cover in (SENDA_MAP.layout)() {
        if cover.kind == CoverKind::Tree {
            disc(
                &mut svg,
                DVec2::new(cover.x, cover.z),
                cover.w / 2.0,
                r##"fill="#3f8a45" fill-opacity="0.55" stroke="#2c6331" stroke-width="1""##,
            );
        }
    }

    // The spawn pads.
    for (team, fill, edge) in [
        (Team::Blue, "#8fb3ff", "#1f55d6"),
        (Team::Red, "#ff9c9c", "#d62a2a"),
    ] {
        for spawn in spawn_positions(team, 1.0) {
            disc(
                &mut svg,
                DVec2::new(spawn.x, spawn.z),
                SPAWN_PAD,
                &format!(r#"fill="{fill}" stroke="{edge}" stroke-width="2""#),
            );
        }
    }

    // The level's wall.
    writeln!(
        svg,
        r#"<polygon points="{}" fill="none" stroke="{WALL}" stroke-width="5" stroke-linejoin="round"/>"#,
        points(&boundary)
    )
    .unwrap();

    // The houses, roofs in their tile's colour, the front wall drawn heavier.
    let mut counts = [0usize; Row::ALL.len()];
    let mut house_labels = String::new();
    for house in &houses {
        let (corners, _) = house_corners(house);
        let style = SENDA_STYLES[usize::from(house.plan.style) % SENDA_STYLES.len()];
        let reference = house.plan.style == REFERENCE_HOUSE && house.row == Row::RumboNorth;
        let (edge, weight) = if reference {
            (GOLD, 5.0)
        } else {
            ("#3a2e26", 1.5)
        };
        writeln!(
            svg,
            r##"<polygon points="{}" fill="#{:06x}" stroke="{edge}" stroke-width="{weight}" stroke-linejoin="round"/>"##,
            points(&corners),
            style.roof
        )
        .unwrap();
        writeln!(
            svg,
            r##"<polyline points="{}" fill="none" stroke="#2a211b" stroke-width="5" stroke-linecap="butt"/>"##,
            points(&corners[..2])
        )
        .unwrap();
        let slot = Row::ALL.iter().position(|&row| row == house.row).unwrap();
        counts[slot] += 1;
        let lot = counts[slot];
        if reference {
            assert_eq!(lot, 4, "the reference house is Rumbo north lot 4");
        }
        let (x, y) = px(DVec2::new(house.x, house.z));
        label(
            &mut house_labels,
            (x, y - 6.0),
            19.0,
            &format!("{} {lot}", row_code(house.row)),
            true,
            "",
        );
        label(
            &mut house_labels,
            (x, y + 14.0),
            11.5,
            &design_code(house.plan.style),
            true,
            "",
        );
    }

    // Names: the streets along their lines, the open ground where it lies.
    let neblina = Street::NeblinaGaviota.line();
    let rumbo = Street::Rumbo.line();
    let bend = neblina_bend();
    // Where Rumbo leaves Neblina, as far along Neblina.
    let junction = (0..=(bend as usize))
        .map(|s| s as f64)
        .min_by(|&a, &b| {
            let from = rumbo.0[0];
            neblina
                .at(a)
                .0
                .distance(from)
                .total_cmp(&neblina.at(b).0.distance(from))
        })
        .unwrap();
    along(&mut svg, &neblina, (junction + 3.0, bend), 21.0, "NEBLINA");
    along(
        &mut svg,
        &neblina,
        (bend + 6.0, neblina.length() - CUL_DE_SAC_RADIUS),
        21.0,
        "GAVIOTA",
    );
    along(
        &mut svg,
        &rumbo,
        (4.0, rumbo.length() - CUL_DE_SAC_RADIUS),
        21.0,
        "RUMBO",
    );
    // Senda Parkway where the map shows it.
    let shown: Vec<f64> = (0..(way.length() as usize))
        .map(|s| s as f64)
        .filter(|&s| {
            let p = way.at(s).0;
            p.x > WEST + 6.0 && p.x < EAST - 6.0 && p.y > NORTH + 4.0
        })
        .collect();
    if let (Some(&from), Some(&to)) = (shown.first(), shown.last()) {
        along(&mut svg, &way, (from, to), 25.0, "SENDA PARKWAY");
    }
    svg.push_str(&house_labels);

    // A spot `behind` metres past the back wall of the house `share` of the way along a
    // row.
    let behind_row = |row: Row, share: f64, behind: f64| {
        let of_row: Vec<&SendaHouse> = houses.iter().filter(|house| house.row == row).collect();
        let house = of_row[((of_row.len() - 1) as f64 * share).round() as usize];
        DVec2::new(house.x, house.z) - house.facing() * (house.d / 2.0 + behind)
    };
    let places = [
        (behind_row(Row::RumboNorth, 0.5, 12.0), "BANK"),
        (
            behind_row(Row::NeblinaWest, 0.5, 9.0) + DVec2::new(0.0, 8.0),
            "PARK",
        ),
        (bend_park(), "BEND PARK"),
        (lane.at(lane.length() * 0.47).0, "GREENBELT"),
        (behind_row(Row::GaviotaSouth, 0.5, 11.0), "SOUTH SLOPE"),
        (DVec2::new(-4.0, SOUTH - 4.0), "↓ to Gardenia"),
    ];
    for (at, name) in places {
        label(
            &mut svg,
            px(at),
            16.0,
            name,
            false,
            r#"letter-spacing="1.5" font-style="italic""#,
        );
    }
    for (row, share) in [
        (Row::RumboNorth, 0.5),
        (Row::RumboSouth, 0.2),
        (Row::GaviotaNorth, 0.75),
        (Row::NeblinaWest, 0.5),
        (Row::GaviotaSouth, 0.5),
    ] {
        label(
            &mut svg,
            px(behind_row(row, share, 5.0)),
            16.0,
            &format!("{} row", row_name(row)),
            false,
            "",
        );
    }
    svg.push_str("</g>\n");

    // North arrow and scale bar, on the map's open north-west corner.
    let (ax, ay) = (48.0, 70.0);
    writeln!(
        svg,
        r#"<polygon points="{ax},{:.0} {:.0},{:.0} {ax},{:.0} {:.0},{:.0}" fill="{INK}"/>"#,
        ay - 42.0,
        ax + 15.0,
        ay + 8.0,
        ay - 4.0,
        ax - 15.0,
        ay + 8.0
    )
    .unwrap();
    label(&mut svg, (ax, ay + 26.0), 20.0, "N", false, "");
    let (sx, sy, metres) = (26.0, height - 34.0, 20.0);
    writeln!(
        svg,
        r##"<rect x="{sx}" y="{sy}" width="{:.0}" height="8" fill="{INK}"/><rect x="{:.0}" y="{sy}" width="{:.0}" height="8" fill="#ffffff" stroke="{INK}" stroke-width="1"/>"##,
        metres * PX / 2.0,
        sx + metres * PX / 2.0,
        metres * PX / 2.0
    )
    .unwrap();
    label(
        &mut svg,
        (sx + metres * PX / 2.0, sy - 14.0),
        14.0,
        &format!("{metres:.0} m"),
        false,
        "",
    );

    // The legend.
    let left = width + 22.0;
    let y = std::cell::Cell::new(44.0);
    let line = |svg: &mut String, size: f64, weight: u32, text: &str, gap: f64| {
        writeln!(
            svg,
            r#"<text x="{left}" y="{:.0}" font-size="{size}" font-family="{FONT}" font-weight="{weight}" fill="{INK}">{text}</text>"#,
            y.get()
        )
        .unwrap();
        y.set(y.get() + gap);
    };
    line(&mut svg, 27.0, 700, "NEIGHBORHOOD", 26.0);
    line(
        &mut svg,
        14.0,
        400,
        "Streets, rows and lots, as the level is built",
        40.0,
    );
    line(&mut svg, 16.0, 700, "Naming a house", 22.0);
    line(
        &mut svg,
        13.5,
        400,
        "Row, then lot: “Rumbo north lot 4” is RN 4.",
        19.0,
    );
    line(
        &mut svg,
        13.5,
        400,
        "Lots count the houses along their row.",
        19.0,
    );
    line(
        &mut svg,
        13.5,
        400,
        "The heavy edge of a house is its front.",
        36.0,
    );
    line(&mut svg, 16.0, 700, "Rows", 22.0);
    for (slot, row) in Row::ALL.into_iter().enumerate() {
        line(
            &mut svg,
            13.5,
            400,
            &format!(
                "{}  {} row, lots 1 to {}",
                row_code(row),
                row_name(row),
                counts[slot]
            ),
            19.0,
        );
    }
    y.set(y.get() + 17.0);
    line(&mut svg, 16.0, 700, "Designs (under each lot)", 22.0);
    line(
        &mut svg,
        13.5,
        400,
        "S, M, L  small, medium, large plan",
        19.0,
    );
    line(&mut svg, 13.5, 400, "Sp  Spanish", 19.0);
    line(&mut svg, 13.5, 400, "Cr  Craftsman", 19.0);
    line(&mut svg, 13.5, 400, "Mo  Monterey", 19.0);
    line(
        &mut svg,
        13.5,
        400,
        "Roofs are drawn in their tile's colour.",
        36.0,
    );
    line(&mut svg, 16.0, 700, "Marks", 26.0);
    let marks: [(&str, String); 7] = [
        (
            "Reference house (RN 4)",
            format!(r##"<rect x="0" y="-11" width="22" height="14" fill="#8e6c5e" stroke="{GOLD}" stroke-width="4"/>"##),
        ),
        (
            "Lot (house and yards)",
            r##"<rect x="0" y="-11" width="22" height="14" fill="#e6dfc8" stroke="#8d8672" stroke-width="1"/>"##.to_string(),
        ),
        (
            "Tree that is cover",
            r##"<circle cx="11" cy="-4" r="8" fill="#3f8a45" fill-opacity="0.55" stroke="#2c6331" stroke-width="1"/>"##.to_string(),
        ),
        (
            "Blue team's spawn pads",
            r##"<circle cx="11" cy="-4" r="8" fill="#8fb3ff" stroke="#1f55d6" stroke-width="2"/>"##.to_string(),
        ),
        (
            "Red team's spawn pads",
            r##"<circle cx="11" cy="-4" r="8" fill="#ff9c9c" stroke="#d62a2a" stroke-width="2"/>"##.to_string(),
        ),
        (
            "The level's wall",
            format!(r#"<line x1="0" y1="-4" x2="22" y2="-4" stroke="{WALL}" stroke-width="5"/>"#),
        ),
        (
            "Greenbelt lane",
            r##"<line x1="0" y1="-4" x2="22" y2="-4" stroke="#4f7a3a" stroke-width="2.5" stroke-dasharray="6 4"/>"##.to_string(),
        ),
    ];
    for (text, mark) in marks {
        writeln!(
            svg,
            r#"<g transform="translate({left},{:.0})">{mark}<text x="32" y="0" font-size="13.5" font-family="{FONT}" fill="{INK}">{text}</text></g>"#,
            y.get()
        )
        .unwrap();
        y.set(y.get() + 23.0);
    }
    svg.push_str("</svg>\n");
    std::fs::write(&path, svg).expect("write the map");
    println!("{path}: {} houses in {} rows", houses.len(), Row::ALL.len());
}
