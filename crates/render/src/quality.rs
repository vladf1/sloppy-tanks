//! Graphics quality: which optional per-pixel features the renderer runs (the
//! [`Tier`]) and how many pixels it shades (the render scale), chosen from the device
//! and, under the Auto setting, adapted to measured frame intervals.
//!
//! The scene renders into a target `scale` times the canvas drawing buffer and the
//! output pass upsamples it into the canvas, so the canvas and the DOM HUD never change
//! size. Auto walks a ladder of [`Rung`]s from the richest picture to the cheapest:
//! within a tier the scale falls in small steps down to that tier's least render
//! pixels per CSS pixel, then the next cheaper tier takes over at the same scale. It
//! steps down one rung after sustained slow frames. A descent that has halved the
//! pixels without making frames faster is undone and held off for a while: the pixels
//! were not what made them slow, and a blurrier picture would only hide that. It
//! probes one rung back up after a stretch of comfortable frames and returns at once
//! when the probe is slow, waiting longer before each retry of that rung, up to a few
//! minutes. Nothing it learns outlives the page, so a device whose browser or build is
//! fixed starts afresh at its starting tier and climbs as far as its frames allow.
//!
//! Two signals drive it. The animation frame interval, which every browser reports,
//! says whether the player sees slow frames. Where the browser reports when the GPU
//! finished a frame (WebGPU's `onSubmittedWorkDone`; GPU timestamps are not offered
//! everywhere), the GPU's frame time says whether the GPU is what makes them slow and
//! how much room it has. It matters in WebKit, which paces a WebGPU canvas whose GPU
//! work overran a display frame at 30, 24 or 15 fps and does not raise the rate again
//! when the work gets cheaper: there the interval stays slow whatever Auto does, so
//! only the GPU time can show that a step down helped or that a richer rung fits. On
//! WebGL, with the interval alone, the interval cannot show headroom while frames
//! keep up with the display, hence the probes.
//!
//! WebKit's lowered rate is also a smoothness bug of its own: one explosion or loading
//! hitch that overruns a display frame leaves the canvas at 15-30 fps for the rest of
//! the session, however cheap the frames become, and reconfiguring the canvas restores
//! the display rate. So under every setting, frames held slow while the GPU has room
//! ask for a pacing reset ([`QualityController::take_pacing_reset`]), retried with
//! growing waits while it does not help.

/// Optional per-pixel features, from cheapest to richest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Low,
    Medium,
    High,
}

impl Tier {
    pub const fn name(self) -> &'static str {
        match self {
            Tier::Low => "low",
            Tier::Medium => "medium",
            Tier::High => "high",
        }
    }

    /// What this tier draws. Today's full picture is High; each cheaper tier drops
    /// the optional features whose cost scales with the pixels they cover and that
    /// the eye misses least from the overhead camera. Medium drops the procedural
    /// detail a few pixels across and the ambient occlusion pass (with the
    /// multisampled depth it stores and the output's taps); Low also drops bloom and
    /// halves the sun shadow map.
    pub const fn features(self) -> TierFeatures {
        match self {
            Tier::High => TierFeatures {
                fine_detail: true,
                occlusion: true,
                bloom: true,
                max_shadow_map: u32::MAX,
            },
            Tier::Medium => TierFeatures {
                fine_detail: false,
                occlusion: false,
                bloom: true,
                max_shadow_map: u32::MAX,
            },
            Tier::Low => TierFeatures {
                fine_detail: false,
                occlusion: false,
                bloom: false,
                max_shadow_map: LOW_SHADOW_MAP,
            },
        }
    }
}

/// The sun shadow map edge the Low tier keeps at most.
const LOW_SHADOW_MAP: u32 = 1024;

/// The optional features a tier keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TierFeatures {
    /// Procedural detail a few pixels across: the ground's small hummocks over its
    /// broad swells, the clouds' finest octave, and on the village, harbor and quarry
    /// grounds their scattered stones, grit and blooms, the turf's tussocks and the
    /// finest noise octaves (`fine_detail` in the effect shaders).
    pub fine_detail: bool,
    /// The ambient occlusion pass and its taps in the output pass.
    pub occlusion: bool,
    /// The bloom chain.
    pub bloom: bool,
    /// The largest sun shadow map edge, in texels.
    pub max_shadow_map: u32,
}

/// The Settings dialog's Graphics choice: Auto adapts the tier and the render scale;
/// a fixed tier draws at the full canvas resolution and never adapts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphicsSetting {
    #[default]
    Auto,
    Fixed(Tier),
}

impl GraphicsSetting {
    /// "auto", "high", "medium" or "low"; anything else is Auto.
    pub fn parse(value: &str) -> Self {
        match value {
            "high" => GraphicsSetting::Fixed(Tier::High),
            "medium" => GraphicsSetting::Fixed(Tier::Medium),
            "low" => GraphicsSetting::Fixed(Tier::Low),
            _ => GraphicsSetting::Auto,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            GraphicsSetting::Auto => "auto",
            GraphicsSetting::Fixed(tier) => tier.name(),
        }
    }
}

/// The kind of device the page runs on (`src/game/graphics.ts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeviceClass {
    #[default]
    Desktop,
    /// A touch tablet such as an iPad.
    Tablet,
    Phone,
}

impl DeviceClass {
    /// "tablet" or "phone"; anything else is a desktop.
    pub fn parse(value: &str) -> Self {
        match value {
            "tablet" => DeviceClass::Tablet,
            "phone" => DeviceClass::Phone,
            _ => DeviceClass::Desktop,
        }
    }
}

/// Below this largest texture edge the GPU is an old or small one.
const SMALL_GPU_TEXTURE_SIZE: u32 = 8192;

/// The tier Auto starts from, on its richest rung. Desktops start with the full
/// picture and step down only if their frames show they must. Touch tablets and
/// phones start on Medium, at one render pixel per CSS pixel: a tile-based mobile
/// GPU pays for every pixel of the multisampled HDR targets in bandwidth and memory
/// (an iPad Pro M4 drew the full 2064×1548 picture at 7 fps), and Auto climbs to the
/// full picture where the GPU's frame time shows room. The WebGL fallback means an
/// older browser or device and starts a tier lower, as does any GPU with small
/// limits. Nothing here keys on the browser's name, which says nothing about the
/// hardware.
pub fn starting_tier(device: DeviceClass, max_texture_size: u32, webgl: bool) -> Tier {
    if max_texture_size < SMALL_GPU_TEXTURE_SIZE {
        return Tier::Low;
    }
    match (webgl, device) {
        (false, DeviceClass::Desktop) => Tier::High,
        (false, DeviceClass::Tablet | DeviceClass::Phone) | (true, DeviceClass::Desktop) => {
            Tier::Medium
        }
        (true, DeviceClass::Tablet | DeviceClass::Phone) => Tier::Low,
    }
}

