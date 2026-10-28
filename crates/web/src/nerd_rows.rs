//! Stats for nerds: the sections and rows both shells lay out, written once from the
//! engine's readings. The page's panel (`src/game/nerd-stats.ts`) reads them as
//! `nerd_stats_json`, the desktop app's (`crates/desktop/src/ui/nerd_stats.rs`) as
//! values. Single player shows Performance, Physics, Render, Battle and Configuration;
//! a room shows the connection and how play feels over it in place of physics, which
//! runs on the server. Every row says what it means, for the shells' hover hints.
//!
//! The game types fill the readings ([`GameStats`], [`RoomStats`]) from the frame clock,
//! the renderer, the simulation and the network client only when a shell polls the
//! panel, a few times a second. The readings also read `stats_json`'s shape, for tests
//! and the desktop's picture fixtures. Numbers have one format in both shells: whole
//! counts with thousands separators ("12,408"), pairs as "9 / 12".

use std::borrow::Cow;
use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};
use sloppy_core::net::feel_readings::Samples;
use sloppy_core::net::hull_prediction::CorrectionCause;

use crate::Shell;

/// One row: what it counts, its value and what it means.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StatsRow {
    pub label: &'static str,
    pub value: String,
    pub hint: Cow<'static, str>,
}

/// A folding section of the panel and its rows.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StatsSection {
    pub title: &'static str,
    pub rows: Vec<StatsRow>,
}

/// The frame clock's averages over recent frames (`FrameTimes`).
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FrameStats {
    /// Frames that ran per second; 0 before the first.
    pub fps: f64,
    /// Main-thread milliseconds a frame spends updating the game, and drawing it.
    pub sim_ms: f64,
    pub render_ms: f64,
}

/// The Graphics setting, the tier and render scale in effect, and the GPU's frame time.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QualityStats {
    pub setting: String,
    pub tier: String,
    pub scale: f64,
    /// Scene pixels per CSS pixel (per logical pixel natively).
    pub render_ratio: f64,
    /// The scene's width and height in pixels.
    pub scene: [u32; 2],
    /// Smoothed, where the GPU reports when it finished a frame.
    pub gpu_ms: Option<f64>,
    /// Auto's tier or scale changes this session.
    pub changes: u32,
}

/// The renderer's counts for the last frame, its GPU resources and its resolution.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RendererStats {
    /// WebGPU or WebGL in the browser, Metal or Direct3D 12 natively.
    pub graphics_api: String,
    pub quality: QualityStats,
    pub draw_calls: u64,
    pub triangles: u64,
    pub shadow_triangles: u64,
    pub reflection_triangles: u64,
    pub main_triangles: u64,
    pub meshes: u64,
    pub textures: u64,
    /// Estimated GPU memory, and the mesh page space within it no mesh uses.
    pub gpu_bytes: f64,
    pub mesh_slack_bytes: f64,
    /// Drawing-buffer pixels per CSS pixel.
    pub pixel_ratio: f64,
}

/// Single player's readings: the frame clock, the renderer and the simulation.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameStats {
    #[serde(flatten)]
    pub frame: FrameStats,
    #[serde(flatten)]
    pub renderer: RendererStats,
    pub bodies: u64,
    pub fixed_bodies: u64,
    pub dynamic_bodies: u64,
    /// Dynamic bodies only.
    pub sleeping_bodies: u64,
    pub colliders: u64,
    pub tanks: u64,
    pub tanks_alive: u64,
    pub mines: u64,
    pub pickups: u64,
    pub pickups_ready: u64,
    pub shots: u64,
    /// Chips, sparks and leaves; smoke has buffers of its own.
    pub particles: u64,
    pub fragments: u64,
    pub max_fragments: u64,
    /// Simulation seconds since the round started.
    pub elapsed: f64,
}

/// A room's readings: the frame clock, the renderer, the displayed scene and the
/// connection.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoomStats {
    #[serde(flatten)]
    pub frame: FrameStats,
    #[serde(flatten)]
    pub renderer: RendererStats,
    /// None until a scene shows.
    pub scene: Option<SceneStats>,
    pub network: ConnectionStats,
    pub feel: FeelStats,
}

/// The counts of a room's displayed scene; its physics runs on the server.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SceneStats {
    pub tanks: u64,
    pub alive: u64,
    pub mines: u64,
    pub pickups: u64,
    pub pickups_ready: u64,
    pub shots: u64,
    pub fragments: u64,
    pub elapsed: f64,
}

/// The game-server connection (core's `NetworkStats`) and the main thread's time
/// applying what arrived.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConnectionStats {
    pub rtt_ms: f64,
    /// Full states and snapshot batches received this session.
    pub received_updates: u64,
    pub snapshot_age_ms: f64,
    pub buffer_ms: f64,
    pub margin_ms: f64,
    /// Share of recent frames drawn past the newest snapshot.
    pub underrun: f64,
    pub server_tick: u64,
    pub input_seq: i64,
    pub input_ack: i64,
    pub connected: bool,
    pub late_batches: u64,
    pub longest_batch_gap_ms: f64,
    pub prediction_lead_ms: f64,
    /// Prediction corrections in metres per second of driving, and the 95th percentile
    /// of recent ones in metres.
    pub correction_m_per_s: f64,
    pub correction_p95_m: f64,
    pub input_starts: u64,
    pub late_input_starts: u64,
    /// Noticeable corrections by cause, in [`CorrectionCause::ALL`] order (`stats_json`
    /// keys them by name).
    #[serde(deserialize_with = "causes_by_name")]
    pub correction_causes: [CauseStats; CorrectionCause::ALL.len()],
    /// Main-thread milliseconds spent applying received messages this session.
    pub receive_ms: f64,
    /// Enemy shells drawn striking the predicted hull that the server flew past.
    pub phantom_strikes: u64,
}

