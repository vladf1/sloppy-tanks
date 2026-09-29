//! The host's fixed-step clock (`src/net/fixed-step-clock.ts`).

/// The host timer's interval: snapshots go out at 20 Hz.
pub const HOST_INTERVAL_MS: f64 = 50.0;
pub const SIMULATION_STEP_MS: f64 = 1000.0 / 60.0;
/// Most ticks one timer callback may run to catch up.
pub const MAX_STEPS_PER_BATCH: u32 = 6;
/// Owed time beyond this means the host cannot keep up: the room ends instead of
/// silently skipping physics.
pub const MAX_TICK_DEBT_MS: f64 = 250.0;
const CLOCK_EPSILON_MS: f64 = 1e-7;

/// Keeps elapsed time owed to physics; a stall never silently drops simulation ticks.
#[derive(Clone, Debug)]
pub struct FixedStepClock {
    pub tick: u64,
    pub debt_ms: f64,
    last_ms: f64,
}

impl FixedStepClock {
    pub fn new(now_ms: f64) -> Self {
        Self {
            tick: 0,
            debt_ms: 0.0,
            last_ms: now_ms,
        }
    }

    /// Runs the ticks now due (at most [`MAX_STEPS_PER_BATCH`]), passing each new tick
    /// number to `step`. Returns false, running nothing, when the debt exceeds
    /// [`MAX_TICK_DEBT_MS`]. A clock that goes backwards adds no time.
    pub fn advance(&mut self, now_ms: f64, mut step: impl FnMut(u64)) -> Result<bool, String> {
        if !now_ms.is_finite() {
            return Err("Invalid host clock".into());
        }
        self.debt_ms += (now_ms - self.last_ms).max(0.0);
        self.last_ms = self.last_ms.max(now_ms);
        if self.debt_ms > MAX_TICK_DEBT_MS + CLOCK_EPSILON_MS {
            return Ok(false);
        }
        let mut steps = 0;
        while self.debt_ms + CLOCK_EPSILON_MS >= SIMULATION_STEP_MS && steps < MAX_STEPS_PER_BATCH {
            self.tick += 1;
            step(self.tick);
            self.debt_ms = (self.debt_ms - SIMULATION_STEP_MS).max(0.0);
            steps += 1;
        }
        Ok(true)
    }
}