/// Touch tablets and phones may go below Low's floor, down to this many render pixels
/// per CSS pixel, but only while frames take longer than this many milliseconds:
/// a mushier picture beats a slide show.
const DEEP_MIN_RATIO: f64 = 0.5;
const DEEP_SLOW_MS: f64 = 50.0;

/// One step of the ladder: a tier and the render scale (scene pixels per canvas
/// pixel, along each axis).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rung {
    pub tier: Tier,
    pub scale: f32,
}

/// The least render pixels per CSS pixel each tier scales down to before Auto hands
/// over to the next cheaper tier. Low's is the floor: below about 0.6 the picture
/// goes mushy on a phone or tablet.
const fn tier_min_ratio(tier: Tier) -> f64 {
    match tier {
        Tier::High => 1.0,
        Tier::Medium => 0.8,
        Tier::Low => 0.6,
    }
}

/// Each scale step keeps this share of the scale along each axis: 19% fewer pixels.
const SCALE_STEP: f64 = 0.9;
/// A tier's last step down to its floor is skipped when it would shave off less than
/// this share of the scale.
const MIN_LAST_STEP: f64 = 0.03;

/// The rungs Auto walks for a canvas of `pixel_ratio` drawing-buffer pixels per CSS
/// pixel, richest first; a fixed setting has its one rung at full scale.
pub fn ladder(setting: GraphicsSetting, pixel_ratio: f64) -> Vec<Rung> {
    if let GraphicsSetting::Fixed(tier) = setting {
        return vec![Rung { tier, scale: 1.0 }];
    }
    let mut rungs = Vec::new();
    let mut scale = 1.0;
    for tier in [Tier::High, Tier::Medium, Tier::Low] {
        rungs.push(Rung {
            tier,
            scale: scale as f32,
        });
        scale = descend(&mut rungs, tier, scale, tier_min_ratio(tier) / pixel_ratio);
    }
    rungs
}

/// Push `tier`'s rungs below `scale` in steps down to `floor` (a scale); returns the
/// last scale.
fn descend(rungs: &mut Vec<Rung>, tier: Tier, mut scale: f64, floor: f64) -> f64 {
    let floor = floor.min(1.0);
    loop {
        let next = scale * SCALE_STEP;
        if next >= floor {
            scale = next;
        } else if scale > floor * (1.0 + MIN_LAST_STEP) {
            scale = floor;
        } else {
            return scale;
        }
        rungs.push(Rung {
            tier,
            scale: scale as f32,
        });
    }
}

/// The display rate Auto holds: 60 Hz frames, whatever the display's own rate.
pub const TARGET_FRAME_MS: f64 = 1000.0 / 60.0;
/// A window whose mean interval exceeds the target by this factor drops frames often
/// enough to see (50 fps or less).
const SLOW_FACTOR: f64 = 1.2;
/// A window within this factor of the target keeps up with the display.
const COMFORT_FACTOR: f64 = 1.04;
/// An interval longer than this is a stall (a hidden tab, a long task, a collection),
/// not a measure of the frame's cost, and is ignored.
const STALL_MS: f64 = 250.0;
/// Frames are judged in windows of this much gameplay time.
const WINDOW_SECONDS: f64 = 1.0;
/// No decision in the first seconds of a round, while the arena's first frames,
/// late uploads and effects settle.
const STARTUP_SECONDS: f64 = 4.0;
/// Frames right after a change are not judged: new targets allocate, the GPU's queue
/// drains.
const SETTLE_SECONDS: f64 = 1.0;
/// Consecutive slow windows before a step down, or a single one this many times the
/// target (under 27 fps, below a canvas paced at 30), so a weak device that starts
/// with the full picture reaches a smooth rung in seconds, still one small step at a
/// time.
const DOWN_AFTER_WINDOWS: u32 = 2;
const VERY_SLOW_FACTOR: f64 = 2.2;
/// Comfortable gameplay before a probe one rung up.
const UP_AFTER_SECONDS: f64 = 8.0;
/// A probe passes after this long without a slow window, or one window after it
/// settles where the GPU's time shows the cost at once.
const PROBATION_SECONDS: f64 = 4.0;
const GPU_PROBATION_SECONDS: f64 = 1.0;
/// The first wait before Auto probes a rung it fell back from, doubling with each
/// failure up to the longest. Bounded, so no rung is given up for good.
const FIRST_BACKOFF_SECONDS: f64 = 20.0;
const MAX_BACKOFF_SECONDS: f64 = 160.0;
/// The first hold on stepping down after a descent that bought nothing, doubling
/// with each such descent up to the longest: each retry costs seconds of a blurrier
/// picture for no gain, so they are rarer than probes.
const FIRST_HOLD_SECONDS: f64 = 60.0;
const MAX_HOLD_SECONDS: f64 = 240.0;
/// A descent is judged once its pixels are at most this share of where it began:
/// single steps are too small to judge, since a browser paces a slow canvas at whole
/// fractions of the display rate (WebKit drops it to 30 or 24 fps), so a 19% step can
/// leave the interval unchanged even when the pixels are the whole cost.
const DESCENT_JUDGED_AT_PIXELS: f64 = 0.5;
/// A judged descent that cut the mean interval by less than this share bought nothing.
const MIN_DESCENT_GAIN: f64 = 0.1;
/// While the main thread's own work fills this share of the interval, frames are
/// slow on the CPU and fewer pixels would not help.
const CPU_BOUND_SHARE: f64 = 0.75;
/// With the GPU's frame time known: slow frames step down only while it exceeds this
/// share of the target (the GPU is what holds them back), and Auto probes a richer
/// rung once its expected GPU time fits within the lower share, after a shorter
/// stretch than the interval alone needs. Windows with fewer samples than the least
/// judge by the interval alone.
const GPU_BUSY_SHARE: f64 = 0.85;
const GPU_ROOM_SHARE: f64 = 0.7;
const UP_AFTER_GPU_SECONDS: f64 = 3.0;
const MIN_GPU_SAMPLES: usize = 5;
/// A richer tier at the same scale is expected to cost the GPU up to this factor more.
const TIER_STEP_COST: f64 = 1.25;
/// Frames whose mean interval exceeds the target by this factor (37 fps or less) while
/// the GPU stays within its busy share for this many windows are held back by the
/// browser's pacing, not by their cost: Auto asks for a pacing reset, first after
/// the shorter wait and then, while frames stay held, after doubling waits.
const PACED_FACTOR: f64 = 1.6;
const PACED_WINDOWS: u32 = 2;
const FIRST_PACING_RETRY_SECONDS: f64 = 10.0;
const MAX_PACING_RETRY_SECONDS: f64 = 120.0;
/// GPU samples one window keeps.
const MAX_GPU_SAMPLES: usize = 256;
/// Changes the log keeps for Stats for nerds.
const LOG_CAPACITY: usize = 24;