/// Corrections put down to one cause: how many, and metres per second of driving.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CauseStats {
    pub count: u64,
    pub m_per_s: f64,
}

fn causes_by_name<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<[CauseStats; CorrectionCause::ALL.len()], D::Error> {
    let by_name = HashMap::<String, CauseStats>::deserialize(deserializer)?;
    Ok(CorrectionCause::ALL.map(|cause| by_name.get(cause.as_str()).copied().unwrap_or_default()))
}

/// How play feels over the connection (core's `FeelReadings` and own-shot counts).
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FeelStats {
    pub view_delay_ms: Summary,
    pub fire_to_flash_ms: Summary,
    pub fire_to_report_ms: Summary,
    pub hit_offset_m: Summary,
    pub incoming_offset_m: Summary,
    pub incoming_behind_m: Summary,
    /// Enemy shells that hit before they could be drawn.
    pub unseen_incoming: u64,
    pub own_shots: OwnShotStats,
}

/// The viewer's own shots drawn the moment they fired, against the server's.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OwnShotStats {
    pub predicted: u64,
    pub confirmed: u64,
    pub unconfirmed_shells: u64,
    pub unpredicted: u64,
    pub phantom_strikes: u64,
    pub error_m: Summary,
}

/// One feel reading: its sample count, mean, 95th percentile and largest.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Summary {
    pub count: u64,
    pub mean: f64,
    pub p95: f64,
    pub max: f64,
}

impl From<&Samples> for Summary {
    fn from(samples: &Samples) -> Self {
        Self {
            count: samples.count,
            mean: samples.mean(),
            p95: samples.percentile(0.95),
            max: samples.largest,
        }
    }
}

/// A room's update rate and its time applying updates, measured between two readings
/// (the panel's refreshes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NetworkRates {
    pub updates_per_second: f64,
    /// Milliseconds applying received messages per second.
    pub receive_ms_per_second: f64,
}

/// The previous reading of a room's connection, which the next measures its rates
/// from.
#[derive(Clone, Copy, Debug, Default)]
pub struct NetworkSampler {
    /// When, the updates received by then and the milliseconds spent applying them.
    last: Option<(f64, u64, f64)>,
}

impl NetworkSampler {
    /// The rates since the previous reading at `now_ms` (none for the first).
    pub fn sample(&mut self, now_ms: f64, connection: &ConnectionStats) -> NetworkRates {
        let mut rates = NetworkRates::default();
        if let Some((at_ms, updates, receive_ms)) = self.last {
            let seconds = (now_ms - at_ms) / 1000.0;
            if seconds > 0.0 {
                rates.updates_per_second =
                    connection.received_updates.saturating_sub(updates) as f64 / seconds;
                rates.receive_ms_per_second = (connection.receive_ms - receive_ms) / seconds;
            }
        }
        self.last = Some((now_ms, connection.received_updates, connection.receive_ms));
        rates
    }
}

/// Single player's panel.
pub fn game_sections(stats: &GameStats, shell: Shell) -> Vec<StatsSection> {
    let mut render = renderer_rows(&stats.renderer, shell);
    render.extend(memory_rows(&stats.renderer));
    let mut battle = field_rows(
        (stats.tanks_alive, stats.tanks),
        stats.mines,
        (stats.pickups_ready, stats.pickups),
        stats.shots,
    );
    battle.extend([
        row(
            "Visual particles",
            grouped(stats.particles),
            "Active chips, sparks and leaves. Smoke uses separate buffers.",
        ),
        row(
            "Debris bodies",
            pair(stats.fragments, stats.max_fragments),
            "Physics debris pieces alive out of the pool cap.",
        ),
        sim_time_row(stats.elapsed),
    ]);
    vec![
        section("Performance", performance_rows(&stats.frame, false, shell)),
        section("Physics", physics_rows(stats)),
        section("Render", render),
        section("Battle", battle),
        section("Configuration", configuration_rows(&stats.renderer, shell)),
    ]
}

/// A room's panel; `rates` come from the room's [`NetworkSampler`].
pub fn room_sections(stats: &RoomStats, rates: NetworkRates, shell: Shell) -> Vec<StatsSection> {
    let battle = stats.scene.map_or_else(Vec::new, |scene| {
        let mut rows = field_rows(
            (scene.alive, scene.tanks),
            scene.mines,
            (scene.pickups_ready, scene.pickups),
            scene.shots,
        );
        rows.extend([
            row(
                "Debris bodies",
                grouped(scene.fragments),
                "Debris pieces in the received scene; server physics allocation is not measured here.",
            ),
            sim_time_row(scene.elapsed),
        ]);
        rows
    });
    vec![
        section("Performance", performance_rows(&stats.frame, true, shell)),
        section(
            "Network",
            network_rows(&stats.network, &stats.feel, rates, shell),
        ),
        // A room shows the client's rendering counts; the allocation estimates belong to
        // single player's panel.
        section("Render", renderer_rows(&stats.renderer, shell)),
        section("Battle", battle),
        section("Configuration", configuration_rows(&stats.renderer, shell)),
    ]
}

