//! Stats for nerds (`src/game/nerd-stats.ts`, a room's rows from
//! `src/net/network-stats.ts`): the engine's `stats_json` in folding sections, shown
//! while the debug tools are on and N has opened the panel. Single player reads
//! Performance, Physics, Render, Battle and Configuration; a room leads with Network,
//! the connection and how play feels over it, each row explained on hover.

use egui::{Align2, Color32, Id, Order, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use serde_json::Value;

use super::UiState;
use super::paint::{self, hex, hexa};
use super::theme::{TEXT_LABEL, bold_font, display_font};

/// A row: label, value and what it means (shown on hover; empty for none).
type Row = (String, String, String);

fn row(label: &str, value: String, hint: &str) -> Row {
    (label.to_owned(), value, hint.to_owned())
}

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

/// What a room's panel measures between two readings: the snapshot batches received
/// and the main thread's time applying them.
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(test, derive(serde::Serialize))]
#[cfg_attr(test, serde(rename_all = "camelCase"))]
pub struct NetworkSample {
    at_ms: f64,
    updates: f64,
    receive_ms: f64,
    /// Batches a second, and milliseconds applying them per second, since the reading
    /// before.
    rate: f64,
    receive_share: f64,
}

/// Take a reading of a room's freshly read `stats` at `now_ms` (`networkStatsSections`
/// measures its update rate the same way, between refreshes).
pub fn sample_network(state: &mut UiState, now_ms: f64) {
    let Some(network) = state.stats.as_ref().map(|stats| &stats["network"]) else {
        return;
    };
    let updates = number(network, "receivedUpdates");
    let receive_ms = number(network, "receiveMs");
    let mut sample = NetworkSample {
        at_ms: now_ms,
        updates,
        receive_ms,
        ..NetworkSample::default()
    };
    if let Some(before) = state.network_sample {
        let elapsed = now_ms - before.at_ms;
        if elapsed > 0.0 {
            sample.rate = (updates - before.updates) * 1000.0 / elapsed;
            sample.receive_share = (receive_ms - before.receive_ms) / elapsed;
        }
    }
    state.network_sample = Some(sample);
}

/// One feel reading (`Reading`): its sample count and statistics.
fn reading(feel: &Value, key: &str) -> (f64, f64, f64, f64) {
    let value = &feel[key];
    (
        number(value, "count"),
        number(value, "mean"),
        number(value, "p95"),
        number(value, "max"),
    )
}

/// Correction causes in the engine's order (`CorrectionCause::ALL`): key, row label and
/// what the server or the prediction saw.
const CORRECTION_CAUSES: [(&str, &str, &str); 7] = [
    (
        "blast",
        "Blast corrections",
        "a blast near your tank or a hit on it",
    ),
    ("tank", "Tank corrections", "contact with another tank"),
    ("crate", "Crate corrections", "contact with a crate or drum"),
    (
        "debris",
        "Debris corrections",
        "solid debris near your tank (wrecks, timber, broken scenery)",
    ),
    ("cover", "Cover corrections", "contact with standing cover"),
    (
        "lateInput",
        "Input corrections",
        "an input change that reached the server after its tick",
    ),
    ("other", "Other corrections", "none of the other causes"),
];

