//! Page plumbing shared by [`Game`](crate::Game) and [`NetGame`](crate::NetGame) (and
//! the labs): errors and JSON for the page, the clock, frame timing, canvas sizing,
//! arena preparation results, the event queue and the local controls' aim.

use glam::Vec2;
use serde::Deserialize;
use sloppy_core::sim::{SimEvent, SimEventType};
use sloppy_render::gpu::Renderer;
use sloppy_render::presentation::camera_rig::CameraRig;
use sloppy_render::presentation::input::InputFrame;
use sloppy_render::presentation::view_settings::CAMERA;
use sloppy_render::presentation::{PrepareStatus, Presentation};
use wasm_bindgen::JsValue;

use crate::events::PendingEvent;

/// Frame deltas are capped so a stalled tab never fast-forwards the match.
const MAX_FRAME_DELTA_SECONDS: f64 = 0.1;
const MILLISECONDS_PER_SECOND: f64 = 1000.0;
/// The HUD refreshes (`hud_json`) every this many frames.
pub const HUD_UPDATE_EVERY_FRAMES: u64 = 4;
/// Events waiting for `drain_events`; a hidden HUD must not grow this without bound.
const MAX_PENDING_EVENTS: usize = 2048;
/// `prepare_step`'s result when nothing is left to prepare.
pub const PREPARED: [f64; 5] = [0.0, 0.0, 0.0, 1.0, 0.0];

pub fn js_error(message: impl Into<String>) -> JsValue {
    js_sys::Error::new(&message.into()).into()
}

/// Page JSON, rejected with serde's message when it does not fit `T`.
pub fn parse<'a, T: Deserialize<'a>>(json: &'a str) -> Result<T, JsValue> {
    serde_json::from_str(json).map_err(|error| js_error(error.to_string()))
}

thread_local! {
    // Looked up once: each frame reads the clock several times.
    static PERFORMANCE: Option<web_sys::Performance> =
        web_sys::window().and_then(|window| window.performance());
}

pub fn now_ms() -> f64 {
    PERFORMANCE.with(|performance| performance.as_ref().map_or(0.0, |p| p.now()))
}

/// The animation-frame clock and the timings of the last frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTimes {
    /// The latest frame timestamp, in milliseconds.
    pub last_ms: Option<f64>,
    pub frame_ms: f64,
    pub sim_ms: f64,
    pub render_ms: f64,
    /// Exponential average of frame time for the fps readout.
    pub average_ms: f64,
}

impl FrameTimes {
    /// Move the clock to the frame at `now` and return its capped delta in seconds. A
    /// queued timestamp can precede a slow rebuild; time never runs backwards.
    pub fn advance(&mut self, now: f64) -> f64 {
        let last = self.last_ms.unwrap_or(now);
        let raw = ((now - last) / MILLISECONDS_PER_SECOND).max(0.0);
        self.last_ms = Some(last.max(now));
        self.frame_ms = raw * MILLISECONDS_PER_SECOND;
        if raw > 0.0 {
            self.average_ms += (self.frame_ms - self.average_ms) * 0.05;
        }
        raw.min(MAX_FRAME_DELTA_SECONDS)
    }

    pub fn fps(&self) -> f64 {
        if self.average_ms > 0.0 {
            MILLISECONDS_PER_SECOND / self.average_ms
        } else {
            0.0
        }
    }
}

/// The canvas's CSS size and the device pixel ratio, which size the drawing buffer.
#[derive(Clone, Copy, Debug)]
pub struct CanvasSize {
    pub css: Vec2,
    pub device_pixel_ratio: f64,
    /// Draw at exactly the CSS size (ratio 1).
    pub exact: bool,
}

impl CanvasSize {
    /// Drawing-buffer pixels per CSS pixel: the device ratio capped at
    /// `CAMERA.max_pixel_ratio`, or 1 when `exact`.
    pub fn pixel_ratio(&self) -> f64 {
        if self.exact {
            1.0
        } else {
            self.device_pixel_ratio.min(CAMERA.max_pixel_ratio)
        }
    }