/// Why Auto changed rung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeReason {
    /// Sustained slow frames.
    Slow,
    /// A descent that halved the pixels without making frames faster, undone.
    Ineffective,
    /// Comfortable frames: a probe one rung up.
    Probe,
    /// The probe's frames were slow: back to the rung it left.
    ProbeFailed,
    /// Frames held slow by the browser's pacing while the GPU had room: the canvas's
    /// pacing was reset, the rung kept.
    PacingReset,
}

impl ChangeReason {
    pub const fn name(self) -> &'static str {
        match self {
            ChangeReason::Slow => "slow",
            ChangeReason::Ineffective => "ineffective",
            ChangeReason::Probe => "probe",
            ChangeReason::ProbeFailed => "probe failed",
            ChangeReason::PacingReset => "pacing reset",
        }
    }
}

/// A rung change, for Stats for nerds and the browser checks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Change {
    /// Gameplay seconds the controller had measured.
    pub at: f64,
    pub to: Rung,
    pub reason: ChangeReason,
    /// The mean frame interval of the window that decided it, in milliseconds.
    pub mean_ms: f64,
    /// Its median GPU frame time, where the browser reports one.
    pub gpu_ms: Option<f64>,
}

/// Frame intervals and main-thread work summed over one window, and its GPU samples.
#[derive(Clone, Debug, Default)]
struct Window {
    interval_ms: f64,
    work_ms: f64,
    frames: u32,
    gpu_ms: Vec<f64>,
}

impl Window {
    fn clear(&mut self) {
        self.interval_ms = 0.0;
        self.work_ms = 0.0;
        self.frames = 0;
        self.gpu_ms.clear();
    }

    /// The median GPU sample, when there are enough to judge by.
    fn gpu_median(&mut self) -> Option<f64> {
        if self.gpu_ms.len() < MIN_GPU_SAMPLES {
            return None;
        }
        self.gpu_ms.sort_by(f64::total_cmp);
        Some(self.gpu_ms[self.gpu_ms.len() / 2])
    }
}

/// One judged window: its mean interval, main-thread work and median GPU time.
#[derive(Clone, Copy, Debug)]
struct Measure {
    mean_ms: f64,
    work_ms: f64,
    gpu_ms: Option<f64>,
}

impl Measure {
    /// What a step changes: the GPU's time where known, else the interval.
    fn cost_ms(&self) -> f64 {
        self.gpu_ms.unwrap_or(self.mean_ms)
    }
}

/// Steps down since frames were last not slow: where they began, at what scale and
/// cost.
#[derive(Clone, Copy, Debug)]
struct Descent {
    from: usize,
    scale: f32,
    cost_ms: f64,
}

/// A probe one rung up, on probation until `until`.
#[derive(Clone, Copy, Debug)]
struct Probe {
    from: usize,
    until: f64,
}

/// Picks the rung to draw at. Feed it every gameplay frame's interval; it returns
/// true when the rung changed.
#[derive(Clone, Debug)]
pub struct QualityController {
    setting: GraphicsSetting,
    start_tier: Tier,
    pixel_ratio: f64,
    rungs: Vec<Rung>,
    index: usize,
    /// Rungs from this index on lie below Low's floor (touch devices only).
    deep_from: usize,
    deep_floor: bool,
    /// Gameplay seconds measured (stalls and frames outside play excluded).
    clock: f64,
    /// No window is judged before this clock.
    settle_until: f64,
    window: Window,
    slow_windows: u32,
    comfortable_seconds: f64,
    descent: Option<Descent>,
    probe: Option<Probe>,
    /// Per rung: the clock before which Auto does not probe up to it, and the wait
    /// after its next failure.
    retry_at: Vec<f64>,
    backoff: Vec<f64>,
    /// No step down before this clock: the last descent bought nothing. The next hold
    /// is twice as long, up to the longest.
    hold_until: f64,
    hold: f64,
    log: Vec<Change>,
    changes: u32,
    /// Consecutive windows of paced frames with GPU room, and the pacing reset due.
    paced_windows: u32,
    pacing_reset: bool,
    pacing_retry_at: f64,
    pacing_wait: f64,
    pacing_resets: u32,
}

impl QualityController {
    pub fn new(setting: GraphicsSetting, start_tier: Tier, pixel_ratio: f64) -> Self {
        let mut controller = Self {
            setting,
            start_tier,
            pixel_ratio,
            rungs: Vec::new(),
            index: 0,
            deep_from: 0,
            deep_floor: false,
            clock: 0.0,
            settle_until: STARTUP_SECONDS,
            window: Window::default(),
            slow_windows: 0,
            comfortable_seconds: 0.0,
            descent: None,
            probe: None,
            retry_at: Vec::new(),
            backoff: Vec::new(),
            hold_until: 0.0,
            hold: FIRST_HOLD_SECONDS,
            log: Vec::new(),
            changes: 0,
            paced_windows: 0,
            pacing_reset: false,
            pacing_retry_at: 0.0,
            pacing_wait: FIRST_PACING_RETRY_SECONDS,
            pacing_resets: 0,
        };
        controller.rebuild(None);
        controller
    }

    /// Let Auto go below Low's floor while frames are very slow (touch tablets and
    /// phones), down to half a render pixel per CSS pixel.
    pub fn with_deep_floor(mut self) -> Self {
        self.deep_floor = true;
        let keep = self.rung();
        self.rebuild(Some((keep.tier, f64::from(keep.scale))));
        self
    }

    /// The rung to draw at.
    pub fn rung(&self) -> Rung {
        self.rungs[self.index]
    }

    pub fn setting(&self) -> GraphicsSetting {
        self.setting
    }

    /// Recent changes, oldest first, and how many there have been.
    pub fn log(&self) -> &[Change] {
        &self.log
    }

    pub fn changes(&self) -> u32 {
        self.changes
    }

    /// Pacing resets asked for so far.
    pub fn pacing_resets(&self) -> u32 {
        self.pacing_resets
    }