/// A room's Network rows (`networkStatsSections`).
fn network_rows(stats: &Value, sample: NetworkSample) -> Vec<Row> {
    let network = &stats["network"];
    let feel = &stats["feel"];
    let n = |key: &str| number(network, key);
    let millis = |value: f64| format!("{} ms", value.round());
    let (fire_count, fire_mean, fire_p95, fire_max) = reading(feel, "fireToFlashMs");
    let report_mean = reading(feel, "fireToReportMs").1;
    let (hit_count, hit_mean, hit_p95, hit_max) = reading(feel, "hitOffsetM");
    let (in_count, in_mean, in_p95, _) = reading(feel, "incomingOffsetM");
    let (behind_count, behind_mean, behind_p95, _) = reading(feel, "incomingBehindM");
    let own = &feel["ownShots"];
    let (error_count, error_mean, error_p95, _) = reading(own, "errorM");
    let or_dash = |count: f64, text: String| if count > 0.0 { text } else { "–".into() };
    let mut rows = vec![
        row(
            "RTT",
            millis(n("rttMs")),
            "Measured round-trip time to the game server.",
        ),
        row(
            "Updates received",
            grouped(n("receivedUpdates")),
            "Full-state messages and snapshot batches received during this session. A batch can contain several simulation snapshots.",
        ),
        row(
            "Update rate",
            format!("{:.1} /s", sample.rate),
            "Full-state messages and snapshot batches received per second, not rendered FPS.",
        ),
        row(
            "Snapshot age",
            millis(n("snapshotAgeMs")),
            "Time since the last full state or snapshot arrived.",
        ),
        row(
            "Playout buffer",
            millis(n("bufferMs")),
            "How far other tanks are drawn behind the fastest recent snapshot arrival. It grows when snapshots arrive late and shrinks slowly afterwards.",
        ),
        row(
            "Buffered ahead",
            millis(n("marginMs")),
            "Received simulation not yet displayed. Negative means snapshots are late and other tanks are briefly extrapolated.",
        ),
        row(
            "Underrun",
            format!("{:.1} %", n("underrun") * 100.0),
            "Share of recent frames drawn past the newest snapshot. Sustained values mean visible stutter.",
        ),
        row(
            "Late batches",
            grouped(n("lateBatches")),
            "Snapshot batches that arrived over 150 ms after the previous one (they leave 60 times a second) during this session. The connection held them up, for example while TCP resent a lost packet, or the app itself froze.",
        ),
        row(
            "Snapshot CPU",
            format!("{:.1} ms/s", sample.receive_share * 1000.0),
            "Main-thread time spent applying received snapshots per second, own-hull prediction replays included. A snapshot that confirms your tank's prediction needs no replay.",
        ),
        row(
            "Longest batch gap",
            millis(n("longestBatchGapMs")),
            "The longest wait between consecutive snapshot batches during this session; about 17 ms while the stream flows.",
        ),
        row(
            "Prediction lead",
            millis(n("predictionLeadMs")),
            "How far your own tank is predicted ahead of the newest server tick, so driving responds at once. About the round trip plus a little margin.",
        ),
        row(
            "Corrections",
            format!("{:.1} cm/s", n("correctionMPerS") * 100.0),
            "How far server snapshots moved your predicted tank, per second of driving. Each correction is glided out over about 0.1 s. Blasts, other players' steering and debris cause most of them.",
        ),
        row(
            "Correction p95",
            format!("{:.1} cm", n("correctionP95M") * 100.0),
            "The 95th percentile of your tank's recent prediction corrections.",
        ),
    ];
    for (key, label, cause) in CORRECTION_CAUSES {
        let cause_stats = &network["correctionCauses"][key];
        rows.push(row(
            label,
            format!("{:.1} cm/s", number(cause_stats, "mPerS") * 100.0),
            &format!(
                "Corrections of 2 cm or more ({} so far), per second of driving, put down to {cause}: the first cause, in the order of these rows, that the server or the prediction saw since the previous snapshot.",
                number(cause_stats, "count")
            ),
        ));
    }
    rows.extend([
        row(
            "View delay",
            millis(reading(feel, "viewDelayMs").1),
            "How far behind your own tank's present other tanks, shells and effects are drawn: the round trip, the input margin and the playout buffer.",
        ),
        row(
            "Fire to flash",
            or_dash(fire_count, millis(fire_mean)),
            &format!(
                "From pressing fire with a loaded gun to your muzzle flash on screen, the mean of {fire_count} presses. {} ms of it passes before the server's report of the shot arrives; the rest is the playout buffer.",
                report_mean.round()
            ),
        ),
        row(
            "Fire to flash p95",
            or_dash(fire_count, millis(fire_p95)),
            &format!(
                "The 95th percentile of recent presses; the slowest took {} ms.",
                fire_max.round()
            ),
        ),
        row(
            "Own shots on time",
            format!(
                "{} / {}",
                number(own, "confirmed"),
                number(own, "predicted")
            ),
            &format!(
                "Your shots drawn the moment you fired them that the server fired too, out of all such shots. {} more were fired by the server without being drawn early, and {} early shells were never fired by the server and were dropped.",
                number(own, "unpredicted"),
                number(own, "unconfirmedShells")
            ),
        ),
        row(
            "Own shell error",
            or_dash(error_count, format!("{:.1} cm", error_mean * 100.0)),
            &format!(
                "How far a shell drawn the moment you fired was from the server's shell, a tenth of a second into its flight: the mean of {error_count}, 95th percentile {:.1} cm. The shell then follows the server's path.",
                error_p95 * 100.0
            ),
        ),
        row(
            "Hit offset",
            or_dash(hit_count, format!("{hit_mean:.2} m")),
            &format!(
                "For each shell hit you scored on a tank ({hit_count} so far): how far the server's impact was from that tank as your screen drew it when your own tank reached the moment of the hit. Near zero, you hit tanks where you see them; otherwise a crossing tank needs that much extra lead."
            ),
        ),
        row(
            "Hit offset p95",
            or_dash(hit_count, format!("{hit_p95:.2} m")),
            &format!("The 95th percentile of recent hits; the largest was {hit_max:.2} m."),
        ),
        row(
            "Incoming offset",
            or_dash(in_count, format!("{in_mean:.2} m")),
            &format!(
                "For each enemy shell that hit you ({in_count} so far): how close it was drawn to your tank, where the server judged it, as your own tank reached the moment of the hit; 95th percentile {in_p95:.2} m. Near zero, a shell you see coming hits where you see it. {} more were fired so close that they hit before they could be drawn.",
                number(feel, "unseenIncoming")
            ),
        ),
        row(
            "Hit where you were",
            or_dash(behind_count, format!("{behind_mean:.2} m")),
            &format!(
                "For each other player's shell that hit you ({behind_count} so far): the server judged it where that player's screen showed your tank, a moment earlier, so it strikes there, this far from your tank as drawn now; 95th percentile {behind_p95:.2} m. It is how far you moved in that moment."
            ),
        ),
        row(
            "Phantom strikes",
            format!(
                "{} / {}",
                n("phantomStrikes"),
                number(own, "phantomStrikes")
            ),
            "Shells drawn striking a tank ahead of the server that the server flew past instead, and flew on: enemy shells at your tank, then your own shells at enemy tanks.",
        ),
        row(
            "Late input starts",
            format!("{} / {}", n("lateInputStarts"), n("inputStarts")),
            "Input changes the server started after the tick your prediction asked for, out of all acknowledged changes during this session. Late ones arrived too late to start on time, so the prediction is corrected.",
        ),
        row(
            "Server tick",
            grouped(n("serverTick")),
            "Latest authoritative simulation tick received.",
        ),
        row(
            "Input seq sent / ack",
            format!("{} / {}", n("inputSeq"), n("inputAck")),
            "Latest input sequence sent and acknowledged by the server. Active input sends up to 20/s; unchanged idle input refreshes once/s to retain your seat. These are not received state updates.",
        ),
        row(
            "Connection",
            if network["connected"].as_bool().unwrap_or(false) {
                "Connected".into()
            } else {
                "Reconnecting".into()
            },
            "Current game-server connection state.",
        ),
    ]);
    rows
}

