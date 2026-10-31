//! Battle Setup's backdrop (`.setup-backdrop` in `battle-setup.html`): a dusk sky with a
//! low sun, a faint floor grid, searchlights sweeping, ridges and pines, two tanks on
//! watch and embers rising, under a vignette. The land is the page's own drawing, laid
//! out as the page lays it (`viewBox="0 0 1600 900"`, `xMidYMax slice`).

use std::sync::OnceLock;

use egui::{
    Color32, Id, LayerId, Mesh, Order, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2,
};

use super::icons::Art;
use super::paint::{self, hex, hexa, mix};

/// The land's view; it covers the screen, its foot on the screen's foot.
const LAND_VIEW: Vec2 = vec2(1600.0, 900.0);

/// A layer of land: its ridge line from left to right, its pines (each a tree of three
/// tiers), and its colour from the top of its shape down to a stop.
struct Land {
    ridge: Vec<Pos2>,
    pines: Vec<[Pos2; PINE_POINTS]>,
    top: f32,
}

/// A pine's outline: the tip, then down its right side tier by tier and up its left.
const PINE_POINTS: usize = 11;

/// The land's three layers' fills: a vertical gradient from `top` at the shape's top to
/// `bottom` at `stop` of its height, solid under it.
struct LandFill {
    top: u32,
    bottom: u32,
    stop: f32,
}

const FAR_FILL: LandFill = LandFill {
    top: 0x3a4f86,
    bottom: 0x22325a,
    stop: 0.5,
};
const MID_FILL: LandFill = LandFill {
    top: 0x1b2a4c,
    bottom: 0x121d36,
    stop: 0.4,
};
const NEAR_FILL: LandFill = LandFill {
    top: 0x0a1222,
    bottom: 0x0a1222,
    stop: 1.0,
};

/// The land read from its paths, once.
fn lands() -> &'static [Land; 3] {
    static LANDS: OnceLock<[Land; 3]> = OnceLock::new();
    LANDS.get_or_init(|| [FAR_RIDGE, MID_RIDGE, NEAR_RIDGE].map(parse_land))
}

/// A layer's path (only `M`, `L` and `Z`): its first outline is the ridge, closed along
/// the view's foot, and the rest are its pines.
fn parse_land(path: &str) -> Land {
    let outlines: Vec<Vec<Pos2>> = path
        .split('Z')
        .filter(|outline| !outline.trim().is_empty())
        .map(|outline| {
            outline
                .split(['M', 'L'])
                .filter_map(|point| {
                    let (x, y) = point.trim().split_once(' ')?;
                    Some(pos2(x.parse().ok()?, y.parse().ok()?))
                })
                .collect()
        })
        .collect();
    let top = outlines
        .iter()
        .flatten()
        .map(|point| point.y)
        .fold(LAND_VIEW.y, f32::min);
    let mut outlines = outlines.into_iter();
    let ridge = outlines
        .next()
        .map(|outline| {
            // Leave out the corners on the view's foot.
            outline[1..outline.len() - 1].to_vec()
        })
        .unwrap_or_default();
    let pines = outlines
        .filter_map(|outline| <[Pos2; PINE_POINTS]>::try_from(outline).ok())
        .collect();
    Land { ridge, pines, top }
}

impl LandFill {
    fn at(&self, land: &Land, y: f32) -> Color32 {
        let stop = land.top + (LAND_VIEW.y - land.top) * self.stop;
        let share = ((y - land.top) / (stop - land.top).max(1.0)).clamp(0.0, 1.0);
        mix(hex(self.top), hex(self.bottom), share)
    }
}

/// Searchlights (`.backdrop-beams i`): their foot above the screen's foot, width at the
/// top and at the foot (a share of it), height as a share of the screen's, and the swing
/// between their angles in degrees; how strong they shine.
const BEAM_FOOT: f32 = 0.16;
const BEAM_WIDTH: f32 = 180.0;
const BEAM_FOOT_SHARE: f32 = 0.08;
const BEAM_HEIGHT: f32 = 1.2;
const BEAM_SWING: (f32, f32) = (-24.0, 18.0);
const BEAM_OPACITY: f32 = 0.55;
/// Each light's distance in from its side of the screen, its sweep's period in seconds
/// and the time it starts into it, and whether it sweeps the other way first.
const BEAMS: [(f32, f32, f32, bool); 2] = [(0.07, 11.0, 0.0, false), (0.93, 13.0, 6.0, true)];

