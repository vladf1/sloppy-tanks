//! Fixed-window request counters keyed by client address.

use std::collections::HashMap;

/// Most keys (client IPs) tracked at once, so hostile IP churn cannot grow memory unbounded.
pub const MAX_RATE_LIMIT_KEYS: usize = 10_000;
const MINUTE_MS: u64 = 60_000;

#[derive(Clone, Copy)]
struct Window {
    start_ms: u64,
    count: u32,
}

/// At most `limit` calls per key in each `period_ms` window. When every tracked key is
/// still inside its window, new keys are refused until one expires: under a flood of more
/// distinct IPs than the cap, failing closed beats unbounded memory.
pub struct RateLimit {
    limit: u32,
    period_ms: u64,
    max_keys: usize,
    windows: HashMap<String, Window>,
    /// Start of the oldest window left after the last sweep; nothing expires before then.
    oldest_start_ms: u64,
}

impl RateLimit {
    /// `limit` calls per key and minute, tracking up to [`MAX_RATE_LIMIT_KEYS`] keys.
    pub fn per_minute(limit: u32) -> Self {
        Self::new(limit, MINUTE_MS, MAX_RATE_LIMIT_KEYS)
    }

    pub fn new(limit: u32, period_ms: u64, max_keys: usize) -> Self {
        Self {
            limit,
            period_ms,
            max_keys,
            windows: HashMap::new(),
            oldest_start_ms: u64::MAX,
        }
    }

    pub fn allow(&mut self, key: &str, now_ms: u64) -> bool {
        let window = match self.windows.get_mut(key) {
            Some(window) => {
                if now_ms.saturating_sub(window.start_ms) >= self.period_ms {
                    *window = Window {
                        start_ms: now_ms,
                        count: 0,
                    };
                }
                window
            }
            None => {
                if self.windows.len() >= self.max_keys && !self.sweep(now_ms) {
                    return false;
                }
                self.oldest_start_ms = self.oldest_start_ms.min(now_ms);
                self.windows.entry(key.to_owned()).or_insert(Window {
                    start_ms: now_ms,
                    count: 0,
                })
            }
        };
        window.count = window.count.saturating_add(1);
        window.count <= self.limit
    }

    /// Drops expired windows; true when that made room. Scans at most once per expiry.
    fn sweep(&mut self, now_ms: u64) -> bool {
        if now_ms.saturating_sub(self.oldest_start_ms) < self.period_ms {
            return false;
        }
        let period_ms = self.period_ms;
        self.windows
            .retain(|_, window| now_ms.saturating_sub(window.start_ms) < period_ms);
        self.oldest_start_ms = self
            .windows
            .values()
            .map(|window| window.start_ms)
            .min()
            .unwrap_or(u64::MAX);
        self.windows.len() < self.max_keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_a_fixed_number_of_calls_per_key_and_window() {
        let mut limit = RateLimit::new(2, 1000, MAX_RATE_LIMIT_KEYS);
        assert!(limit.allow("a", 0));
        assert!(limit.allow("a", 10));
        assert!(!limit.allow("a", 20));
        assert!(limit.allow("b", 20));
        assert!(limit.allow("a", 1000));
    }

    #[test]
    fn refuses_new_keys_while_every_window_is_live_then_frees_expired_ones() {
        let mut limit = RateLimit::new(5, 1000, 3);
        for key in ["a", "b", "c"] {
            assert!(limit.allow(key, 0));
        }
        // A flood of fresh addresses cannot grow the map past its cap.
        for index in 0..100 {
            assert!(!limit.allow(&format!("new{index}"), 500));
        }
        assert!(limit.allow("a", 500), "tracked keys keep their budget");
        assert!(limit.allow("d", 1000), "expired windows make room");
        assert!(limit.allow("e", 1000));
        assert!(limit.allow("f", 1000));
        assert!(!limit.allow("g", 1000));
    }
}