/// The renderer's rows, shared by single player and rooms (`renderRows`).
fn render_rows(stats: &Value) -> Vec<Row> {
    let n = |key: &str| number(stats, key);
    let megabytes = |key: &str| format!("{:.1} MB", n(key) / 1_048_576.0);
    let quality = &stats["quality"];
    let scene = quality["scene"]
        .as_array()
        .map(|size| {
            size.iter()
                .map(|value| format!("{}", value.as_f64().unwrap_or(0.0)))
                .collect::<Vec<_>>()
                .join("×")
        })
        .unwrap_or_default();
    vec![
        row(
            "Graphics API",
            stats["graphicsApi"].as_str().unwrap_or("—").to_owned(),
            "Metal on macOS, Direct3D 12 on Windows.",
        ),
        row(
            "Graphics setting",
            capitalized(quality["setting"].as_str().unwrap_or("—")),
            "The Settings dialog's Graphics choice. Auto lowers the quality tier and the render scale while frames run slow and raises them again once they keep up.",
        ),
        row(
            "Quality tier",
            capitalized(quality["tier"].as_str().unwrap_or("—")),
            "The optional per-pixel features in use: High is the full picture; Medium drops fine ground and cloud detail and ambient occlusion; Low also drops bloom and halves the shadow map.",
        ),
        row(
            "Render scale",
            format!(
                "{}%",
                (quality["scale"].as_f64().unwrap_or(1.0) * 100.0).round()
            ),
            "The 3D scene's resolution against the window's drawing buffer; the output pass upsamples it.",
        ),
        row(
            "Scene pixels",
            scene,
            "The 3D scene's width and height in pixels before the upsampling.",
        ),
        row(
            "GPU frame time",
            quality["gpuMs"]
                .as_f64()
                .map_or("—".to_owned(), |gpu| format!("{gpu:.1} ms")),
            "How long the GPU takes to finish a frame, from its scene's submission until the GPU reports it done (more when it was still busy with the frame before).",
        ),
        row(
            "Draw calls / frame",
            grouped(n("drawCalls")),
            "GPU draw calls issued per rendered frame.",
        ),
        row(
            "Triangles / frame",
            grouped(n("triangles")),
            "Triangles submitted per rendered frame.",
        ),
        row("Shadow triangles", grouped(n("shadowTriangles")), ""),
        row(
            "Reflection triangles",
            grouped(n("reflectionTriangles")),
            "",
        ),
        row("Main triangles", grouped(n("mainTriangles")), ""),
        row("GPU geometries", grouped(n("meshes")), ""),
        row("GPU textures", grouped(n("textures")), ""),
        row("GPU memory", megabytes("gpuBytes"), ""),
        row("Mesh page slack", megabytes("meshSlackBytes"), ""),
    ]
}