/// Embers (`.backdrop-embers i`): two layers, each three tiled dots (tile size, offset,
/// colour, radius) rising a screen's height in a period (seconds) at an opacity.
type EmberDot = ([f32; 2], [f32; 2], u32, f32);
const EMBERS: [([EmberDot; 3], f32, f32); 2] = [
    (
        [
            ([310.0, 330.0], [20.0, 40.0], 0xffb35c, 1.5),
            ([230.0, 270.0], [140.0, 120.0], 0xffd27a, 1.0),
            ([420.0, 380.0], [60.0, 260.0], 0xff8a3d, 1.2),
        ],
        26.0,
        0.5,
    ),
    (
        [
            ([270.0, 410.0], [100.0, 10.0], 0xffb35c, 1.5),
            ([370.0, 300.0], [10.0, 200.0], 0xffd27a, 1.0),
            ([190.0, 350.0], [170.0, 90.0], 0xff8a3d, 1.2),
        ],
        38.0,
        0.35,
    ),
];

/// The floor (`.backdrop-floor`): its far edge as a share of the screen's height, its
/// grid's cell, the perspective it tilts under and its tilt, its lines' colour, and the
/// share of its depth over which it fades in.
const FLOOR_HORIZON: f32 = 0.6;
const FLOOR_DEPTH: f32 = 0.7;
const FLOOR_CELL: f32 = 64.0;
const FLOOR_PERSPECTIVE: f32 = 500.0;
const FLOOR_TILT: f32 = 62.0;
const FLOOR_LINE: (u32, f32) = (0x4aa8ff, 0.11);
const FLOOR_FADE: (f32, f32) = (0.25, 0.9);

/// The dusk battlefield behind Battle Setup.
pub fn backdrop(context: &egui::Context, art: &Art) {
    // The whole screen, under an iPad's home indicator too.
    let painter = paint::full_screen(
        context,
        LayerId::new(Order::Background, Id::unique("setup backdrop")),
    );
    let screen = context.viewport_rect();
    let time = context.input(|input| input.time);
    sky(&painter, screen);
    beams(&painter, screen, time);
    // The land covers the screen as the page's `slice` does, its foot on the screen's.
    let scale = (screen.width() / LAND_VIEW.x).max(screen.height() / LAND_VIEW.y);
    let origin = pos2(
        screen.center().x - LAND_VIEW.x * scale / 2.0,
        screen.bottom() - LAND_VIEW.y * scale,
    );
    let place = |point: Pos2| origin + point.to_vec2() * scale;
    for (land, fill) in lands().iter().zip([FAR_FILL, MID_FILL, NEAR_FILL]) {
        land_layer(&painter, land, &fill, place);
    }
    // Two tanks on watch, the right one facing left (`.backdrop-tank`).
    let [_, _, tank_width, tank_height] = art.view("backdrop-tank");
    let left = Rect::from_min_size(
        place(pos2(70.0, 742.0)),
        vec2(tank_width, tank_height) * 1.5 * scale,
    );
    art.icon(&painter, "backdrop-tank", left, hex(0x070d19));
    let right = Rect::from_min_size(
        place(pos2(1530.0 - tank_width * 1.7, 736.0)),
        vec2(tank_width, tank_height) * 1.7 * scale,
    );
    art.icon_flipped(&painter, "backdrop-tank", right, hex(0x070d19));
    floor(&painter, screen);
    embers(&painter, screen, time);
    paint::vignette(
        &painter,
        screen,
        vec2(0.8, 0.8),
        &[(0.55, Color32::TRANSPARENT), (1.0, hexa(0x02050b, 0.8))],
        1.0,
    );
}

