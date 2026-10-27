//! Stats for nerds (`src/game/nerd-stats.ts`): the engine's `stats_json` in sections
//! (Performance, Physics, Render, Battle, Configuration), shown while Settings allow
//! it and N has opened the panel.

use egui::{Align2, Color32, Id, LayerId, Order, Painter, Rect, Stroke, StrokeKind, pos2, vec2};
use serde_json::Value;

use super::UiState;
use super::paint::{self, hex, hexa};
use super::theme::{GOLD, TEXT_LABEL, bold_font, display_font};

type Row = (&'static str, String);

fn number(stats: &Value, key: &str) -> f64 {
    stats[key].as_f64().unwrap_or(0.0)
}

/// 1234567 as "1,234,567".
fn grouped(value: f64) -> String {
    let digits = format!("{}", value.round().max(0.0) as u64);
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// The panel's sections and their rows.
fn sections(stats: &Value) -> Vec<(&'static str, Vec<Row>)> {
    let n = |key: &str| number(stats, key);
    let ms = |key: &str| format!("{:.2} ms", n(key));
    let megabytes = |key: &str| format!("{:.1} MB", n(key) / 1_048_576.0);
    let quality = &stats["quality"];
    let fps = n("fps");
    let performance = vec![
        (
            "FPS",
            if fps > 0.0 {
                format!("{fps:.0}")
            } else {
                "—".into()
            },
        ),
        ("Frame interval", format!("{:.1} ms", n("frameMs"))),
        ("Sim CPU / frame", ms("simMs")),
        ("Render CPU / frame", ms("renderMs")),
    ];
    let dynamic = n("dynamicBodies");
    let sleeping = n("sleepingBodies");
    let physics = vec![
        ("Bodies", grouped(n("bodies"))),
        (
            "Fixed / dynamic",
            format!("{} / {}", n("fixedBodies"), dynamic),
        ),
        (
            "Awake / sleeping",
            format!("{} / {}", dynamic - sleeping, sleeping),
        ),
        ("Colliders", grouped(n("colliders"))),
    ];
    let scene = quality["scene"]
        .as_array()
        .map(|size| {
            size.iter()
                .map(|value| format!("{}", value.as_f64().unwrap_or(0.0)))
                .collect::<Vec<_>>()
                .join("×")
        })
        .unwrap_or_default();
    let render = vec![
        (
            "Graphics API",
            stats["graphicsApi"].as_str().unwrap_or("—").to_owned(),
        ),
        (
            "Graphics setting",
            capitalized(quality["setting"].as_str().unwrap_or("—")),
        ),
        (
            "Quality tier",
            capitalized(quality["tier"].as_str().unwrap_or("—")),
        ),
        (
            "Render scale",
            format!(
                "{}%",
                (quality["scale"].as_f64().unwrap_or(1.0) * 100.0).round()
            ),
        ),
        ("Scene pixels", scene),
        (
            "GPU frame time",
            quality["gpuMs"]
                .as_f64()
                .map_or("—".to_owned(), |gpu| format!("{gpu:.1} ms")),
        ),
        ("Draw calls / frame", grouped(n("drawCalls"))),
        (
            "Shadow / reflection draws",
            format!("{} / {}", n("shadowDrawCalls"), n("reflectionDrawCalls")),
        ),
        ("Triangles / frame", grouped(n("triangles"))),
        ("Shadow triangles", grouped(n("shadowTriangles"))),
        ("Reflection triangles", grouped(n("reflectionTriangles"))),
        ("Main triangles", grouped(n("mainTriangles"))),
        (
            "Pipelines",
            format!("{} ({} late)", n("pipelines"), n("latePipelines")),
        ),
        ("GPU geometries", grouped(n("meshes"))),
        ("GPU textures", grouped(n("textures"))),
        ("GPU memory", megabytes("gpuBytes")),
        ("Mesh page slack", megabytes("meshSlackBytes")),
    ];
    let battle = vec![
        ("Tanks", format!("{} / {}", n("tanksAlive"), n("tanks"))),
        ("Mines", grouped(n("mines"))),
        (
            "Pickups ready",
            format!("{} / {}", n("pickupsReady"), n("pickups")),
        ),
        ("Projectiles", grouped(n("shots"))),
        ("Visual particles", grouped(n("particles"))),
        (
            "Debris bodies",
            format!("{} / {}", n("fragments"), n("maxFragments")),
        ),
        ("Sim time", format!("{:.1}s", n("elapsed"))),
    ];
    let configuration = vec![("Pixel ratio", format!("{}", n("pixelRatio")))];
    vec![
        ("PERFORMANCE", performance),
        ("PHYSICS", physics),
        ("RENDER", render),
        ("BATTLE", battle),
        ("CONFIGURATION", configuration),
    ]
}

const PANEL_WIDTH: f32 = 300.0;
const ROW_HEIGHT: f32 = 16.0;
const SECTION_HEAD: f32 = 22.0;
/// The panel hangs under the kill feed's three rows, clear of them.
const NERD_STATS_TOP: f32 = 190.0;

pub fn panel(context: &egui::Context, state: &UiState) {
    let Some(stats) = &state.stats else { return };
    let screen = context.content_rect();
    let list = sections(stats);
    let rows: usize = list.iter().map(|(_, rows)| rows.len()).sum();
    let content = 34.0 + list.len() as f32 * (SECTION_HEAD + 4.0) + rows as f32 * ROW_HEIGHT + 22.0;
    // A short window scales the type down rather than running off the screen.
    let available = screen.height() - NERD_STATS_TOP - 40.0;
    let scale = (available / content).clamp(0.6, 1.0);
    let size = vec2(PANEL_WIDTH * scale.max(0.85), content * scale);
    let rect = Rect::from_min_size(
        pos2(
            screen.right() - 16.0 - size.x,
            screen.top() + NERD_STATS_TOP,
        ),
        size,
    );
    let painter = context.layer_painter(LayerId::new(Order::Foreground, Id::unique("nerd stats")));
    draw(&painter, rect, scale, &list);
}

fn draw(painter: &Painter, rect: Rect, scale: f32, list: &[(&'static str, Vec<Row>)]) {
    painter.rect_filled(rect, 10, hexa(0x050b16, 0.86));
    painter.rect_stroke(
        rect,
        10,
        Stroke::new(1.0, hexa(0x9fd0ff, 0.2)),
        StrokeKind::Inside,
    );
    let left = rect.left() + 12.0;
    let right = rect.right() - 12.0;
    let mut y = rect.top() + 10.0;
    paint::text(
        painter,
        pos2(left, y),
        Align2::LEFT_TOP,
        "Stats for Nerds",
        display_font(20.0 * scale),
        Color32::WHITE,
        0.4,
    );
    paint::text(
        painter,
        pos2(right, y + 6.0 * scale),
        Align2::RIGHT_TOP,
        "N to hide",
        egui::FontId::proportional(11.0 * scale),
        hex(0x8fa8c0),
        0.0,
    );
    y += 34.0 * scale;
    let row_height = ROW_HEIGHT * scale;
    for (title, rows) in list {
        painter.rect_filled(
            Rect::from_min_size(pos2(left, y + 4.0 * scale), vec2(3.0, 11.0 * scale)),
            1,
            GOLD,
        );
        let heading = paint::text(
            painter,
            pos2(left + 9.0, y + 9.0 * scale),
            Align2::LEFT_CENTER,
            title,
            bold_font(11.0 * scale),
            TEXT_LABEL,
            1.6,
        );
        painter.line_segment(
            [
                pos2(heading.right() + 10.0, y + 9.0 * scale),
                pos2(right, y + 9.0 * scale),
            ],
            Stroke::new(1.0, hexa(0x9fd0ff, 0.12)),
        );
        y += SECTION_HEAD * scale;
        for (label, value) in rows {
            paint::text(
                painter,
                pos2(left, y + row_height / 2.0),
                Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(12.0 * scale),
                hex(0xa9c0d8),
                0.0,
            );
            paint::text(
                painter,
                pos2(right, y + row_height / 2.0),
                Align2::RIGHT_CENTER,
                value,
                bold_font(12.0 * scale),
                Color32::WHITE,
                0.0,
            );
            y += row_height;
        }
        y += 4.0 * scale;
    }
}