    pub fn apply(&self, view: &mut Presentation) {
        let ratio = self.pixel_ratio();
        let width = (f64::from(self.css.x) * ratio).round().max(1.0) as u32;
        let height = (f64::from(self.css.y) * ratio).round().max(1.0) as u32;
        view.resize(width, height);
    }
}

/// The configuration both game types take: `{ seed?, assetBase, cssWidth?, cssHeight?,
/// pixelRatio?, firstPerson?, zoom?, hideReticle? }`.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ViewConfig {
    seed: Option<f64>,
    asset_base: String,
    css_width: Option<f64>,
    css_height: Option<f64>,
    pixel_ratio: Option<f64>,
    first_person: bool,
    zoom: Option<f64>,
    hide_reticle: bool,
}

/// The renderer on `canvas` and its presentation with the saved camera preferences, the
/// canvas size to apply and the seed (random when `config_json` has none). Rejects when
/// the renderer cannot start.
pub async fn create_view(
    canvas: web_sys::HtmlCanvasElement,
    config_json: &str,
) -> Result<(Presentation, CanvasSize, f64), JsValue> {
    let config: ViewConfig = parse(config_json)?;
    let seed = config
        .seed
        .unwrap_or_else(|| (js_sys::Math::random() * 1e9).floor());
    let size = CanvasSize {
        css: Vec2::new(
            config.css_width.unwrap_or(f64::from(canvas.client_width())) as f32,
            config
                .css_height
                .unwrap_or(f64::from(canvas.client_height())) as f32,
        ),
        device_pixel_ratio: config.pixel_ratio.unwrap_or(1.0),
        exact: false,
    };
    let renderer = Renderer::new(canvas, config.asset_base)
        .await
        .map_err(js_error)?;
    let mut view = Presentation::new(renderer, seed.to_bits());
    view.rig
        .restore_preferences(config.first_person, config.zoom);
    view.hide_reticle = config.hide_reticle;
    Ok((view, size, seed))
}

/// `prepare_step`'s `[compiled, remaining, texturesPending, done, gpuPending]`.
pub fn prepare_result(status: &PrepareStatus) -> Vec<f64> {
    let flag = |value: bool| if value { 1.0 } else { 0.0 };
    vec![
        f64::from(status.compiled),
        f64::from(status.remaining),
        f64::from(status.textures_pending),
        flag(status.ready),
        flag(status.gpu_pending),
    ]
}

/// Show `event` and queue it for `drain_events` with the screen angle of damage the
/// viewer took, dropping the oldest event once the queue is full.
pub fn queue_event(
    events: &mut Vec<PendingEvent>,
    view: &mut Presentation,
    event: SimEvent,
    player_hit: bool,
    own: bool,
) {
    view.event(&event, player_hit);
    let hurt = matches!(event.kind, SimEventType::Hurt | SimEventType::Death);
    let damage_angle = (own && hurt).then(|| view.damage_angle(&event)).flatten();
    if events.len() >= MAX_PENDING_EVENTS {
        events.remove(0);
    }
    events.push(PendingEvent {
        event,
        player_hit,
        own,
        damage_angle,
    });
}

/// The local controls' aim from the tank at `(x, z)`: the first-person yaw (turned by
/// the look and the drive stick's sideways push when `turn`), or the angle to the
/// pointer on the ground, which is also returned.
pub fn aim(
    rig: &mut CameraRig,
    input: &InputFrame,
    (x, z): (f64, f64),
    dt: f64,
    turn: bool,
) -> (f64, Option<(f64, f64)>) {
    if rig.first_person.enabled {
        if turn {
            let stick = f64::from(input.stick_turn);
            rig.first_person.turn(input.look_pixels, stick, dt);
        }
        (rig.first_person.yaw, None)
    } else {
        let target = rig.aim(Vec2::new(input.pointer.0, input.pointer.1));
        let (tx, tz) = (f64::from(target.x), f64::from(target.z));
        ((tx - x).atan2(tz - z), Some((tx, tz)))
    }
}