/// Shown for a reading with nothing measured yet.
const NONE: &str = "—";
const BYTES_PER_MEGABYTE: f64 = 1_048_576.0;

fn section(title: &'static str, rows: Vec<StatsRow>) -> StatsSection {
    StatsSection { title, rows }
}

fn row(label: &'static str, value: String, hint: impl Into<Cow<'static, str>>) -> StatsRow {
    StatsRow {
        label,
        value,
        hint: hint.into(),
    }
}

/// Rounded half up like the page's numbers always were, never "-0".
fn whole(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

/// 1234567 as "1,234,567".
fn grouped(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn pair(first: impl std::fmt::Display, second: impl std::fmt::Display) -> String {
    format!("{first} / {second}")
}

fn millis(value: f64) -> String {
    format!("{} ms", whole(value))
}

fn metres(value: f64) -> String {
    format!("{value:.2} m")
}

fn centimetres_per_second(metres_per_second: f64) -> String {
    format!("{:.1} cm/s", metres_per_second * 100.0)
}

fn megabytes(bytes: f64) -> String {
    format!("{:.1} MB", bytes / BYTES_PER_MEGABYTE)
}

/// `text` once `summary` has samples.
fn sampled(summary: &Summary, text: String) -> String {
    if summary.count > 0 { text } else { NONE.into() }
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// What each shell calls the time since it started: reloading starts a page over.
const fn session(shell: Shell) -> &'static str {
    match shell {
        Shell::Page => "this page session",
        Shell::Desktop => "this session",
    }
}

fn performance_rows(frame: &FrameStats, room: bool, shell: Shell) -> Vec<StatsRow> {
    let measured = |text: String| {
        if frame.fps > 0.0 { text } else { NONE.into() }
    };
    let update = if room {
        row(
            "Update CPU / frame",
            measured(format!("{:.2} ms", frame.sim_ms)),
            "Average client interpolation, input and effect update CPU time per frame. Excludes message decoding and server physics.",
        )
    } else {
        row(
            "Sim CPU / frame",
            measured(format!("{:.2} ms", frame.sim_ms)),
            "Average CPU time spent stepping the simulation per frame. Excludes GPU work.",
        )
    };
    vec![
        row(
            "FPS",
            measured(whole(frame.fps).to_string()),
            "Rendered frames per second, averaged over recent frames.",
        ),
        row(
            "Frame interval",
            measured(format!("{:.1} ms", 1000.0 / frame.fps)),
            "Average wall-clock time per frame over recent frames.",
        ),
        update,
        row(
            "Render CPU / frame",
            measured(format!("{:.2} ms", frame.render_ms)),
            match shell {
                Shell::Page => {
                    "Average CPU time spent submitting draw calls per frame. Excludes GPU work."
                }
                Shell::Desktop => {
                    "Average main-thread time building and encoding a frame. The GPU driver's own work counts here too, where a browser does it in its GPU process."
                }
            },
        ),
    ]
}

fn physics_rows(stats: &GameStats) -> Vec<StatsRow> {
    vec![
        row(
            "Bodies",
            grouped(stats.bodies),
            "Rigid bodies in the physics world.",
        ),
        row(
            "Fixed / dynamic",
            pair(stats.fixed_bodies, stats.dynamic_bodies),
            "Static bodies versus simulated bodies.",
        ),
        row(
            "Awake / sleeping",
            pair(
                stats.dynamic_bodies.saturating_sub(stats.sleeping_bodies),
                stats.sleeping_bodies,
            ),
            "Simulated bodies awake versus sleeping. Dynamic bodies only.",
        ),
        row(
            "Colliders",
            grouped(stats.colliders),
            "Collision shapes in the physics world.",
        ),
    ]
}

/// Both panels' Render rows. One row per scene pass: the panel is 32 characters wide,
/// too narrow for the three counts on one row.
fn renderer_rows(renderer: &RendererStats, shell: Shell) -> Vec<StatsRow> {
    let quality = &renderer.quality;
    let (surface, unit) = match shell {
        Shell::Page => ("canvas", "CSS pixel"),
        Shell::Desktop => ("window", "logical pixel"),
    };
    vec![
        row(
            "Graphics API",
            renderer.graphics_api.clone(),
            match shell {
                Shell::Page => {
                    "WebGPU, or WebGL where the browser offers no WebGPU (or the page has ?webgl)."
                }
                Shell::Desktop => "Metal on macOS, Direct3D 12 on Windows.",
            },
        ),
        row(
            "Graphics setting",
            capitalized(&quality.setting),
            "The Settings dialog's Graphics choice. Auto lowers the quality tier and the render scale while frames run slow and raises them again once they keep up.",
        ),
        row(
            "Quality tier",
            capitalized(&quality.tier),
            "The optional per-pixel features in use: High is the full picture; Medium drops fine ground and cloud detail and ambient occlusion; Low also drops bloom and halves the shadow map.",
        ),
        row(
            "Render scale",
            format!("{}%", whole(quality.scale * 100.0)),
            format!(
                "The 3D scene's resolution against the {surface}'s drawing buffer, {:.2} scene pixels per {unit}; the output pass upsamples it. {} automatic changes this session.",
                quality.render_ratio, quality.changes
            ),
        ),
        row(
            "Scene pixels",
            format!("{}×{}", quality.scene[0], quality.scene[1]),
            "The 3D scene's width and height in pixels before the upsampling.",
        ),
        row(
            "GPU frame time",
            quality
                .gpu_ms
                .map_or_else(|| NONE.into(), |ms| format!("{ms:.1} ms")),
            match shell {
                Shell::Page => {
                    "How long the GPU takes to finish a frame, from its scene's submission until WebGPU reports it done (more when the GPU was still busy with the frame before). Auto lowers the quality only while this is what slows frames. Not reported on WebGL."
                }
                Shell::Desktop => {
                    "How long the GPU takes to finish a frame, from its scene's submission until the GPU reports it done (more when it was still busy with the frame before). Auto lowers the quality only while this is what slows frames."
                }
            },
        ),
        row(
            "Draw calls / frame",
            grouped(renderer.draw_calls),
            "GPU draw calls issued per rendered frame.",
        ),
        row(
            "Triangles / frame",
            grouped(renderer.triangles),
            "Triangles submitted per rendered frame.",
        ),
        row(
            "Shadow triangles",
            grouped(renderer.shadow_triangles),
            "Triangles submitted to the shadow map pass per rendered frame.",
        ),
        row(
            "Reflection triangles",
            grouped(renderer.reflection_triangles),
            "Triangles submitted to the reflection pass per rendered frame.",
        ),
        row(
            "Main triangles",
            grouped(renderer.main_triangles),
            "Triangles submitted to the main scene pass per rendered frame.",
        ),
        row(
            "GPU geometries",
            grouped(renderer.meshes),
            "Distinct meshes currently uploaded to the GPU, sharing a few mesh page buffers. Changes on map load, not per frame.",
        ),
        row(
            "GPU textures",
            grouped(renderer.textures),
            "Textures currently uploaded to the GPU. Changes on map load, not per frame.",
        ),
    ]
}

fn memory_rows(renderer: &RendererStats) -> [StatsRow; 2] {
    [
        row(
            "GPU memory",
            megabytes(renderer.gpu_bytes),
            "Estimated GPU memory for mesh pages, textures, render targets, the shadow map and instances.",
        ),
        row(
            "Mesh page slack",
            megabytes(renderer.mesh_slack_bytes),
            "Mesh page space no mesh uses, included in GPU memory.",
        ),
    ]
}

/// The Battle rows both panels open with: tanks alive, mines, pickups and shots.
fn field_rows(
    (alive, tanks): (u64, u64),
    mines: u64,
    (ready, pickups): (u64, u64),
    shots: u64,
) -> Vec<StatsRow> {
    vec![
        row(
            "Tanks",
            pair(alive, tanks),
            "Tanks alive out of total spawned.",
        ),
        row("Mines", grouped(mines), "Live mines on the field."),
        row(
            "Pickups ready",
            pair(ready, pickups),
            "Pickups available now out of total placed.",
        ),
        row("Projectiles", grouped(shots), "Shots currently flying."),
    ]
}

fn sim_time_row(elapsed: f64) -> StatsRow {
    row(
        "Sim time",
        format!("{elapsed:.1}s"),
        "Elapsed simulation time since the round started.",
    )
}

fn configuration_rows(renderer: &RendererStats, shell: Shell) -> Vec<StatsRow> {
    vec![row(
        "Pixel ratio",
        renderer.pixel_ratio.to_string(),
        match shell {
            Shell::Page => "Renderer resolution multiplier, capped from the display pixel ratio.",
            Shell::Desktop => "Renderer resolution multiplier, capped from the display's.",
        },
    )]
}

/// A correction cause's row label and what the server or the prediction saw.
const fn cause_words(cause: CorrectionCause) -> (&'static str, &'static str) {
    match cause {
        CorrectionCause::Blast => ("Blast corrections", "a blast near your tank or a hit on it"),
        CorrectionCause::Tank => ("Tank corrections", "contact with another tank"),
        CorrectionCause::Crate => ("Crate corrections", "contact with a crate or drum"),
        CorrectionCause::Debris => (
            "Debris corrections",
            "solid debris near your tank (wrecks, timber, broken scenery)",
        ),
        CorrectionCause::Cover => ("Cover corrections", "contact with standing cover"),
        CorrectionCause::LateInput => (
            "Input corrections",
            "an input change that reached the server after its tick",
        ),
        CorrectionCause::Other => ("Other corrections", "none of the other causes"),
    }
}

fn network_rows(
    network: &ConnectionStats,
    feel: &FeelStats,
    rates: NetworkRates,
    shell: Shell,
) -> Vec<StatsRow> {
    let session = session(shell);
    let program = match shell {
        Shell::Page => "page",
        Shell::Desktop => "app",
    };
    let mut rows = vec![
        row(
            "RTT",
            millis(network.rtt_ms),
            "Measured round-trip time to the game server.",
        ),
        row(
            "Updates received",
            grouped(network.received_updates),
            format!(
                "Full-state messages and snapshot batches received during {session}. A batch can contain several simulation snapshots."
            ),
        ),
        row(
            "Update rate",
            format!("{:.1} /s", rates.updates_per_second),
            "Full-state messages and snapshot batches received per second, not rendered FPS.",
        ),
        row(
            "Snapshot age",
            millis(network.snapshot_age_ms),
            "Time since the last full state or snapshot arrived.",
        ),
        row(
            "Playout buffer",
            millis(network.buffer_ms),
            "How far other tanks are drawn behind the fastest recent snapshot arrival. It grows when snapshots arrive late and shrinks slowly afterwards.",
        ),
        row(
            "Buffered ahead",
            millis(network.margin_ms),
            "Received simulation not yet displayed. Negative means snapshots are late and other tanks are briefly extrapolated.",
        ),
        row(
            "Underrun",
            format!("{:.1} %", network.underrun * 100.0),
            "Share of recent frames drawn past the newest snapshot. Sustained values mean visible stutter.",
        ),
        row(
            "Late batches",
            grouped(network.late_batches),
            format!(
                "Snapshot batches that arrived over 150 ms after the previous one (they leave 60 times a second) during {session}. The connection held them up, for example while TCP resent a lost packet, or the {program} itself froze."
            ),
        ),
        row(
            "Snapshot CPU",
            format!("{:.1} ms/s", rates.receive_ms_per_second),
            "Main-thread time spent applying received snapshots per second, own-hull prediction replays included. A snapshot that confirms your tank's prediction needs no replay.",
        ),
        row(
            "Longest batch gap",
            millis(network.longest_batch_gap_ms),
            format!(
                "The longest wait between consecutive snapshot batches during {session}; about 17 ms while the stream flows."
            ),
        ),
        row(
            "Prediction lead",
            millis(network.prediction_lead_ms),
            "How far your own tank is predicted ahead of the newest server tick, so driving responds at once. About the round trip plus a little margin.",
        ),
        row(
            "Corrections",
            centimetres_per_second(network.correction_m_per_s),
            "How far server snapshots moved your predicted tank, per second of driving. Each correction is glided out over about 0.1 s. Blasts, other players' steering and debris cause most of them.",
        ),
        row(
            "Correction p95",
            format!("{:.1} cm", network.correction_p95_m * 100.0),
            "The 95th percentile of your tank's recent prediction corrections.",
        ),
    ];
    rows.extend(
        CorrectionCause::ALL
            .into_iter()
            .zip(network.correction_causes)
            .map(|(cause, tally)| {
                let (label, seen) = cause_words(cause);
                row(
                    label,
                    centimetres_per_second(tally.m_per_s),
                    format!(
                        "Corrections of 2 cm or more ({} so far), per second of driving, put down to {seen}: the first cause, in the order of these rows, that the server or the prediction saw since the previous snapshot.",
                        tally.count
                    ),
                )
            }),
    );
    rows.extend(feel_rows(feel, network.phantom_strikes));
    rows.extend([
        row(
            "Late input starts",
            pair(network.late_input_starts, network.input_starts),
            format!(
                "Input changes the server started after the tick your prediction asked for, out of all acknowledged changes during {session}. Late ones arrived too late to start on time, so the prediction is corrected."
            ),
        ),
        row(
            "Server tick",
            grouped(network.server_tick),
            "Latest authoritative simulation tick received.",
        ),
        row(
            "Input seq sent / ack",
            pair(network.input_seq, network.input_ack),
            "Latest input sequence sent and acknowledged by the server. Active input sends up to 20/s; unchanged idle input refreshes once/s to retain your seat. These are not received state updates.",
        ),
        row(
            "Connection",
            if network.connected {
                "Connected"
            } else {
                "Reconnecting"
            }
            .into(),
            "Current game-server connection state.",
        ),
    ]);
    rows
}

/// How shooting and being shot feel; `phantom_strikes` are enemy shells drawn striking
/// the viewer's tank that the server flew past.
fn feel_rows(feel: &FeelStats, phantom_strikes: u64) -> [StatsRow; 10] {
    let fire = &feel.fire_to_flash_ms;
    let own = &feel.own_shots;
    let error = &own.error_m;
    let hit = &feel.hit_offset_m;
    let incoming = &feel.incoming_offset_m;
    let behind = &feel.incoming_behind_m;
    [
        row(
            "View delay",
            millis(feel.view_delay_ms.mean),
            "How far behind your own tank's present other tanks, shells and effects are drawn: the round trip, the input margin and the playout buffer.",
        ),
        row(
            "Fire to flash",
            sampled(fire, millis(fire.mean)),
            format!(
                "From pressing fire with a loaded gun to your muzzle flash on screen, the mean of {} presses. {} ms of it passes before the server's report of the shot arrives; the rest is the playout buffer.",
                fire.count,
                whole(feel.fire_to_report_ms.mean)
            ),
        ),
        row(
            "Fire to flash p95",
            sampled(fire, millis(fire.p95)),
            format!(
                "The 95th percentile of recent presses; the slowest took {} ms.",
                whole(fire.max)
            ),
        ),
        row(
            "Own shots on time",
            pair(own.confirmed, own.predicted),
            format!(
                "Your shots drawn the moment you fired them that the server fired too, out of all such shots. {} more were fired by the server without being drawn early, and {} early shells were never fired by the server and were dropped.",
                own.unpredicted, own.unconfirmed_shells
            ),
        ),
        row(
            "Own shell error",
            sampled(error, format!("{:.1} cm", error.mean * 100.0)),
            format!(
                "How far a shell drawn the moment you fired was from the server's shell, a tenth of a second into its flight: the mean of {}, 95th percentile {:.1} cm. The shell then follows the server's path.",
                error.count,
                error.p95 * 100.0
            ),
        ),
        row(
            "Hit offset",
            sampled(hit, metres(hit.mean)),
            format!(
                "For each shell hit you scored on a tank ({} so far): how far the server's impact was from that tank as your screen drew it when your own tank reached the moment of the hit. Near zero, you hit tanks where you see them; otherwise a crossing tank needs that much extra lead.",
                hit.count
            ),
        ),
        row(
            "Hit offset p95",
            sampled(hit, metres(hit.p95)),
            format!(
                "The 95th percentile of recent hits; the largest was {:.2} m.",
                hit.max
            ),
        ),
        row(
            "Incoming offset",
            sampled(incoming, metres(incoming.mean)),
            format!(
                "For each enemy shell that hit you ({} so far): how close it was drawn to your tank, where the server judged it, as your own tank reached the moment of the hit; 95th percentile {:.2} m. Near zero, a shell you see coming hits where you see it. {} more were fired so close that they hit before they could be drawn.",
                incoming.count, incoming.p95, feel.unseen_incoming
            ),
        ),
        row(
            "Hit where you were",
            sampled(behind, metres(behind.mean)),
            format!(
                "For each other player's shell that hit you ({} so far): the server judged it where that player's screen showed your tank, a moment earlier, so it strikes there, this far from your tank as drawn now; 95th percentile {:.2} m. It is how far you moved in that moment.",
                behind.count, behind.p95
            ),
        ),
        row(
            "Phantom strikes",
            pair(phantom_strikes, own.phantom_strikes),
            "Shells drawn striking a tank ahead of the server that the server flew past instead, and flew on: enemy shells at your tank, then your own shells at enemy tanks.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The panel is 32 monospace columns: a label padded to 20, a space and the value.
    const PANEL_COLUMNS: usize = 32;
    const LABEL_COLUMNS: usize = 20;

    /// The renderer's counts, which a room and single player report alike.
    fn renderer() -> serde_json::Value {
        json!({
            "fps": 60.0, "simMs": 1.234, "renderMs": 2.5,
            "graphicsApi": "WebGPU",
            "quality": {
                "setting": "auto", "tier": "medium", "scale": 0.667, "renderRatio": 1,
                "scene": [1180, 820], "gpuMs": 6.27, "changes": 2, "log": [],
            },
            "drawCalls": 42, "triangles": 628263, "shadowTriangles": 241429,
            "reflectionTriangles": 192922, "mainTriangles": 193912, "meshes": 9,
            "textures": 11, "pixelRatio": 1.5,
        })
    }

    fn game() -> GameStats {
        let mut stats = renderer();
        stats.as_object_mut().unwrap().extend(
            json!({
                "bodies": 10, "fixedBodies": 6, "dynamicBodies": 4, "sleepingBodies": 1,
                "colliders": 4, "gpuBytes": 412.3 * BYTES_PER_MEGABYTE,
                "meshSlackBytes": 12.5 * BYTES_PER_MEGABYTE, "tanks": 3, "tanksAlive": 2,
                "mines": 2, "pickups": 2, "pickupsReady": 1, "shots": 3, "particles": 3,
                "fragments": 1, "maxFragments": 8, "elapsed": 12.34,
            })
            .as_object()
            .unwrap()
            .clone(),
        );
        serde_json::from_value(stats).expect("single player's readings")
    }

    /// A room's `NetGame.stats_json()`: the received scene and the network timeline.
    fn room() -> RoomStats {
        let summary = |count: u64, mean: f64, p95: f64, max: f64| json!({"count": count, "mean": mean, "p50": mean, "p95": p95, "max": max});
        let mut stats = renderer();
        stats.as_object_mut().unwrap().extend(
            json!({
                "scene": {
                    "tanks": 3, "alive": 2, "mines": 2, "pickups": 2, "pickupsReady": 1,
                    "shots": 3, "fragments": 1, "elapsed": 12.34,
                },
                "network": {
                    "rttMs": 42, "receivedUpdates": 7, "snapshotAgeMs": 12, "bufferMs": 80,
                    "marginMs": 20, "underrun": 0, "serverTick": 900, "inputSeq": 10,
                    "inputAck": 9, "connected": true, "lateBatches": 3,
                    "longestBatchGapMs": 412.4, "predictionLeadMs": 120,
                    "correctionMPerS": 0.04, "correctionP95M": 0.03, "inputStarts": 25,
                    "lateInputStarts": 2,
                    "correctionCauses": {
                        "blast": {"count": 1, "mPerS": 0.002},
                        "tank": {"count": 3, "mPerS": 0.012},
                        "crate": {"count": 0, "mPerS": 0},
                        "debris": {"count": 2, "mPerS": 0.02},
                        "cover": {"count": 0, "mPerS": 0},
                        "lateInput": {"count": 0, "mPerS": 0},
                        "other": {"count": 1, "mPerS": 0.006},
                    },
                    "receiveMs": 140, "phantomStrikes": 2,
                },
                "feel": {
                    "viewDelayMs": summary(600, 211.0, 230.0, 260.0),
                    "fireToFlashMs": summary(12, 196.4, 240.0, 251.0),
                    "fireToReportMs": summary(12, 118.0, 150.0, 160.0),
                    "unansweredPresses": 1,
                    "hitOffsetM": summary(5, 1.234, 2.05, 2.1),
                    "incomingOffsetM": summary(4, 0.42, 0.9, 0.9),
                    "incomingBehindM": summary(2, 0.87, 1.3, 1.3),
                    "unseenIncoming": 1,
                    "ownShots": {
                        "predicted": 14, "confirmed": 13, "unconfirmedShells": 1,
                        "unpredicted": 2, "errorM": summary(13, 0.031, 0.08, 0.1),
                        "phantomStrikes": 3,
                    },
                },
            })
            .as_object()
            .unwrap()
            .clone(),
        );
        serde_json::from_value(stats).expect("a room's readings")
    }

    fn titles(sections: &[StatsSection]) -> Vec<&str> {
        sections.iter().map(|section| section.title).collect()
    }

    fn rows(sections: &[StatsSection]) -> impl Iterator<Item = &StatsRow> {
        sections.iter().flat_map(|section| &section.rows)
    }

    fn value<'a>(sections: &'a [StatsSection], label: &str) -> &'a str {
        rows(sections)
            .find(|row| row.label == label)
            .unwrap_or_else(|| panic!("a {label} row"))
            .value
            .as_str()
    }

    fn hint<'a>(sections: &'a [StatsSection], label: &str) -> &'a str {
        &rows(sections)
            .find(|row| row.label == label)
            .unwrap_or_else(|| panic!("a {label} row"))
            .hint
    }

    fn assert_rows_fit(sections: &[StatsSection]) {
        for row in rows(sections) {
            let columns =
                row.label.chars().count().max(LABEL_COLUMNS) + 1 + row.value.chars().count();
            assert!(
                columns <= PANEL_COLUMNS,
                "{} {} fits the panel",
                row.label,
                row.value
            );
        }
    }

    #[test]
    fn single_player_shows_its_sections_and_every_row_explains_itself() {
        let sections = game_sections(&game(), Shell::Page);
        assert_eq!(
            titles(&sections),
            [
                "Performance",
                "Physics",
                "Render",
                "Battle",
                "Configuration"
            ]
        );
        assert_eq!(rows(&sections).count(), 31);
        assert_rows_fit(&sections);
        for row in rows(&sections) {
            assert!(!row.hint.is_empty(), "{} has a hint", row.label);
        }
        for (label, expected) in [
            ("FPS", "60"),
            ("Frame interval", "16.7 ms"),
            ("Sim CPU / frame", "1.23 ms"),
            ("Render CPU / frame", "2.50 ms"),
            ("Bodies", "10"),
            ("Fixed / dynamic", "6 / 4"),
            ("Awake / sleeping", "3 / 1"),
            ("Graphics API", "WebGPU"),
            ("Graphics setting", "Auto"),
            ("Quality tier", "Medium"),
            ("Render scale", "67%"),
            ("Scene pixels", "1180×820"),
            ("GPU frame time", "6.3 ms"),
            ("Draw calls / frame", "42"),
            ("Triangles / frame", "628,263"),
            ("Shadow triangles", "241,429"),
            ("Reflection triangles", "192,922"),
            ("Main triangles", "193,912"),
            ("GPU geometries", "9"),
            ("GPU textures", "11"),
            ("GPU memory", "412.3 MB"),
            ("Mesh page slack", "12.5 MB"),
            ("Tanks", "2 / 3"),
            ("Pickups ready", "1 / 2"),
            ("Debris bodies", "1 / 8"),
            ("Sim time", "12.3s"),
            ("Pixel ratio", "1.5"),
        ] {
            assert_eq!(value(&sections, label), expected, "{label}");
        }
        assert!(hint(&sections, "Draw calls / frame").contains("per rendered frame"));
        assert!(hint(&sections, "Graphics API").contains("WebGL"));
        assert!(hint(&sections, "GPU geometries").contains("uploaded to the GPU"));
        assert!(hint(&sections, "GPU textures").contains("uploaded to the GPU"));
        assert!(hint(&sections, "Render scale").contains("1.00 scene pixels per CSS pixel"));
        assert!(hint(&sections, "Render scale").contains("2 automatic changes"));
    }

    #[test]
    fn a_room_shows_the_received_scene_and_the_network_in_place_of_physics() {
        let sections = room_sections(&room(), NetworkRates::default(), Shell::Page);
        assert_eq!(
            titles(&sections),
            [
                "Performance",
                "Network",
                "Render",
                "Battle",
                "Configuration"
            ]
        );
        assert_rows_fit(&sections);
        for (label, expected) in [
            ("Update CPU / frame", "1.23 ms"),
            ("RTT", "42 ms"),
            ("Input seq sent / ack", "10 / 9"),
            ("Late input starts", "2 / 25"),
            ("Fire to flash", "196 ms"),
            ("Own shots on time", "13 / 14"),
            ("Own shell error", "3.1 cm"),
            ("Hit offset p95", "2.05 m"),
            ("Incoming offset", "0.42 m"),
            ("Hit where you were", "0.87 m"),
            ("Phantom strikes", "2 / 3"),
            ("Debris corrections", "2.0 cm/s"),
            ("Longest batch gap", "412 ms"),
            ("Reflection triangles", "192,922"),
            ("Graphics setting", "Auto"),
            ("Quality tier", "Medium"),
            ("Render scale", "67%"),
            ("Scene pixels", "1180×820"),
            ("GPU frame time", "6.3 ms"),
            ("Tanks", "2 / 3"),
            ("Debris bodies", "1"),
            ("Connection", "Connected"),
        ] {
            assert_eq!(value(&sections, label), expected, "{label}");
        }
        assert!(!rows(&sections).any(|row| row.label == "Sim CPU / frame"));
        // The allocation estimates are single player's.
        assert!(!rows(&sections).any(|row| row.label == "GPU memory"));
        assert!(hint(&sections, "Debris corrections").contains("(2 so far)"));
        assert!(hint(&sections, "Fire to flash").contains("118 ms of it"));
    }

    #[test]
    fn correction_causes_follow_the_engines_order() {
        let sections = room_sections(&room(), NetworkRates::default(), Shell::Page);
        let network = &sections[1].rows;
        let first = network
            .iter()
            .position(|row| row.label == "Blast corrections")
            .expect("cause rows");
        let labels: Vec<_> = network[first..first + CorrectionCause::ALL.len()]
            .iter()
            .map(|row| row.label)
            .collect();
        let expected: Vec<_> = CorrectionCause::ALL
            .into_iter()
            .map(|cause| cause_words(cause).0)
            .collect();
        assert_eq!(labels, expected);
        assert_eq!(value(&sections, "Tank corrections"), "1.2 cm/s");
        assert_eq!(value(&sections, "Other corrections"), "0.6 cm/s");
    }

    #[test]
    fn a_room_before_its_first_scene_has_no_battle_rows() {
        let stats = RoomStats {
            scene: None,
            ..room()
        };
        let sections = room_sections(&stats, NetworkRates::default(), Shell::Page);
        assert_eq!(sections[3].title, "Battle");
        assert!(sections[3].rows.is_empty());
    }

    #[test]
    fn nothing_measured_reads_as_a_dash() {
        let mut stats = room();
        stats.frame = FrameStats::default();
        stats.renderer.quality.gpu_ms = None;
        stats.feel = FeelStats::default();
        let sections = room_sections(&stats, NetworkRates::default(), Shell::Page);
        for label in [
            "FPS",
            "Frame interval",
            "Update CPU / frame",
            "Render CPU / frame",
            "GPU frame time",
            "Fire to flash",
            "Own shell error",
            "Hit offset",
            "Incoming offset",
        ] {
            assert_eq!(value(&sections, label), NONE, "{label}");
        }
    }

    #[test]
    fn the_update_rate_is_measured_between_readings() {
        let mut sampler = NetworkSampler::default();
        let mut connection = ConnectionStats {
            received_updates: 100,
            receive_ms: 10.0,
            ..ConnectionStats::default()
        };
        assert_eq!(sampler.sample(1000.0, &connection), NetworkRates::default());
        connection.received_updates = 160;
        connection.receive_ms = 12.0;
        let rates = sampler.sample(2000.0, &connection);
        assert!((rates.updates_per_second - 60.0).abs() < 1e-9);
        assert!((rates.receive_ms_per_second - 2.0).abs() < 1e-9);
        let sections = room_sections(&room(), rates, Shell::Page);
        assert_eq!(value(&sections, "Update rate"), "60.0 /s");
        assert_eq!(value(&sections, "Snapshot CPU"), "2.0 ms/s");
        // A repeated reading at the same moment measures nothing.
        assert_eq!(sampler.sample(2000.0, &connection), NetworkRates::default());
    }

    #[test]
    fn each_shell_names_itself_and_its_graphics() {
        let page = room_sections(&room(), NetworkRates::default(), Shell::Page);
        let desktop = room_sections(&room(), NetworkRates::default(), Shell::Desktop);
        assert!(hint(&page, "Late batches").contains("this page session"));
        assert!(hint(&page, "Late batches").contains("page itself froze"));
        assert!(hint(&desktop, "Late batches").contains("during this session"));
        assert!(hint(&desktop, "Late batches").contains("app itself froze"));
        assert!(hint(&desktop, "Graphics API").contains("Metal"));
        assert!(!hint(&desktop, "GPU frame time").contains("WebGPU"));
        assert!(hint(&desktop, "Pixel ratio").ends_with("from the display's."));
        assert!(hint(&page, "Pixel ratio").ends_with("from the display pixel ratio."));
        // Values are the same text in both shells.
        let values = |sections: &[StatsSection]| -> Vec<String> {
            rows(sections).map(|row| row.value.clone()).collect()
        };
        assert_eq!(values(&page), values(&desktop));
    }

    #[test]
    fn numbers_have_one_format() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
        assert_eq!(millis(-0.4), "0 ms");
        assert_eq!(millis(-2.5), "-2 ms");
        assert_eq!(millis(2.5), "3 ms");
        assert_eq!(pair(24_790, 24_781), "24790 / 24781");
    }

    #[test]
    fn the_page_reads_sections_of_labelled_rows() {
        let sections = game_sections(&game(), Shell::Page);
        let text = serde_json::to_value(&sections[0]).expect("a section serializes");
        assert_eq!(text["title"], "Performance");
        assert_eq!(text["rows"][0]["label"], "FPS");
        assert_eq!(text["rows"][0]["value"], "60");
        assert!(text["rows"][0]["hint"].as_str().is_some());
    }
}