/// The panel's sections and their rows.
fn sections(stats: &Value, sample: NetworkSample) -> Vec<(&'static str, Vec<Row>)> {
    let n = |key: &str| number(stats, key);
    let configuration = vec![row(
        "Pixel ratio",
        format!("{}", n("pixelRatio")),
        "Renderer resolution multiplier, capped from the display's.",
    )];
    let network = stats["network"].is_object();
    let time = |key: &str, precision: usize| {
        stats[key]
            .as_f64()
            .map_or("—".to_owned(), |value| format!("{value:.precision$} ms"))
    };
    let performance = vec![
        row(
            "FPS",
            stats["fps"]
                .as_f64()
                .filter(|fps| *fps > 0.0)
                .map_or("—".to_owned(), |fps| format!("{fps:.0}")),
            "Frames drawn per second.",
        ),
        row("Frame interval", time("frameMs", 1), "Time between frames."),
        row(
            if network {
                "Update CPU / frame"
            } else {
                "Sim CPU / frame"
            },
            time("simMs", 2),
            "Main-thread time updating the game per frame. Excludes GPU work.",
        ),
        row(
            "Render CPU / frame",
            time("renderMs", 2),
            "Main-thread time building and encoding a frame; the GPU driver's own work counts here too, where a browser does it in its GPU process.",
        ),
    ];
    if stats["network"].is_object() {
        let scene = &stats["scene"];
        let s = |key: &str| number(scene, key);
        let battle = if scene.is_object() {
            vec![
                row(
                    "Tanks",
                    format!("{} / {}", s("alive"), s("tanks")),
                    "Tanks alive out of total spawned.",
                ),
                row("Mines", grouped(s("mines")), "Live mines on the field."),
                row(
                    "Pickups ready",
                    format!("{} / {}", s("pickupsReady"), s("pickups")),
                    "Pickups available now out of total placed.",
                ),
                row(
                    "Projectiles",
                    grouped(s("shots")),
                    "Shots currently flying.",
                ),
                row(
                    "Debris bodies",
                    grouped(s("fragments")),
                    "Debris pieces in the received scene; server physics allocation is not measured here.",
                ),
                row(
                    "Sim time",
                    format!("{:.1}s", s("elapsed")),
                    "Elapsed simulation time since the round started.",
                ),
            ]
        } else {
            Vec::new()
        };
        let mut render = render_rows(stats);
        // Rooms show the same client rendering counters as the page; allocation
        // estimates belong to the single-player engine's panel.
        render.truncate(render.len() - 2);
        return vec![
            ("PERFORMANCE", performance),
            ("NETWORK", network_rows(stats, sample)),
            ("RENDER", render),
            ("BATTLE", battle),
            ("CONFIGURATION", configuration),
        ];
    }
    let dynamic = n("dynamicBodies");
    let sleeping = n("sleepingBodies");
    let physics = vec![
        row("Bodies", grouped(n("bodies")), ""),
        row(
            "Fixed / dynamic",
            format!("{} / {}", n("fixedBodies"), dynamic),
            "",
        ),
        row(
            "Awake / sleeping",
            format!("{} / {}", dynamic - sleeping, sleeping),
            "",
        ),
        row("Colliders", grouped(n("colliders")), ""),
    ];
    let battle = vec![
        row(
            "Tanks",
            format!("{} / {}", n("tanksAlive"), n("tanks")),
            "Tanks alive out of total spawned.",
        ),
        row("Mines", grouped(n("mines")), "Live mines on the field."),
        row(
            "Pickups ready",
            format!("{} / {}", n("pickupsReady"), n("pickups")),
            "Pickups available now out of total placed.",
        ),
        row(
            "Projectiles",
            grouped(n("shots")),
            "Shots currently flying.",
        ),
        row("Visual particles", grouped(n("particles")), ""),
        row(
            "Debris bodies",
            format!("{} / {}", n("fragments"), n("maxFragments")),
            "",
        ),
        row(
            "Sim time",
            format!("{:.1}s", n("elapsed")),
            "Elapsed simulation time since the round started.",
        ),
    ];
    vec![
        ("PERFORMANCE", performance),
        ("PHYSICS", physics),
        ("RENDER", render_rows(stats)),
        ("BATTLE", battle),
        ("CONFIGURATION", configuration),
    ]
}

