//! Page plumbing shared by [`Game`](crate::Game) and [`NetGame`](crate::NetGame) (and
//! the labs): errors and JSON for the page, the clock, canvas sizing, the graphics
//! quality, arena preparation results, the event queue and the local controls' aim.
//! Frame timing is [`crate::frame_clock`].

use glam::Vec2;
use serde::Deserialize;
use serde_json::{Value, json};
use sloppy_core::sim::{SimEvent, SimEventType};
use sloppy_render::gpu::{RenderTarget, Renderer};
use sloppy_render::presentation::camera_rig::CameraRig;
use sloppy_render::presentation::input::InputFrame;
use sloppy_render::presentation::view_settings::CAMERA;
use sloppy_render::presentation::{PrepareStatus, Presentation};
use sloppy_render::quality::{
    DeviceClass, GraphicsSetting, QualityController, Tier, starting_tier,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

use crate::events::PendingEvent;
use crate::frame_clock::{FramePacing, FrameTimes};

/// The HUD refreshes (`hud_json`) every this many frames.
pub const HUD_UPDATE_EVERY_FRAMES: u64 = 4;
/// Events waiting for `drain_events`; a hidden HUD must not grow this without bound.
const MAX_PENDING_EVENTS: usize = 2048;
/// Preparation's status when nothing is left to prepare.
pub const PREPARED: PrepareStatus = PrepareStatus {
    compiled: 0,
    remaining: 0,
    textures_pending: 0,
    gpu_pending: false,
    ready: true,
};

/// The view the player chose, which the page saves after a camera input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraPreferences {
    pub first_person: bool,
    /// The overhead zoom, clamped to the camera's range.
    pub zoom: f64,
}

impl CameraPreferences {
    pub fn of(rig: &CameraRig) -> Self {
        let [first_person, zoom] = rig.preferences();
        Self {
            first_person: first_person == 1.0,
            zoom,
        }
    }

    /// `camera_preferences`' `[firstPerson, zoom]`.
    pub fn to_vec(self) -> Vec<f64> {
        vec![f64::from(u8::from(self.first_person)), self.zoom]
    }
}

/// What a failed call rejects with: a JS `Error` on the page, its message natively.
#[cfg(target_arch = "wasm32")]
pub type PageError = JsValue;
#[cfg(not(target_arch = "wasm32"))]
pub type PageError = String;

#[cfg(target_arch = "wasm32")]
pub fn js_error(message: impl Into<String>) -> PageError {
    js_sys::Error::new(&message.into()).into()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn js_error(message: impl Into<String>) -> PageError {
    message.into()
}

/// Page JSON, rejected with serde's message when it does not fit `T`.
pub fn parse<'a, T: Deserialize<'a>>(json: &'a str) -> Result<T, PageError> {
    serde_json::from_str(json).map_err(|error| js_error(error.to_string()))
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    // Looked up once: each frame reads the clock several times.
    static PERFORMANCE: Option<web_sys::Performance> =
        web_sys::window().and_then(|window| window.performance());
}

#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> f64 {
    PERFORMANCE.with(|performance| performance.as_ref().map_or(0.0, |p| p.now()))
}

/// Milliseconds since the process's first reading: the clock natively, where the
/// shell passes frame times from the same origin ([`clock_start`]).
#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> f64 {
    clock_start().elapsed().as_secs_f64() * 1000.0
}

/// The origin of [`now_ms`] natively.
#[cfg(not(target_arch = "wasm32"))]
pub fn clock_start() -> std::time::Instant {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    *START.get_or_init(std::time::Instant::now)
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
        view.css_height = f64::from(self.css.y);
    }
}

/// How far each new GPU frame time sample moves the smoothed one.
const GPU_TIME_SMOOTHING: f64 = 0.1;

/// The graphics tier and render scale: the page's Graphics setting and the device
/// choose where the quality controller starts, and under Auto it adapts them to the
/// interval of every gameplay frame (`sloppy_render::quality`).
pub struct Graphics {
    controller: QualityController,
    /// The GPU's frame time, smoothed over recent samples, for Stats for nerds.
    gpu_ms: Option<f64>,
}