/// The sky: night overhead to a warm horizon, a glow at the top and the low sun.
fn sky(painter: &Painter, screen: Rect) {
    painter.add(paint::vgradient(
        screen,
        &[
            (0.0, hex(0x060c1a)),
            (0.42, hex(0x0f2347)),
            (0.6, hex(0x2a3d6b)),
            (0.7, hex(0x5a4a6a)),
            (0.78, hex(0x1a2338)),
            (1.0, hex(0x1a2338)),
        ],
    ));
    let at = |x: f32, y: f32| {
        pos2(
            screen.left() + screen.width() * x,
            screen.top() + screen.height() * y,
        )
    };
    painter.add(paint::radial(
        at(0.5, 0.0),
        vec2(screen.width() * 0.7, screen.height() * 0.6),
        &[(0.0, hexa(0x1d3f7a, 0.4)), (0.7, Color32::TRANSPARENT)],
    ));
    painter.add(paint::radial(
        at(0.5, 0.7),
        vec2(screen.width() * 0.9, screen.height() * 0.45),
        &[(0.0, hexa(0xff7a3d, 0.18)), (0.75, Color32::TRANSPARENT)],
    ));
    painter.add(paint::radial(
        at(0.5, 0.66),
        vec2(screen.width() * 0.55, screen.height() * 0.3),
        &[(0.0, hexa(0xffb35c, 0.33)), (0.7, Color32::TRANSPARENT)],
    ));
}