    /// Whether the canvas's pacing should be reset now (once per request).
    pub fn take_pacing_reset(&mut self) -> bool {
        std::mem::take(&mut self.pacing_reset)
    }

    /// Choose another setting; Auto restarts from the starting tier. Returns whether
    /// the rung changed.
    pub fn set_setting(&mut self, setting: GraphicsSetting) -> bool {
        if setting == self.setting {
            return false;
        }
        let before = self.rung();
        self.setting = setting;
        self.rebuild(None);
        self.restart();
        self.rung() != before
    }

    /// The canvas's drawing-buffer pixels per CSS pixel changed (another display):
    /// rebuild the ladder, keeping the tier and the nearest scale. Returns whether
    /// the rung changed.
    pub fn set_pixel_ratio(&mut self, pixel_ratio: f64) -> bool {
        if (pixel_ratio - self.pixel_ratio).abs() < 1e-6 {
            return false;
        }
        let before = self.rung();
        // The same render pixels per CSS pixel on the new canvas.
        let ratio = f64::from(before.scale) * self.pixel_ratio;
        self.pixel_ratio = pixel_ratio;
        self.rebuild(Some((before.tier, ratio / pixel_ratio)));
        self.rung() != before
    }

    /// A new round began: its first seconds are not judged.
    pub fn round_started(&mut self) {
        self.window.clear();
        self.slow_windows = 0;
        self.comfortable_seconds = 0.0;
        self.descent = None;
        self.settle_until = self.settle_until.max(self.clock + STARTUP_SECONDS);
    }

    /// Frames outside play (menus, pause, results): drop the partial window, and let
    /// the first frames back in play settle.
    pub fn interrupted(&mut self) {
        self.window.clear();
        self.settle_until = self.settle_until.max(self.clock + SETTLE_SECONDS);
    }

    /// One gameplay frame: `interval_ms` since the last one, the main thread's own
    /// work on it, and a GPU frame time when one arrived since the last call. Returns
    /// whether the rung changed.
    pub fn observe(&mut self, interval_ms: f64, work_ms: f64, gpu_ms: Option<f64>) -> bool {
        if !(interval_ms > 0.0 && interval_ms <= STALL_MS) {
            return false;
        }
        self.clock += interval_ms / 1000.0;
        if self.clock < self.settle_until {
            return false;
        }
        let window = &mut self.window;
        window.interval_ms += interval_ms;
        window.work_ms += work_ms.max(0.0);
        window.frames += 1;
        if let Some(gpu) = gpu_ms.filter(|gpu| *gpu >= 0.0 && *gpu <= STALL_MS)
            && window.gpu_ms.len() < MAX_GPU_SAMPLES
        {
            window.gpu_ms.push(gpu);
        }
        if window.interval_ms < WINDOW_SECONDS * 1000.0 {
            return false;
        }
        let frames = f64::from(window.frames);
        let measure = Measure {
            mean_ms: window.interval_ms / frames,
            work_ms: window.work_ms / frames,
            gpu_ms: window.gpu_median(),
        };
        self.window.clear();
        self.judge(measure)
    }

    /// How much more the GPU is expected to spend one rung up.
    fn step_up_cost(&self) -> f64 {
        let (rich, here) = (self.rungs[self.index - 1], self.rung());
        let pixels = f64::from(rich.scale / here.scale).powi(2);
        if rich.tier > here.tier {
            pixels * TIER_STEP_COST
        } else {
            pixels
        }
    }

    /// Ask for a pacing reset after windows of frames held slow while the GPU had room;
    /// returns whether one is due now.
    fn check_pacing(&mut self, measure: Measure) -> bool {
        let held = measure.gpu_ms.is_some_and(|gpu| {
            measure.mean_ms > TARGET_FRAME_MS * PACED_FACTOR
                && gpu <= TARGET_FRAME_MS * GPU_BUSY_SHARE
                && measure.work_ms < measure.mean_ms * CPU_BOUND_SHARE
        });
        if !held {
            self.paced_windows = 0;
            if measure.mean_ms <= TARGET_FRAME_MS * SLOW_FACTOR {
                self.pacing_wait = FIRST_PACING_RETRY_SECONDS;
            }
            return false;
        }
        self.paced_windows += 1;
        if self.paced_windows < PACED_WINDOWS || self.clock < self.pacing_retry_at {
            return false;
        }
        self.paced_windows = 0;
        self.pacing_reset = true;
        self.pacing_resets += 1;
        self.pacing_retry_at = self.clock + self.pacing_wait;
        self.pacing_wait = (self.pacing_wait * 2.0).min(MAX_PACING_RETRY_SECONDS);
        true
    }

