//! `--frame-log`: once a second, the frame rate and where a frame's time went, on
//! stderr. Evidence for frame-rate work, not a player feature.

use std::time::Instant;

/// One second's frames.
pub struct FrameLog {
    start: Instant,
    last: Option<Instant>,
    frames: u32,
    longest_ms: f64,
    engine_ms: f64,
    overlay_ms: f64,
    present_ms: f64,
    sim_ms: f64,
    render_ms: f64,
}

/// A frame's parts, in milliseconds.
#[derive(Default)]
pub struct FrameParts {
    /// The engine's `frame` call (simulation, presentation, drawing and waiting for a
    /// window texture).
    pub engine: f64,
    /// egui's layout and tessellation.
    pub overlay: f64,
    /// Drawing the overlay and showing the frame.
    pub present: f64,
    /// The engine's own simulation and render times.
    pub sim: f64,
    pub render: f64,
}

impl FrameLog {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            last: None,
            frames: 0,
            longest_ms: 0.0,
            engine_ms: 0.0,
            overlay_ms: 0.0,
            present_ms: 0.0,
            sim_ms: 0.0,
            render_ms: 0.0,
        }
    }

    /// Count a frame that ended now; `gpu_ms` is the engine's smoothed GPU frame time,
    /// which the line closing the second reports.
    pub fn frame(
        &mut self,
        parts: &FrameParts,
        size: (u32, u32),
        scene: (u32, u32),
        gpu_ms: Option<f64>,
    ) {
        let now = Instant::now();
        if let Some(last) = self.last {
            self.longest_ms = self.longest_ms.max((now - last).as_secs_f64() * 1000.0);
        }
        self.last = Some(now);
        self.frames += 1;
        self.engine_ms += parts.engine;
        self.overlay_ms += parts.overlay;
        self.present_ms += parts.present;
        self.sim_ms += parts.sim;
        self.render_ms += parts.render;
        let elapsed = (now - self.start).as_secs_f64();
        if elapsed >= 1.0 {
            let frames = f64::from(self.frames);
            eprintln!(
                "fps {:.1} longest {:.1} ms | engine {:.2} (sim {:.2} render {:.2}) overlay {:.2} present {:.2} gpu {} ms | canvas {}x{} scene {}x{}",
                frames / elapsed,
                self.longest_ms,
                self.engine_ms / frames,
                self.sim_ms / frames,
                self.render_ms / frames,
                self.overlay_ms / frames,
                self.present_ms / frames,
                gpu_ms.map_or_else(|| "-".to_owned(), |ms| format!("{ms:.2}")),
                size.0,
                size.1,
                scene.0,
                scene.1,
            );
            *self = Self {
                last: self.last,
                ..Self::new()
            };
        }
    }
}