/// A layer of land: the ridge filled to the view's foot in its gradient, and its pines.
fn land_layer(painter: &Painter, land: &Land, fill: &LandFill, place: impl Fn(Pos2) -> Pos2) {
    let stop = land.top + (LAND_VIEW.y - land.top) * fill.stop;
    let mut mesh = Mesh::default();
    // Each column of the ridge: its top, the gradient's stop where that is lower, and
    // the foot, so the colour turns where the page's does.
    for (index, point) in land.ridge.iter().enumerate() {
        for y in [point.y, point.y.max(stop), LAND_VIEW.y] {
            mesh.colored_vertex(place(pos2(point.x, y)), fill.at(land, y));
        }
        if index > 0 {
            let base = (index as u32 - 1) * 3;
            for row in 0..2 {
                let (a, b) = (base + row, base + 3 + row);
                mesh.add_triangle(a, b, a + 1);
                mesh.add_triangle(b, b + 1, a + 1);
            }
        }
    }
    painter.add(Shape::mesh(mesh));
    // A pine's three tiers, each a convex piece: the top triangle and two trapezoids.
    for pine in &land.pines {
        let color = fill.at(land, (pine[0].y + pine[5].y) / 2.0);
        for tier in [[0, 1, 10, 10], [9, 2, 3, 8], [7, 4, 5, 6]] {
            let mut points: Vec<Pos2> = tier.iter().map(|&index| place(pine[index])).collect();
            points.dedup();
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
    }
}

/// CSS `ease-in-out` closely enough for a slow sweep.
fn ease_in_out(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Two searchlights sweeping from behind the ridges, brightest at their foot.
fn beams(painter: &Painter, screen: Rect, time: f64) {
    for (across, period, start, reverse) in BEAMS {
        // Back and forth over the swing (`alternate`), the second the other way first.
        let phase = ((time as f32 + start) / period).rem_euclid(2.0);
        let mut sweep = if phase <= 1.0 { phase } else { 2.0 - phase };
        if reverse {
            sweep = 1.0 - sweep;
        }
        let angle =
            (BEAM_SWING.0 + (BEAM_SWING.1 - BEAM_SWING.0) * ease_in_out(sweep)).to_radians();
        let foot = pos2(
            screen.left() + screen.width() * across,
            screen.bottom() - screen.height() * BEAM_FOOT,
        );
        let length = screen.height() * BEAM_HEIGHT;
        // Up the beam's axis, turned clockwise by `angle`, and across it.
        let up = vec2(angle.sin(), -angle.cos());
        let side = vec2(angle.cos(), angle.sin());
        let mut mesh = Mesh::default();
        for (along, alpha) in [(0.0, 0.25), (0.45, 0.063), (0.8, 0.0), (1.0, 0.0)] {
            let half = BEAM_WIDTH * (BEAM_FOOT_SHARE + (1.0 - BEAM_FOOT_SHARE) * along) / 2.0;
            let center = foot + up * length * along;
            let color = hexa(0xbfe3ff, alpha * BEAM_OPACITY);
            mesh.colored_vertex(center - side * half, color);
            mesh.colored_vertex(center + side * half, color);
        }
        for row in 0..3 {
            let a = row * 2;
            mesh.add_triangle(a, a + 1, a + 2);
            mesh.add_triangle(a + 1, a + 3, a + 2);
        }
        painter.add(Shape::mesh(mesh));
    }
}

/// Embers drifting up the screen in two tiled layers.
fn embers(painter: &Painter, screen: Rect, time: f64) {
    for (dots, period, opacity) in EMBERS {
        // The layer is twice the screen's height and rises one screen's height a period.
        let rise = (time as f32 / period).fract() * screen.height();
        for (tile, offset, color, radius) in dots {
            let color = hexa(color, opacity);
            let mut y = offset[1] + tile[1] / 2.0 - rise;
            while y < screen.height() + radius {
                if y > -radius {
                    let mut x = offset[0] + tile[0] / 2.0;
                    while x < screen.width() + radius {
                        let center = screen.min + vec2(x, y);
                        // The dot's soft edge, then its core.
                        painter.circle_filled(center, radius + 1.0, color.gamma_multiply(0.35));
                        painter.circle_filled(center, radius, color);
                        x += tile[0];
                    }
                }
                y += tile[1];
            }
        }
    }
}

/// The floor: a grid laid flat from the horizon toward the viewer, fading in from the
/// distance.
fn floor(painter: &Painter, screen: Rect) {
    let horizon = screen.top() + screen.height() * FLOOR_HORIZON;
    let depth = screen.height() * FLOOR_DEPTH;
    let (sin, cos) = FLOOR_TILT.to_radians().sin_cos();
    // A point on the floor, `x` from the screen's middle and `y` from its far edge, as
    // the tilted plane projects it; none at or behind the eye.
    let project = |x: f32, y: f32| {
        let reach = FLOOR_PERSPECTIVE - y * sin;
        (reach > 1.0).then(|| {
            let scale = FLOOR_PERSPECTIVE / reach;
            pos2(screen.center().x + x * scale, horizon + y * cos * scale)
        })
    };
    let strength = |y: f32| {
        let (from, to) = (depth * FLOOR_FADE.0, depth * FLOOR_FADE.1);
        ((y - from) / (to - from)).clamp(0.0, 1.0) * FLOOR_LINE.1
    };
    let line = |a: Pos2, b: Pos2, alpha: f32| {
        if alpha > 0.002 {
            painter.line_segment([a, b], Stroke::new(1.0, hexa(FLOOR_LINE.0, alpha)));
        }
    };
    // How deep the floor shows before it passes under the screen's foot.
    let mut near = 0.0;
    while let Some(point) = project(0.0, near + 4.0) {
        if point.y > screen.bottom() || near > depth {
            break;
        }
        near += 4.0;
    }
    let half_width = screen.width();
    // Across: one line a cell.
    let mut y = FLOOR_CELL;
    while y <= near {
        if let (Some(left), Some(right)) =
            (project(-half_width * 4.0, y), project(half_width * 4.0, y))
        {
            line(left, right, strength(y));
        }
        y += FLOOR_CELL;
    }
    // Along: each line in short pieces, so it fades as it nears.
    let mut x = -half_width;
    while x <= half_width {
        let mut from = depth * FLOOR_FADE.0;
        while from < near {
            let to = (from + FLOOR_CELL / 2.0).min(near);
            if let (Some(a), Some(b)) = (project(x, from), project(x, to)) {
                line(a, b, strength((from + to) / 2.0));
            }
            from = to;
        }
        x += FLOOR_CELL;
    }
}

/// The land's layers, far to near, as the page draws them (`.backdrop-land`).
const FAR_RIDGE: &str = "\
    M-20 900L-20 525L20 554L60 540L100 519L140 535L180 538L220 526L260 543L300 561\
    L340 564L380 577L420 587L460 609L500 591L540 582L580 597L620 610L660 579L700 592\
    L740 590L780 616L820 616L860 590L900 591L940 616L980 616L1020 616L1060 616L1100 616\
    L1140 616L1180 607L1220 596L1260 616L1300 604L1340 580L1380 566L1420 561L1460 546\
    L1500 577L1540 581L1580 597L1620 616L1620 900Z";
const MID_RIDGE: &str = "\
    M-20 900L-20 642L10 633L40 636L70 636L100 633L130 621L160 637L190 619L220 614\
    L250 608L280 614L310 631L340 632L370 618L400 615L430 606L460 600L490 597L520 591\
    L550 592L580 597L610 590L640 606L670 605L700 619L730 624L760 630L790 631L820 631\
    L850 638L880 656L910 652L940 660L970 650L1000 654L1030 648L1060 641L1090 636\
    L1120 648L1150 663L1180 648L1210 661L1240 644L1270 640L1300 626L1330 609L1360 629\
    L1390 618L1420 623L1450 609L1480 607L1510 611L1540 596L1570 613L1600 629L1620 900Z\
    M1441 634L1453 663L1448 663L1459 687L1452 687L1464 699L1417 699L1429 687L1422 687\
    L1433 663L1428 663ZM1228 649L1236 668L1233 668L1240 683L1236 683L1243 690L1213 690\
    L1221 683L1216 683L1224 668L1220 668ZM387 657L394 672L391 672L397 684L393 684\
    L399 690L375 690L381 684L378 684L384 672L381 672ZM992 658L999 676L996 676L1003 690\
    L999 690L1006 697L977 697L985 690L980 690L987 676L984 676ZM783 635L794 661L789 661\
    L799 682L793 682L803 692L762 692L772 682L766 682L776 661L771 661ZM1427 654L1434 672\
    L1431 672L1438 687L1434 687L1441 694L1412 694L1420 687L1415 687L1422 672L1419 672Z\
    M1447 660L1453 676L1450 676L1457 689L1453 689L1459 695L1434 695L1440 689L1437 689\
    L1443 676L1440 676ZM-15 630L-2 660L-8 660L4 684L-3 684L9 696L-39 696L-27 684L-34 684\
    L-22 660L-28 660ZM1492 668L1498 682L1496 682L1501 693L1498 693L1503 699L1481 699\
    L1487 693L1483 693L1489 682L1486 682ZM289 627L302 658L296 658L308 683L301 683\
    L313 696L264 696L276 683L269 683L281 658L275 658ZM1094 652L1102 670L1099 670\
    L1106 685L1101 685L1109 692L1080 692L1087 685L1082 685L1090 670L1086 670ZM323 638\
    L333 662L328 662L338 681L332 681L342 691L304 691L313 681L308 681L317 662L312 662Z\
    M1242 639L1253 665L1248 665L1259 687L1252 687L1263 697L1221 697L1231 687L1225 687\
    L1236 665L1230 665ZM797 660L803 674L800 674L806 685L803 685L808 691L786 691L791 685\
    L788 685L794 674L791 674ZM1487 662L1494 678L1491 678L1497 691L1494 691L1500 697\
    L1475 697L1481 691L1477 691L1483 678L1480 678ZM1445 667L1451 682L1448 682L1454 693\
    L1450 693L1456 699L1433 699L1439 693L1435 693L1441 682L1438 682ZM1393 630L1406 661\
    L1400 661L1413 687L1405 687L1418 700L1368 700L1380 687L1373 687L1385 661L1379 661Z\
    M1234 646L1244 667L1239 667L1248 684L1243 684L1251 692L1218 692L1226 684L1221 684\
    L1229 667L1225 667ZM1173 629L1186 658L1180 658L1192 682L1185 682L1197 694L1150 694\
    L1162 682L1155 682L1166 658L1160 658ZM365 653L374 673L370 673L378 691L373 691\
    L382 699L348 699L357 691L352 691L360 673L356 673ZM896 652L905 672L901 672L909 688\
    L904 688L912 696L881 696L888 688L884 688L892 672L888 672ZM1185 654L1194 673L1190 673\
    L1198 689L1193 689L1201 697L1170 697L1177 689L1173 689L1181 673L1177 673ZM673 655\
    L681 672L677 672L684 686L680 686L687 693L660 693L666 686L662 686L669 672L666 672Z\
    M1596 627L1610 658L1604 658L1616 684L1609 684L1621 696L1571 696L1584 684L1576 684\
    L1589 658L1582 658ZM1410 625L1424 655L1418 655L1430 680L1422 680L1434 692L1386 692\
    L1398 680L1391 680L1403 655L1397 655ZM1038 660L1045 675L1042 675L1048 689L1045 689\
    L1051 695L1025 695L1032 689L1028 689L1034 675L1031 675ZM894 639L906 666L901 666\
    L912 689L905 689L916 699L872 699L883 689L877 689L888 666L882 666ZM771 657L778 674\
    L775 674L781 688L777 688L784 694L757 694L764 688L760 688L767 674L763 674ZM252 639\
    L264 664L259 664L269 685L263 685L273 695L232 695L242 685L236 685L246 664L241 664Z\
    M901 647L911 670L906 670L915 689L910 689L919 698L883 698L892 689L886 689L895 670\
    L891 670ZM574 630L586 657L581 657L592 679L585 679L596 690L553 690L564 679L557 679\
    L568 657L563 657ZM470 640L480 664L476 664L485 683L479 683L489 692L451 692L461 683\
    L455 683L465 664L460 664ZM1166 651L1174 669L1170 669L1178 685L1173 685L1181 692\
    L1151 692L1158 685L1154 685L1161 669L1157 669ZM1062 644L1071 665L1067 665L1075 682\
    L1070 682L1079 691L1045 691L1053 682L1048 682L1057 665L1052 665ZM1248 636L1260 665\
    L1254 665L1266 688L1259 688L1270 699L1225 699L1236 688L1229 688L1241 665L1235 665Z\
    M1472 642L1484 668L1479 668L1489 689L1483 689L1493 700L1452 700L1462 689L1456 689\
    L1466 668L1461 668ZM21 636L33 663L28 663L39 685L32 685L43 696L0 696L10 685L4 685\
    L15 663L9 663ZM1167 624L1180 654L1175 654L1186 679L1179 679L1191 690L1144 690\
    L1156 679L1148 679L1160 654L1154 654ZM203 658L210 673L207 673L213 685L209 685\
    L215 691L191 691L197 685L193 685L199 673L196 673ZM119 666L125 679L122 679L128 691\
    L125 691L130 696L108 696L114 691L110 691L116 679L113 679ZM1531 654L1538 671L1535 671\
    L1542 685L1538 685L1545 691L1517 691L1524 685L1520 685L1527 671L1524 671ZM312 635\
    L323 661L318 661L328 682L322 682L332 692L291 692L301 682L295 682L305 661L300 661Z\
    M728 624L742 656L736 656L749 682L741 682L754 694L703 694L716 682L708 682L721 656\
    L715 656ZM1085 633L1097 660L1092 660L1103 683L1096 683L1107 694L1063 694L1074 683\
    L1067 683L1078 660L1073 660ZM663 660L669 674L666 674L672 686L669 686L675 692L651 692\
    L657 686L654 686L659 674L657 674ZM1305 623L1318 654L1312 654L1324 679L1317 679\
    L1329 692L1280 692L1292 679L1285 679L1297 654L1291 654ZM1498 641L1508 664L1503 664\
    L1512 682L1507 682L1516 691L1480 691L1489 682L1483 682L1492 664L1488 664ZM325 638\
    L335 662L330 662L340 682L334 682L344 692L305 692L315 682L309 682L319 662L314 662Z\
    M108 656L116 674L112 674L120 689L115 689L123 696L93 696L101 689L96 689L104 674\
    L100 674ZM892 651L902 673L897 673L906 691L901 691L910 700L874 700L883 691L878 691\
    L887 673L882 673ZM103 647L113 668L108 668L117 685L112 685L120 693L87 693L95 685\
    L90 685L98 668L94 668ZM-13 664L-7 677L-10 677L-4 689L-8 689L-2 694L-24 694L-19 689\
    L-22 689L-16 677L-19 677ZM130 660L138 678L134 678L141 692L137 692L144 699L116 699\
    L123 692L118 692L126 678L122 678ZM1255 634L1267 661L1262 661L1272 683L1266 683\
    L1277 694L1233 694L1244 683L1238 683L1248 661L1243 661ZM1382 630L1396 660L1389 660\
    L1402 686L1394 686L1407 698L1357 698L1370 686L1362 686L1375 660L1369 660ZM1065 641\
    L1076 664L1071 664L1081 684L1075 684L1084 693L1046 693L1056 684L1050 684L1060 664\
    L1055 664ZM647 645L657 668L653 668L662 686L656 686L665 695L630 695L639 686L633 686\
    L642 668L638 668ZM911 638L922 664L917 664L928 686L921 686L932 697L889 697L900 686\
    L894 686L904 664L899 664ZM1018 640L1030 666L1024 666L1035 688L1028 688L1039 699\
    L997 699L1007 688L1001 688L1011 666L1006 666ZM1366 625L1380 656L1374 656L1386 682\
    L1379 682L1391 694L1341 694L1354 682L1346 682L1359 656L1352 656ZM779 631L792 662\
    L786 662L798 687L791 687L803 700L754 700L766 687L759 687L771 662L765 662ZM567 660\
    L573 675L570 675L576 687L573 687L579 693L555 693L561 687L557 687L563 675L560 675Z\
    M1428 639L1439 665L1434 665L1444 687L1438 687L1449 697L1407 697L1417 687L1411 687\
    L1421 665L1416 665ZM1547 631L1559 658L1553 658L1564 681L1558 681L1569 692L1524 692\
    L1535 681L1529 681L1540 658L1534 658ZM371 629L383 657L377 657L388 680L382 680\
    L393 691L348 691L359 680L353 680L364 657L358 657ZM1482 634L1494 662L1488 662\
    L1499 685L1493 685L1504 696L1459 696L1470 685L1464 685L1475 662L1469 662ZM1147 643\
    L1157 666L1152 666L1162 685L1156 685L1165 694L1128 694L1138 685L1132 685L1141 666\
    L1137 666ZM1410 646L1421 670L1416 670L1425 689L1420 689L1429 699L1391 699L1401 689\
    L1395 689L1404 670L1400 670ZM424 637L435 663L430 663L440 684L434 684L444 694L403 694\
    L413 684L407 684L418 663L412 663ZM628 658L635 675L632 675L639 689L635 689L641 696\
    L614 696L621 689L617 689L624 675L621 675Z";
const NEAR_RIDGE: &str = "\
    M-20 900L-20 774L30 783L80 791L130 787L180 791L230 792L280 783L330 778L380 788\
    L430 776L480 767L530 765L580 754L630 758L680 768L730 760L780 753L830 764L880 768\
    L930 761L980 751L1030 755L1080 750L1130 757L1180 760L1230 766L1280 757L1330 753\
    L1380 758L1430 769L1480 770L1530 772L1580 779L1620 900ZM144 733L159 766L152 766\
    L166 793L158 793L171 806L118 806L131 793L123 793L136 766L130 766ZM1313 720L1331 760\
    L1323 760L1339 793L1329 793L1345 809L1281 809L1297 793L1287 793L1303 760L1295 760Z\
    M67 679L91 735L80 735L103 782L89 782L112 804L21 804L44 782L30 782L53 735L42 735Z\
    M1279 685L1303 741L1292 741L1314 786L1301 786L1323 808L1235 808L1257 786L1243 786\
    L1266 741L1255 741ZM152 661L180 726L167 726L193 779L177 779L203 805L100 805L126 779\
    L110 779L136 726L123 726ZM1330 705L1350 751L1341 751L1360 790L1348 790L1367 809\
    L1292 809L1311 790L1300 790L1318 751L1309 751ZM1277 681L1302 739L1290 739L1314 786\
    L1300 786L1323 809L1230 809L1254 786L1240 786L1263 739L1251 739ZM1387 722L1404 760\
    L1397 760L1412 792L1403 792L1418 807L1357 807L1372 792L1363 792L1378 760L1371 760Z\
    M234 729L249 765L242 765L257 795L248 795L262 809L205 809L219 795L210 795L225 765\
    L218 765ZM1546 726L1561 760L1554 760L1567 787L1559 787L1573 801L1519 801L1532 787\
    L1524 787L1538 760L1531 760ZM1523 670L1550 733L1538 733L1563 784L1548 784L1573 810\
    L1473 810L1498 784L1483 784L1508 733L1495 733ZM305 732L319 765L313 765L326 792\
    L318 792L331 805L279 805L292 792L284 792L297 765L291 765ZM212 709L231 752L223 752\
    L240 788L229 788L247 805L178 805L195 788L185 788L202 752L193 752Z";