    fn judge(&mut self, measure: Measure) -> bool {
        if self.check_pacing(measure) {
            // The rung stays: a change now would hide whether the reset helped.
            self.record(ChangeReason::PacingReset, measure);
            return false;
        }
        if self.rungs.len() < 2 {
            return false;
        }
        let mean = measure.mean_ms;
        let frames_slow = mean > TARGET_FRAME_MS * SLOW_FACTOR;
        // Slow frames call for fewer pixels only while the GPU is what makes them slow,
        // and a rung up only when the GPU has room for it.
        let (slow, very_slow, room, up_after, probation) = match measure.gpu_ms {
            Some(gpu) => (
                frames_slow && gpu > TARGET_FRAME_MS * GPU_BUSY_SHARE,
                gpu > TARGET_FRAME_MS * VERY_SLOW_FACTOR,
                self.index > 0 && gpu * self.step_up_cost() <= TARGET_FRAME_MS * GPU_ROOM_SHARE,
                UP_AFTER_GPU_SECONDS,
                GPU_PROBATION_SECONDS,
            ),
            None => (
                frames_slow,
                mean > TARGET_FRAME_MS * VERY_SLOW_FACTOR,
                mean <= TARGET_FRAME_MS * COMFORT_FACTOR,
                UP_AFTER_SECONDS,
                PROBATION_SECONDS,
            ),
        };
        if let Some(probe) = self.probe {
            if slow {
                let failed = self.index;
                self.probe = None;
                self.retry_at[failed] = self.clock + self.backoff[failed];
                self.backoff[failed] = (self.backoff[failed] * 2.0).min(MAX_BACKOFF_SECONDS);
                return self.change(probe.from, ChangeReason::ProbeFailed, measure);
            }
            if self.clock < probe.until {
                return false;
            }
            self.probe = None;
            self.backoff[self.index] = FIRST_BACKOFF_SECONDS;
        }
        if !slow {
            self.slow_windows = 0;
            self.descent = None;
            if !room {
                self.comfortable_seconds = 0.0;
                return false;
            }
            self.comfortable_seconds += WINDOW_SECONDS;
            if self.comfortable_seconds >= up_after
                && self.index > 0
                && self.clock >= self.retry_at[self.index - 1]
            {
                self.probe = Some(Probe {
                    from: self.index,
                    until: self.clock + SETTLE_SECONDS + probation,
                });
                return self.change(self.index - 1, ChangeReason::Probe, measure);
            }
            return false;
        }
        self.comfortable_seconds = 0.0;
        if let Some(descent) = self.descent {
            let pixels = (self.rung().scale / descent.scale).powi(2);
            if f64::from(pixels) <= DESCENT_JUDGED_AT_PIXELS
                && measure.cost_ms() > descent.cost_ms * (1.0 - MIN_DESCENT_GAIN)
            {
                self.descent = None;
                self.hold_until = self.clock + self.hold;
                self.hold = (self.hold * 2.0).min(MAX_HOLD_SECONDS);
                return self.change(descent.from, ChangeReason::Ineffective, measure);
            }
        }
        self.slow_windows += 1;
        let cpu_bound = measure.work_ms >= mean * CPU_BOUND_SHARE;
        // The frames the player sees, not the GPU's completion time, which a queue of
        // frames behind it stretches.
        let below_floor = self.index + 1 >= self.deep_from && mean <= DEEP_SLOW_MS;
        if (self.slow_windows < DOWN_AFTER_WINDOWS && !very_slow)
            || cpu_bound
            || below_floor
            || self.clock < self.hold_until
            || self.index + 1 == self.rungs.len()
        {
            return false;
        }
        if self.descent.is_none() {
            self.descent = Some(Descent {
                from: self.index,
                scale: self.rung().scale,
                cost_ms: measure.cost_ms(),
            });
        }
        self.change(self.index + 1, ChangeReason::Slow, measure)
    }

    fn change(&mut self, index: usize, reason: ChangeReason, measure: Measure) -> bool {
        self.index = index;
        self.slow_windows = 0;
        self.comfortable_seconds = 0.0;
        self.changes += 1;
        self.record(reason, measure);
        true
    }

    /// Log a change at the current rung and let the frames after it settle.
    fn record(&mut self, reason: ChangeReason, measure: Measure) {
        self.window.clear();
        self.settle_until = self.clock + SETTLE_SECONDS;
        if self.log.len() == LOG_CAPACITY {
            self.log.remove(0);
        }
        self.log.push(Change {
            at: self.clock,
            to: self.rung(),
            reason,
            mean_ms: measure.mean_ms,
            gpu_ms: measure.gpu_ms,
        });
    }

    /// Forget probes, holds and backoffs, as for a new page.
    fn restart(&mut self) {
        self.window.clear();
        self.slow_windows = 0;
        self.comfortable_seconds = 0.0;
        self.descent = None;
        self.probe = None;
        self.hold_until = 0.0;
        self.hold = FIRST_HOLD_SECONDS;
        self.settle_until = self.clock + STARTUP_SECONDS;
    }