/// The web panel is 32 monospace characters plus 24 pixels, at 11px type.
const PANEL_COLUMNS: f32 = 32.0;
const ROW_HEIGHT: f32 = 16.5;
const SECTION_HEAD: f32 = 20.8;
const SECTION_GAP: f32 = 6.0;
const HEADER_HEIGHT: f32 = 37.0;
const PANEL_PADDING: f32 = 7.0;

/// Whether `section` shows its rows: every one but Configuration until folded, as the
/// page's panel opens.
fn section_open(state: &UiState, section: &str) -> bool {
    state
        .nerd_sections
        .get(section)
        .copied()
        .unwrap_or(section != "CONFIGURATION")
}

pub fn panel(context: &egui::Context, state: &mut UiState) {
    let Some(stats) = state.stats.clone() else {
        return;
    };
    let list = sections(&stats, state.network_sample.unwrap_or_default());
    let screen = context.content_rect();
    let row_font = egui::FontId::monospace(11.0);
    let font_painter = context.layer_painter(egui::LayerId::background());
    let character_width = paint::text_width(&font_painter, "0", row_font.clone(), 0.0);
    let panel_width = (PANEL_COLUMNS * character_width + 24.0).min(screen.width() - 16.0);
    let rows: f32 = list
        .iter()
        .map(|(title, rows)| {
            SECTION_HEAD
                + if section_open(state, title) {
                    rows.len() as f32 * ROW_HEIGHT
                } else {
                    0.0
                }
        })
        .sum();
    let height = (HEADER_HEIGHT + rows + SECTION_GAP * (list.len() - 1) as f32 + PANEL_PADDING)
        .min(screen.height() - 24.0);
    let mut toggled = None;
    let mut closed = false;
    egui::Area::new(Id::unique("nerd stats"))
        .order(Order::Foreground)
        .fixed_pos(pos2(
            screen.right() - 8.0 - panel_width,
            screen.bottom() - 5.0 - height,
        ))
        .show(context, |ui| {
            let rect = Rect::from_min_size(ui.cursor().min, vec2(panel_width, height));
            let painter = ui.painter().clone();
            painter.add(paint::rounded_vgradient(
                rect,
                [12.0; 4],
                &[(0.0, hexa(0x11264a, 0.94)), (1.0, hexa(0x08142a, 0.95))],
            ));
            painter.rect_stroke(
                rect,
                12,
                Stroke::new(1.0, hexa(0x9fd0ff, 0.15)),
                StrokeKind::Inside,
            );
            let left = rect.left() + 13.0;
            let header = Rect::from_min_max(
                rect.min,
                pos2(rect.right() - 44.0, rect.top() + HEADER_HEIGHT),
            );
            paint::text(
                &painter,
                pos2(left, rect.top() + 12.0),
                Align2::LEFT_TOP,
                "Stats for Nerds",
                display_font(20.0),
                Color32::WHITE,
                0.4,
            );
            closed |= ui
                .interact(header, Id::unique("nerd stats title"), Sense::click())
                .clicked();
            // The header stays while the rows scroll, including the full 44px close target.
            let close =
                Rect::from_min_size(pos2(rect.right() - 44.0, rect.top()), vec2(44.0, 44.0));
            let response = ui
                .interact(close, Id::unique("nerd stats close"), Sense::click())
                .on_hover_text("Hide (N)");
            if response.hovered() {
                painter.rect_filled(close, 6, hexa(0xffffff, 0.12));
            }
            paint::text(
                &painter,
                close.center(),
                Align2::CENTER_CENTER,
                "×",
                egui::FontId::proportional(24.0),
                hex(0xeeeeee),
                0.0,
            );
            closed |= response.clicked();
            let body = Rect::from_min_max(
                pos2(rect.left() + PANEL_PADDING, rect.top() + HEADER_HEIGHT),
                pos2(rect.right() - PANEL_PADDING, rect.bottom() - PANEL_PADDING),
            );
            let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
            egui::ScrollArea::vertical()
                .id_salt("nerd stats rows")
                .max_height(body.height())
                .auto_shrink([false, true])
                .show(&mut body_ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let width = body.width();
                    for (index, (title, rows)) in list.iter().enumerate() {
                        if index > 0 {
                            ui.add_space(SECTION_GAP);
                        }
                        let open = section_open(state, title);
                        let (head, response) =
                            ui.allocate_exact_size(vec2(width, SECTION_HEAD), Sense::click());
                        let painter = ui.painter();
                        let x = head.left() + 5.0;
                        let tip = pos2(x + 4.0, head.center().y);
                        let arrow = if open {
                            vec![
                                tip + vec2(-3.5, -2.5),
                                tip + vec2(3.5, -2.5),
                                tip + vec2(0.0, 3.0),
                            ]
                        } else {
                            vec![
                                tip + vec2(-2.0, -3.5),
                                tip + vec2(3.0, 0.0),
                                tip + vec2(-2.0, 3.5),
                            ]
                        };
                        let color = if response.hovered() {
                            Color32::WHITE
                        } else {
                            TEXT_LABEL
                        };
                        painter.add(egui::Shape::convex_polygon(arrow, color, Stroke::NONE));
                        paint::text(
                            painter,
                            pos2(x + 13.0, head.center().y),
                            Align2::LEFT_CENTER,
                            title,
                            bold_font(10.5),
                            color,
                            1.5,
                        );
                        if response.clicked() {
                            toggled = Some((*title, !open));
                        }
                        if !open {
                            continue;
                        }
                        for (label, value, hint) in rows {
                            let (line, response) =
                                ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
                            paint::text(
                                ui.painter(),
                                pos2(line.left() + 5.0, line.center().y),
                                Align2::LEFT_CENTER,
                                &format!("{label:20} {value}"),
                                row_font.clone(),
                                hex(0xe6eef8),
                                0.0,
                            );
                            if !hint.is_empty() {
                                response.on_hover_text(hint);
                            }
                        }
                    }
                });
            ui.allocate_rect(rect, Sense::hover());
        });
    if let Some((section, open)) = toggled {
        state.nerd_sections.insert(section, open);
    }
    if closed {
        state.show_nerd_stats = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_room_keeps_performance_before_its_network_rows() {
        let stats = serde_json::json!({
            "network": {
                "rttMs": 42.4,
                "receivedUpdates": 1200,
                "connected": true,
                "correctionCauses": {"blast": {"count": 3, "mPerS": 0.012}}
            },
            "feel": {"fireToFlashMs": {"count": 0}},
            "scene": {"tanks": 12, "alive": 9},
            "quality": {}
        });
        let list = sections(&stats, NetworkSample::default());
        let titles: Vec<_> = list.iter().map(|(title, _)| *title).collect();
        assert_eq!(
            titles,
            [
                "PERFORMANCE",
                "NETWORK",
                "RENDER",
                "BATTLE",
                "CONFIGURATION"
            ]
        );
        let network = &list[1].1;
        let value = |label: &str| {
            network
                .iter()
                .find(|row| row.0 == label)
                .map(|row| row.1.clone())
        };
        assert_eq!(value("RTT").as_deref(), Some("42 ms"));
        assert_eq!(value("Blast corrections").as_deref(), Some("1.2 cm/s"));
        assert_eq!(value("Fire to flash").as_deref(), Some("–"));
        assert_eq!(value("Connection").as_deref(), Some("Connected"));
        assert_eq!(list[3].1[0].1, "9 / 12");
        assert_eq!(list[0].1[2].0, "Update CPU / frame");
        assert!(!list[2].1.iter().any(|row| row.0 == "GPU memory"));
    }

    #[test]
    fn single_player_keeps_its_own_sections() {
        let list = sections(
            &serde_json::json!({"fps": 120.0, "quality": {}}),
            NetworkSample::default(),
        );
        assert_eq!(list[0].0, "PERFORMANCE");
        assert_eq!(list[0].1[0].1, "120");
    }

    #[test]
    fn the_update_rate_is_measured_between_readings() {
        let mut state = UiState {
            stats: Some(
                serde_json::json!({"network": {"receivedUpdates": 100, "receiveMs": 10.0}}),
            ),
            ..UiState::default()
        };
        sample_network(&mut state, 1000.0);
        state.stats =
            Some(serde_json::json!({"network": {"receivedUpdates": 160, "receiveMs": 12.0}}));
        sample_network(&mut state, 2000.0);
        let sample = state.network_sample.expect("a reading");
        assert!((sample.rate - 60.0).abs() < 1e-9);
        assert!((sample.receive_share * 1000.0 - 2.0).abs() < 1e-9);
    }
}