impl Graphics {
    /// Start from `setting` ("auto", "high", "medium", "low") on `device` ("desktop",
    /// "tablet", "phone"), and draw at the controller's first rung.
    pub fn new(renderer: &mut Renderer, setting: &str, device: &str, pixel_ratio: f64) -> Self {
        let webgl = sloppy_render::GRAPHICS_API == "WebGL";
        let start = starting_tier(
            DeviceClass::parse(device),
            renderer.max_texture_size(),
            webgl,
        );
        let class = DeviceClass::parse(device);
        let mut controller =
            QualityController::new(GraphicsSetting::parse(setting), start, pixel_ratio);
        if class != DeviceClass::Desktop {
            controller = controller.with_deep_floor();
        }
        let graphics = Self {
            controller,
            gpu_ms: None,
        };
        graphics.apply(renderer);
        graphics
    }

    fn apply(&self, renderer: &mut Renderer) {
        let rung = self.controller.rung();
        renderer.set_quality(rung.tier, rung.scale);
    }

    /// After drawing a frame: under Auto, a gameplay frame's interval, main-thread
    /// work and the GPU's frame time where sampled feed the controller; other frames
    /// (menus, pause, results) interrupt it. Only frames that ran come here, so with
    /// `framesInFlight` the interval spans the animation frames skipped while the GPU
    /// caught up: those are the slow frames the player sees, not fast ones.
    pub fn frame(&mut self, renderer: &mut Renderer, times: &FrameTimes, playing: bool) {
        let gpu_ms = renderer.take_gpu_frame_ms();
        if let Some(sample) = gpu_ms {
            let smoothed = self.gpu_ms.unwrap_or(sample);
            self.gpu_ms = Some(smoothed + (sample - smoothed) * GPU_TIME_SMOOTHING);
        }
        if !playing {
            self.controller.interrupted();
            return;
        }
        if self
            .controller
            .observe(times.frame_ms, times.sim_ms + times.render_ms, gpu_ms)
        {
            self.apply(renderer);
        }
        if self.controller.take_pacing_reset() {
            renderer.reset_canvas_pacing();
        }
    }

    /// The Settings dialog's Graphics choice.
    pub fn set_setting(&mut self, renderer: &mut Renderer, setting: &str) {
        if self.controller.set_setting(GraphicsSetting::parse(setting)) {
            self.apply(renderer);
        }
    }

    /// The canvas moved to a display of another pixel ratio.
    pub fn set_pixel_ratio(&mut self, renderer: &mut Renderer, pixel_ratio: f64) {
        if self.controller.set_pixel_ratio(pixel_ratio) {
            self.apply(renderer);
        }
    }

    /// A new round's arena: its first seconds are not judged.
    pub fn round_started(&mut self) {
        self.controller.round_started();
    }

    /// The GPU's frame time in milliseconds, smoothed over recent samples, once sampled.
    pub fn gpu_ms(&self) -> Option<f64> {
        self.gpu_ms
    }

    /// Stats for nerds' `quality` rows: the setting, tier, render scale, the scene's
    /// size and the recent changes.
    pub fn stats(&self, renderer: &Renderer, pixel_ratio: f64) -> Value {
        let (tier, scale) = renderer.quality();
        let (width, height) = renderer.scene_size();
        let log: Vec<Value> = self
            .controller
            .log()
            .iter()
            .map(|change| {
                json!({
                    "at": change.at,
                    "tier": change.to.tier.name(),
                    "scale": change.to.scale,
                    "reason": change.reason.name(),
                    "meanMs": change.mean_ms,
                    "gpuMs": change.gpu_ms,
                })
            })
            .collect();
        json!({
            "setting": self.controller.setting().name(),
            "tier": tier.name(),
            "scale": scale,
            "renderRatio": f64::from(scale) * pixel_ratio,
            "scene": [width, height],
            "gpuMs": self.gpu_ms,
            "changes": self.controller.changes(),
            "pacingResets": self.controller.pacing_resets(),
            "log": log,
        })
    }
}