    /// Build the ladder and stand on the rung of `keep`'s tier nearest its scale, or
    /// on the starting tier's richest rung.
    fn rebuild(&mut self, keep: Option<(Tier, f64)>) {
        self.rungs = ladder(self.setting, self.pixel_ratio);
        self.deep_from = self.rungs.len();
        if self.deep_floor && self.setting == GraphicsSetting::Auto {
            let last = f64::from(self.rungs[self.deep_from - 1].scale);
            let floor = DEEP_MIN_RATIO / self.pixel_ratio;
            descend(&mut self.rungs, Tier::Low, last, floor);
        }
        let tier = keep.map_or(self.start_tier, |(tier, _)| tier);
        let tier = match self.setting {
            GraphicsSetting::Fixed(fixed) => fixed,
            GraphicsSetting::Auto => tier,
        };
        let candidates = self
            .rungs
            .iter()
            .enumerate()
            .filter(|(_, rung)| rung.tier == tier);
        self.index = match keep {
            Some((_, scale)) => {
                let distance = |rung: &Rung| (f64::from(rung.scale) - scale).abs();
                candidates
                    .min_by(|a, b| distance(a.1).total_cmp(&distance(b.1)))
                    .map(|(index, _)| index)
            }
            None => candidates.map(|(index, _)| index).next(),
        }
        .unwrap_or(0);
        self.retry_at = vec![0.0; self.rungs.len()];
        self.backoff = vec![FIRST_BACKOFF_SECONDS; self.rungs.len()];
        self.descent = None;
        self.probe = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IPAD: f64 = 1.5;
    /// Gameplay seconds of frames at `interval_ms` with `work_ms` of main-thread
    /// work each; returns the changes they caused.
    fn run(controller: &mut QualityController, seconds: f64, interval_ms: f64) -> u32 {
        run_with_work(controller, seconds, interval_ms, 1.0)
    }

    fn run_with_work(
        controller: &mut QualityController,
        seconds: f64,
        interval_ms: f64,
        work_ms: f64,
    ) -> u32 {
        let frames = (seconds * 1000.0 / interval_ms).round() as u32;
        (0..frames)
            .filter(|_| controller.observe(interval_ms, work_ms, None))
            .count() as u32
    }

    /// A GPU-bound frame: its interval is the display's 60 Hz frame or the GPU time
    /// of `pixels` at `ms_per_megapixel`, plus a fixed cost, quantized to vsync.
    fn gpu_interval(rung: Rung, canvas_megapixels: f64, ms_per_megapixel: f64) -> f64 {
        let pixels = canvas_megapixels * f64::from(rung.scale).powi(2);
        let tier_cost = match rung.tier {
            Tier::High => 1.0,
            Tier::Medium => 0.85,
            Tier::Low => 0.7,
        };
        let gpu = pixels * ms_per_megapixel * tier_cost;
        (gpu / TARGET_FRAME_MS).ceil().max(1.0) * TARGET_FRAME_MS
    }

    fn simulate(
        controller: &mut QualityController,
        seconds: f64,
        megapixels: f64,
        ms_per_megapixel: f64,
    ) -> Vec<Rung> {
        let mut visited = vec![controller.rung()];
        let mut elapsed = 0.0;
        while elapsed < seconds {
            let interval = gpu_interval(controller.rung(), megapixels, ms_per_megapixel);
            if controller.observe(interval, 1.0, None) {
                visited.push(controller.rung());
            }
            elapsed += interval / 1000.0;
        }
        visited
    }

    #[test]
    fn the_ladder_steps_down_through_the_tiers_to_the_floor() {
        let rungs = ladder(GraphicsSetting::Auto, IPAD);
        assert_eq!(
            rungs[0],
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
        let last = *rungs.last().unwrap();
        assert_eq!(last.tier, Tier::Low);
        // 0.6 render pixels per CSS pixel on a 1.5 canvas.
        assert!((f64::from(last.scale) * IPAD - 0.6).abs() < 1e-6);
        for pair in rungs.windows(2) {
            let (rich, cheap) = (pair[0], pair[1]);
            // One thing changes per rung: the tier at the same scale, or a small
            // scale step within a tier.
            if rich.tier == cheap.tier {
                let step = cheap.scale / rich.scale;
                assert!((0.89..0.98).contains(&step), "{rich:?} -> {cheap:?}");
            } else {
                assert!(cheap.tier < rich.tier);
                assert_eq!(cheap.scale, rich.scale);
            }
        }
        // High keeps at least one render pixel per CSS pixel, Medium 0.8.
        for rung in &rungs {
            let ratio = f64::from(rung.scale) * IPAD;
            match rung.tier {
                Tier::High => assert!(ratio >= 1.0 - 1e-6),
                Tier::Medium => assert!(ratio >= 0.8 - 1e-6),
                Tier::Low => assert!(ratio >= 0.6 - 1e-6),
            }
        }
    }

    #[test]
    fn a_one_to_one_canvas_starts_dropping_features_before_pixels() {
        let rungs = ladder(GraphicsSetting::Auto, 1.0);
        assert_eq!(
            rungs[0],
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
        assert_eq!(
            rungs[1],
            Rung {
                tier: Tier::Medium,
                scale: 1.0
            }
        );
        assert!(rungs.iter().all(|rung| rung.scale >= 0.6 - 1e-6));
    }

    #[test]
    fn fixed_settings_draw_their_tier_at_full_scale_and_never_adapt() {
        let mut controller =
            QualityController::new(GraphicsSetting::Fixed(Tier::Medium), Tier::High, IPAD);
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::Medium,
                scale: 1.0
            }
        );
        assert_eq!(run(&mut controller, 60.0, 100.0), 0);
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::Medium,
                scale: 1.0
            }
        );
    }

    #[test]
    fn devices_start_at_their_tier() {
        // WebGPU: the full picture on desktops, Medium on touch devices.
        assert_eq!(
            starting_tier(DeviceClass::Desktop, 16384, false),
            Tier::High
        );
        for device in [DeviceClass::Tablet, DeviceClass::Phone] {
            assert_eq!(starting_tier(device, 16384, false), Tier::Medium);
        }
        // The WebGL fallback: an older browser or device.
        assert_eq!(
            starting_tier(DeviceClass::Desktop, 16384, true),
            Tier::Medium
        );
        assert_eq!(starting_tier(DeviceClass::Tablet, 16384, true), Tier::Low);
        assert_eq!(starting_tier(DeviceClass::Desktop, 4096, false), Tier::Low);
        let controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
        // Medium starts on its richest rung: one render pixel per CSS pixel.
        let controller = QualityController::new(GraphicsSetting::Auto, Tier::Medium, IPAD);
        let rung = controller.rung();
        assert_eq!(rung.tier, Tier::Medium);
        assert!((f64::from(rung.scale) * IPAD - 1.0).abs() < 0.01);
    }

    #[test]
    fn nothing_changes_in_the_first_seconds_of_a_round() {
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        assert_eq!(run(&mut controller, STARTUP_SECONDS - 0.2, 100.0), 0);
        controller.round_started();
        assert_eq!(run(&mut controller, STARTUP_SECONDS - 0.2, 100.0), 0);
        assert_eq!(controller.rung().scale, 1.0);
    }

    #[test]
    fn steady_frames_at_the_display_rate_never_change_the_picture() {
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        assert_eq!(run(&mut controller, 300.0, TARGET_FRAME_MS), 0);
        // A 120 Hz display too.
        assert_eq!(run(&mut controller, 60.0, TARGET_FRAME_MS / 2.0), 0);
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
    }

    #[test]
    fn a_brief_hitch_or_stall_changes_nothing() {
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        run(&mut controller, 10.0, TARGET_FRAME_MS);
        // One slow second (an explosion's first puffs), then smooth again.
        assert_eq!(run(&mut controller, 1.0, 33.3), 0);
        assert_eq!(run(&mut controller, 5.0, TARGET_FRAME_MS), 0);
        // A tab switch or a long task.
        assert!(!controller.observe(4000.0, 1.0, None));
        assert!(!controller.observe(600.0, 1.0, None));
        assert_eq!(run(&mut controller, 5.0, TARGET_FRAME_MS), 0);
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
    }

    #[test]
    fn sustained_slow_frames_step_down_one_rung_at_a_time() {
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        run(&mut controller, STARTUP_SECONDS, TARGET_FRAME_MS);
        // Two slow seconds: one step, then a settling second before the next.
        assert_eq!(run(&mut controller, 2.05, 33.3), 1);
        assert_eq!(controller.rung().scale, 0.9);
        let log = controller.log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].reason, ChangeReason::Slow);
        // Slow frames keep stepping down, no faster than one rung per settling
        // second and two slow windows.
        let changes = run(&mut controller, 9.0, 25.0);
        assert!((2..=3).contains(&changes), "{changes} changes in 9 s");
        assert!(
            controller
                .log()
                .iter()
                .all(|change| change.reason == ChangeReason::Slow)
        );
    }

    #[test]
    fn main_thread_bound_frames_keep_their_pixels() {
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        run(&mut controller, STARTUP_SECONDS, TARGET_FRAME_MS);
        assert_eq!(run_with_work(&mut controller, 20.0, 33.3, 30.0), 0);
        assert_eq!(controller.rung().scale, 1.0);
    }

    #[test]
    fn a_descent_that_buys_nothing_is_undone_and_held() {
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        run(&mut controller, STARTUP_SECONDS, TARGET_FRAME_MS);
        // Frames slow for a reason pixels do not explain: 70 ms whatever the scale.
        // Auto steps down (a rung every two seconds, so slow are they) until the
        // pixels have halved, sees no gain and goes back.
        assert_eq!(run(&mut controller, 14.0, 70.0), 5);
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
        assert_eq!(
            controller.log().last().unwrap().reason,
            ChangeReason::Ineffective
        );
        // It holds the full picture for a minute before trying again, then twice as
        // long: one descent in the next two minutes.
        assert_eq!(run(&mut controller, FIRST_HOLD_SECONDS - 6.0, 70.0), 0);
        assert_eq!(run(&mut controller, 120.0, 70.0), 5);
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
    }

    #[test]
    fn a_descent_on_paced_frames_is_judged_only_once_the_pixels_halve() {
        // WebKit paces a canvas that misses the display frame at 30 fps: three steps
        // of a pixel-bound 30 ms frame all show 33.3 ms, and the fourth reaches 60.
        let megapixels = 2.18;
        let cost = 30.0 / megapixels;
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        let visited = simulate(&mut controller, 20.0, megapixels, cost);
        assert!(
            controller
                .log()
                .iter()
                .all(|change| change.reason == ChangeReason::Slow)
        );
        assert_eq!(visited.last().unwrap().scale, 0.729);
    }

    #[test]
    fn a_gpu_bound_tablet_settles_and_holds_without_pumping() {
        // An iPad canvas (2.2 Mpx at a 1.5 ratio) whose full picture takes 30 ms.
        let megapixels = 2.18;
        let cost = 30.0 / megapixels;
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        let visited = simulate(&mut controller, 60.0, megapixels, cost);
        let settled = controller.rung();
        assert!(gpu_interval(settled, megapixels, cost) <= TARGET_FRAME_MS + 1e-9);
        // It got there one rung at a time, early, and stayed: the probes up from
        // there fail rarely, with growing waits between them.
        assert!(visited.len() <= 8, "{visited:?}");
        let log_before = controller.changes();
        simulate(&mut controller, 300.0, megapixels, cost);
        let probes = controller.changes() - log_before;
        assert!(probes <= 8, "{probes} changes in five minutes");
        let rung = controller.rung();
        assert!(gpu_interval(rung, megapixels, cost) <= TARGET_FRAME_MS + 1e-9);
    }

    #[test]
    fn a_device_that_got_faster_climbs_back_to_the_full_picture() {
        // A tablet starts on Medium; its frames keep up (a fixed browser), so it
        // probes up rung by rung to High at full scale.
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::Medium, IPAD);
        let visited = simulate(&mut controller, 120.0, 2.18, 1.0);
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
        assert!(
            visited
                .windows(2)
                .all(|pair| { pair[1].tier >= pair[0].tier && pair[1].scale >= pair[0].scale })
        );
    }

    #[test]
    fn a_low_floor_is_never_permanent() {
        // Frames far too slow at any scale push Auto to the floor...
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        let slow_at_any_scale = |rung: Rung| 40.0 + 40.0 * f64::from(rung.scale).powi(2);
        let mut elapsed = 0.0;
        while elapsed < 90.0 {
            let interval = slow_at_any_scale(controller.rung());
            controller.observe(interval, 1.0, None);
            elapsed += interval / 1000.0;
        }
        assert_eq!(controller.rung().tier, Tier::Low);
        // ...but once the frames become cheap (the browser's bug fixed mid-session),
        // it climbs back to the top.
        simulate(&mut controller, 240.0, 2.18, 1.0);
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
    }

    #[test]
    fn settings_and_pixel_ratio_changes_rebuild_the_ladder() {
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        assert!(controller.set_setting(GraphicsSetting::Fixed(Tier::Low)));
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::Low,
                scale: 1.0
            }
        );
        assert!(controller.set_setting(GraphicsSetting::Auto));
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
        run(&mut controller, STARTUP_SECONDS, TARGET_FRAME_MS);
        run(&mut controller, 2.05, 33.3);
        assert_eq!(controller.rung().scale, 0.9);
        // The same render pixels per CSS pixel on a 1:1 display: the tier's nearest
        // scale there.
        assert!(controller.set_pixel_ratio(1.0));
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
        assert!(!controller.set_pixel_ratio(1.0));
        assert_eq!(
            GraphicsSetting::parse("medium"),
            GraphicsSetting::Fixed(Tier::Medium)
        );
        assert_eq!(GraphicsSetting::parse("nonsense"), GraphicsSetting::Auto);
        assert_eq!(GraphicsSetting::Fixed(Tier::Low).name(), "low");
    }

    /// Frames whose GPU takes `gpu(rung)` milliseconds, shown at `interval(gpu)`,
    /// with each frame's GPU time sampled; returns the rungs visited.
    fn simulate_gpu(
        controller: &mut QualityController,
        seconds: f64,
        gpu: impl Fn(Rung) -> f64,
        interval: impl Fn(f64) -> f64,
    ) -> Vec<Rung> {
        let mut visited = vec![controller.rung()];
        let mut elapsed = 0.0;
        while elapsed < seconds {
            let gpu_ms = gpu(controller.rung());
            let interval_ms = interval(gpu_ms);
            if controller.observe(interval_ms, 1.0, Some(gpu_ms)) {
                visited.push(controller.rung());
            }
            elapsed += interval_ms / 1000.0;
        }
        visited
    }

    /// The GPU time of an iPad-sized frame whose full picture costs `full` ms.
    fn tablet_gpu(full: f64) -> impl Fn(Rung) -> f64 {
        move |rung: Rung| {
            let tier = match rung.tier {
                Tier::High => 1.0,
                Tier::Medium => 0.85,
                Tier::Low => 0.7,
            };
            full * f64::from(rung.scale).powi(2) * tier
        }
    }

    /// Vsync at 60 Hz: the GPU time rounded up to whole display frames.
    fn vsync(gpu: f64) -> f64 {
        (gpu / TARGET_FRAME_MS).ceil().max(1.0) * TARGET_FRAME_MS
    }

    #[test]
    fn a_stuck_paced_canvas_with_an_idle_gpu_keeps_the_full_picture() {
        // WebKit stuck at 24 fps after a hitch, though the GPU now needs 6 ms.
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        let visited = simulate_gpu(&mut controller, 120.0, |_| 6.0, |_| 41.7);
        assert_eq!(
            visited,
            vec![Rung {
                tier: Tier::High,
                scale: 1.0
            }]
        );
    }

    #[test]
    fn with_gpu_time_a_gpu_bound_tablet_steps_down_until_the_gpu_fits_and_stays() {
        // WebKit's pacing: a frame whose GPU work overruns the display frame drops the
        // canvas to the paced rate for good, until the canvas's pacing is reset.
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        let gpu = tablet_gpu(30.0);
        let mut paced_ms: f64 = 0.0;
        let mut visited = vec![controller.rung()];
        let mut elapsed = 0.0;
        while elapsed < 360.0 {
            let gpu_ms = gpu(controller.rung());
            paced_ms = paced_ms.max(vsync(gpu_ms));
            if controller.observe(paced_ms, 1.0, Some(gpu_ms)) {
                visited.push(controller.rung());
            }
            if controller.take_pacing_reset() {
                paced_ms = 0.0;
            }
            elapsed += paced_ms / 1000.0;
            if elapsed > 60.0 && visited.len() > 1 {
                // Settled by now: the GPU fits, a reset restored the display rate,
                // and the next rung up, which would not fit, is never probed.
                let settled = controller.rung();
                assert!(
                    gpu(settled) <= TARGET_FRAME_MS * GPU_BUSY_SHARE,
                    "{visited:?}"
                );
                assert_eq!(paced_ms, TARGET_FRAME_MS);
            }
        }
        assert!(
            controller
                .log()
                .iter()
                .all(|change| change.reason != ChangeReason::Probe)
        );
        assert_eq!(controller.pacing_resets(), 1);
        assert_eq!(visited.len(), 5, "{visited:?}");
    }

    #[test]
    fn with_gpu_time_a_cheap_frame_climbs_to_the_full_picture_within_a_minute() {
        // A device that started lowest (or fell there) whose GPU now has plenty of room.
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::Low, IPAD);
        let visited = simulate_gpu(&mut controller, 60.0, tablet_gpu(5.0), vsync);
        assert_eq!(
            controller.rung(),
            Rung {
                tier: Tier::High,
                scale: 1.0
            }
        );
        assert!(
            controller
                .log()
                .iter()
                .all(|change| change.reason == ChangeReason::Probe)
        );
        assert!(visited.len() >= 3);
    }

    #[test]
    fn with_gpu_time_a_rung_that_would_not_fit_is_not_probed() {
        // 12 ms at Medium's top rung: High at the same scale would cost about 15 ms.
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::Medium, IPAD);
        let visited = simulate_gpu(&mut controller, 120.0, |_| 12.0, vsync);
        assert_eq!(visited.len(), 1, "{visited:?}");
    }

    #[test]
    fn frames_held_slow_while_the_gpu_has_room_ask_for_a_pacing_reset() {
        // WebKit stuck at 15 fps after a hitch while the GPU needs 10 ms, under any
        // setting: a reset is asked for after two such windows, then with doubling
        // waits while the frames stay held.
        for setting in [GraphicsSetting::Auto, GraphicsSetting::Fixed(Tier::High)] {
            let mut controller = QualityController::new(setting, Tier::High, IPAD);
            let mut resets = 0;
            let mut elapsed = 0.0;
            while elapsed < 120.0 {
                controller.observe(66.7, 1.0, Some(10.0));
                resets += u32::from(controller.take_pacing_reset());
                elapsed += 0.0667;
            }
            assert!((4..=6).contains(&resets), "{resets} resets in two minutes");
            assert_eq!(controller.pacing_resets(), resets);
            assert_eq!(controller.changes(), 0);
            assert_eq!(controller.rung().scale, 1.0);
        }
        // Once a reset brings the display rate back, nothing more is asked.
        let mut controller = QualityController::new(GraphicsSetting::Auto, Tier::High, IPAD);
        let mut resets = 0;
        let mut held = true;
        let mut elapsed = 0.0;
        while elapsed < 60.0 {
            let interval = if held { 66.7 } else { TARGET_FRAME_MS };
            controller.observe(interval, 1.0, Some(10.0));
            if controller.take_pacing_reset() {
                resets += 1;
                held = false;
            }
            elapsed += interval / 1000.0;
        }
        assert_eq!(resets, 1);
        // A GPU without room is a cost, not pacing: no reset.
        let mut controller =
            QualityController::new(GraphicsSetting::Fixed(Tier::High), Tier::High, IPAD);
        let mut elapsed = 0.0;
        while elapsed < 60.0 {
            controller.observe(66.7, 1.0, Some(40.0));
            assert!(!controller.take_pacing_reset());
            elapsed += 0.0667;
        }
    }

    #[test]
    fn a_touch_device_goes_below_the_floor_only_while_frames_take_over_50_ms() {
        let floor_ratio =
            |controller: &QualityController| f64::from(controller.rung().scale) * IPAD;
        // 40 ms frames whatever the scale: down to Low's floor of 0.6, no further.
        let mut controller =
            QualityController::new(GraphicsSetting::Auto, Tier::Medium, IPAD).with_deep_floor();
        let start = controller.rung();
        assert_eq!(start.tier, Tier::Medium);
        assert!((f64::from(start.scale) * IPAD - 1.0).abs() < 0.01);
        let mut elapsed = 0.0;
        while elapsed < 40.0 {
            let interval = 20.0 + 25.0 * f64::from(controller.rung().scale).powi(2);
            controller.observe(interval, 1.0, None);
            elapsed += interval / 1000.0;
        }
        assert!(
            (floor_ratio(&controller) - 0.6).abs() < 0.01,
            "{:?}",
            controller.rung()
        );
        // Frames over 50 ms go on down to half a render pixel per CSS pixel.
        let mut controller =
            QualityController::new(GraphicsSetting::Auto, Tier::Medium, IPAD).with_deep_floor();
        let mut elapsed = 0.0;
        while elapsed < 60.0 {
            let interval = 45.0 + 60.0 * f64::from(controller.rung().scale).powi(2);
            controller.observe(interval, 1.0, None);
            elapsed += interval / 1000.0;
        }
        assert!(
            (floor_ratio(&controller) - 0.5).abs() < 0.01,
            "{:?}",
            controller.rung()
        );
        // A GPU whose completion time a queue of frames stretches past 50 ms while the
        // frames themselves take 25 ms stays at the floor.
        let mut controller =
            QualityController::new(GraphicsSetting::Auto, Tier::Medium, IPAD).with_deep_floor();
        let mut elapsed = 0.0;
        while elapsed < 60.0 {
            let gpu = 50.0 + 100.0 * f64::from(controller.rung().scale).powi(2);
            controller.observe(25.0, 1.0, Some(gpu));
            elapsed += 0.025;
        }
        assert!(
            (floor_ratio(&controller) - 0.6).abs() < 0.01,
            "{:?}",
            controller.rung()
        );
        // A desktop never goes below 0.6.
        assert!(
            ladder(GraphicsSetting::Auto, IPAD)
                .iter()
                .all(|rung| f64::from(rung.scale) * IPAD >= 0.6 - 1e-6)
        );
    }
}
