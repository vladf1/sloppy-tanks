//! The stages of round preparation, kept apart from the renderer so their order is
//! testable natively.
//!
//! Creating a pipeline returns at once in the browser: the GPU process compiles it when
//! it reaches the command, which takes seconds for shaders the system's shader cache has
//! not seen (about 4.5 s for an arena on an Apple GPU). Meanwhile the page gets no
//! animation frames and its timers stop, so a room page can neither read snapshots nor
//! acknowledge them, and the room drops a client that stops acknowledging for three
//! seconds. The arena therefore counts as prepared only once the GPU has run the
//! warm-up that uses every pipeline; a room page stays suspended until then.

/// Where preparation stands for the arena being built.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preparation {
    /// Creating pipelines and waiting for textures.
    #[default]
    Compiling,
    /// The warm-up that draws every variant was submitted; waiting for the GPU to run it.
    AwaitingGpu,
    /// The GPU compiled and ran the warm-up; the first frames can draw.
    Ready,
}

impl Preparation {
    /// Reports whether to warm up now: true once, when nothing remains to compile and
    /// every texture has loaded. The caller then submits the warm-up and asks the GPU to
    /// report when it has run everything submitted.
    pub fn warm_up_due(&mut self, remaining: u32, textures_pending: u32) -> bool {
        let due = *self == Preparation::Compiling && remaining == 0 && textures_pending == 0;
        if due {
            *self = Preparation::AwaitingGpu;
        }
        due
    }

    /// Records whether the GPU has run the submitted warm-up.
    pub fn gpu_finished(&mut self, idle: bool) {
        if *self == Preparation::AwaitingGpu && idle {
            *self = Preparation::Ready;
        }
    }

    pub fn awaiting_gpu(self) -> bool {
        self == Preparation::AwaitingGpu
    }

    pub fn ready(self) -> bool {
        self == Preparation::Ready
    }
}

#[cfg(test)]
mod tests {
    use super::Preparation;

    #[test]
    fn warms_up_once_after_pipelines_and_textures() {
        let mut preparation = Preparation::default();
        assert!(!preparation.warm_up_due(3, 0), "pipelines left");
        assert!(!preparation.warm_up_due(0, 2), "textures still loading");
        assert!(preparation.warm_up_due(0, 0));
        assert!(!preparation.warm_up_due(0, 0), "one warm-up per arena");
        assert!(preparation.awaiting_gpu());
    }

    #[test]
    fn is_not_ready_until_the_gpu_ran_the_warm_up() {
        let mut preparation = Preparation::default();
        preparation.gpu_finished(true);
        assert!(
            !preparation.ready(),
            "an idle GPU before the warm-up proves nothing"
        );
        preparation.warm_up_due(0, 0);
        for _ in 0..3 {
            preparation.gpu_finished(false);
            assert!(!preparation.ready(), "the GPU is still compiling");
        }
        preparation.gpu_finished(true);
        assert!(preparation.ready());
        assert!(!preparation.warm_up_due(0, 0));
    }
}