/// Whether the renderer multisamples, decided once per page since every pipeline is
/// made for it: desktops do; touch tablets and phones draw single-sampled, since four
/// samples of the HDR target and its depth cost a tile-based mobile GPU dearly in
/// fill and bandwidth, unless the player chose High.
fn multisampled(device: &str, setting: &str) -> bool {
    DeviceClass::parse(device) == DeviceClass::Desktop
        || GraphicsSetting::parse(setting) == GraphicsSetting::Fixed(Tier::High)
}

/// The configuration both game types take: `{ seed?, assetBase, cssWidth?, cssHeight?,
/// pixelRatio?, firstPerson?, zoom?, hideReticle?, graphics?, device?,
/// framesInFlight? }`. `framesInFlight` runs frames only while fewer are on the GPU,
/// for browsers that queue a WebGPU canvas's frames without holding animation frames
/// back ([`FramePacing`]; 0, the default, leaves the pacing to the browser).
#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ViewConfig {
    pub seed: Option<f64>,
    pub asset_base: String,
    pub css_width: Option<f64>,
    pub css_height: Option<f64>,
    pub pixel_ratio: Option<f64>,
    pub first_person: bool,
    pub zoom: Option<f64>,
    pub hide_reticle: bool,
    /// The saved Graphics setting; Auto when absent.
    pub graphics: String,
    /// "desktop", "tablet" or "phone" (`src/game/graphics.ts`).
    pub device: String,
    pub frames_in_flight: u32,
}

/// What [`create_view`] sets up for a game type.
pub struct PageView {
    pub view: Presentation,
    /// The canvas size to apply.
    pub canvas: CanvasSize,
    pub graphics: Graphics,
    /// The configured seed, or a random one.
    pub seed: f64,
    pub pacing: FramePacing,
}

/// The renderer on `canvas` and its presentation with the saved camera preferences, the
/// canvas size to apply, the graphics quality, the seed (random when `config` has none)
/// and the frame pacing. Rejects when the renderer cannot start.
pub async fn create_view(canvas: RenderTarget, config: ViewConfig) -> Result<PageView, PageError> {
    let seed = config.seed.unwrap_or_else(|| (random() * 1e9).floor());
    #[cfg(target_arch = "wasm32")]
    let client = (canvas.client_width(), canvas.client_height());
    // A window's size natively is in device pixels.
    #[cfg(not(target_arch = "wasm32"))]
    let client = {
        let ratio = config.pixel_ratio.unwrap_or(1.0);
        (
            (f64::from(canvas.width) / ratio) as i32,
            (f64::from(canvas.height) / ratio) as i32,
        )
    };
    let size = CanvasSize {
        css: Vec2::new(
            config.css_width.unwrap_or(f64::from(client.0)) as f32,
            config.css_height.unwrap_or(f64::from(client.1)) as f32,
        ),
        device_pixel_ratio: config.pixel_ratio.unwrap_or(1.0),
        exact: false,
    };
    let msaa = multisampled(&config.device, &config.graphics);
    let mut renderer = Renderer::new(canvas, config.asset_base, msaa)
        .await
        .map_err(js_error)?;
    let graphics = Graphics::new(
        &mut renderer,
        &config.graphics,
        &config.device,
        size.pixel_ratio(),
    );
    let mut view = Presentation::new(renderer, seed.to_bits());
    view.rig
        .restore_preferences(config.first_person, config.zoom);
    view.hide_reticle = config.hide_reticle;
    Ok(PageView {
        view,
        canvas: size,
        graphics,
        seed,
        pacing: FramePacing::new(config.frames_in_flight),
    })
}

/// A uniform draw in `[0, 1)` for an unseeded game.
#[cfg(target_arch = "wasm32")]
fn random() -> f64 {
    js_sys::Math::random()
}

/// A uniform draw in `[0, 1)` for an unseeded game, from the standard library's
/// per-process random hash keys.
#[cfg(not(target_arch = "wasm32"))]
fn random() -> f64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(clock_start().elapsed().as_nanos());
    (hasher.finish() >> 11) as f64 / (1u64 << 53) as f64
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
