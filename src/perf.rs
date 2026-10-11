use std::time::Instant;

use serde_json::{Value, json};

use crate::audio::ClockStats;

const RECENT: usize = 240;
const KEEP: usize = 60_000;

#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub avg_ms: f32,
    pub p99_ms: f32,
    pub p50_ms: f32,
    pub delay_ms: f32,
    pub swap_ms: f32,
}

pub struct Perf {
    last: Instant,
    recent: Vec<f32>,
    swap_recent: Vec<f32>,
    swaps: Vec<f32>,
    delays: Vec<f32>,
    awaits: Vec<f32>,
    frames: Vec<f32>,
    cpu: Vec<f32>,
    updates: Vec<f32>,
    cached: Stats,
    since_cache: u32,
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    sorted
        .get(((sorted.len() as f32 - 1.0) * p).round() as usize)
        .copied()
        .unwrap_or(0.0)
}

fn summary(values: &[f32]) -> Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    json!({
        "avg": sorted.iter().sum::<f32>() / sorted.len().max(1) as f32,
        "p50": percentile(&sorted, 0.5),
        "p99": percentile(&sorted, 0.99),
        "p999": percentile(&sorted, 0.999),
        "max": sorted.last().copied().unwrap_or(0.0),
    })
}

impl Perf {
    pub fn new() -> Self {
        Self {
            last: Instant::now(),
            recent: Vec::with_capacity(RECENT),
            swap_recent: Vec::new(),
            swaps: Vec::new(),
            delays: Vec::new(),
            awaits: Vec::new(),
            frames: Vec::new(),
            cpu: Vec::new(),
            updates: Vec::new(),
            cached: Stats::default(),
            since_cache: 0,
        }
    }

    pub fn tick(&mut self) -> f32 {
        let ms = self.last.elapsed().as_secs_f32() * 1000.0;
        self.last = Instant::now();
        if self.recent.len() == RECENT {
            self.recent.remove(0);
        }
        self.recent.push(ms);
        if self.frames.len() < KEEP {
            self.frames.push(ms);
        }
        self.since_cache += 1;
        if self.since_cache >= 15 {
            self.since_cache = 0;
            let mut sorted = self.recent.clone();
            sorted.sort_by(f32::total_cmp);
            let avg = sorted.iter().sum::<f32>() / sorted.len() as f32;
            self.cached = Stats {
                avg_ms: avg,
                p99_ms: percentile(&sorted, 0.99),
                p50_ms: percentile(&sorted, 0.5),
                delay_ms: self.cached.delay_ms,
                swap_ms: self.swap_recent.iter().sum::<f32>()
                    / self.swap_recent.len().max(1) as f32,
            };
        }
        ms
    }

    pub fn swap(&mut self, wait_ms: f32, delay_ms: f32) {
        let ms = (wait_ms - delay_ms).max(0.0);
        if self.awaits.len() < KEEP {
            self.awaits.push(wait_ms);
            self.delays.push(delay_ms);
        }
        if self.swap_recent.len() == RECENT {
            self.swap_recent.remove(0);
        }
        self.swap_recent.push(ms);
        if self.swaps.len() < KEEP {
            self.swaps.push(ms);
        }
    }

    pub fn cpu(&mut self, started: Instant) {
        if self.cpu.len() < KEEP {
            self.cpu.push(started.elapsed().as_secs_f32() * 1000.0);
        }
    }

    pub fn update(&mut self, ms: f32) {
        if self.updates.len() < KEEP {
            self.updates.push(ms);
        }
    }

    pub fn stats(&self) -> Stats {
        self.cached
    }

    pub fn reset(&mut self) {
        self.frames.clear();
        self.cpu.clear();
        self.updates.clear();
        self.swaps.clear();
        self.awaits.clear();
        self.delays.clear();
    }

    pub fn json(&self, clock: ClockStats) -> Value {
        let mut sorted = self.frames.clone();
        sorted.sort_by(f32::total_cmp);
        let median = percentile(&sorted, 0.5);
        json!({
            "frames": self.frames.len(),
            "frame_ms": summary(&self.frames),
            "cpu_ms": summary(&self.cpu),
            "update_ms": summary(&self.updates),
            "swap_ms": summary(&self.swaps),
            "slow_frames": self.frames.iter().filter(|f| **f > median * 1.5).count(),
            "pacer_delay_ms": summary(&self.delays),
            "pacer_zero_pct": 100.0 * self.delays.iter().filter(|d| **d < 0.05).count() as f32
                / self.delays.len().max(1) as f32,
            "audio_period_ms": clock.period_ms,
            "clock_jitter_ms": clock.jitter_ms,
        })
    }

    pub fn report(&self, clock: ClockStats) -> String {
        let mut frames = self.frames.clone();
        frames.sort_by(f32::total_cmp);
        let mut swaps = self.swaps.clone();
        swaps.sort_by(f32::total_cmp);
        let avg = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
        let max = |v: &[f32]| v.iter().copied().fold(0.0, f32::max);
        format!(
            "frames {} | await ms avg {:.2} delay avg {:.2} swap block ms avg {:.2} p99 {:.2} | frame ms avg {:.2} p50 {:.2} p99 {:.2} max {:.2} | cpu ms avg {:.2} max {:.2} | audio period {:.2} ms | detect spread {:.2} ms | clock jitter {:.3} ms rms",
            frames.len(),
            avg(&self.awaits),
            avg(&self.delays),
            avg(&swaps),
            percentile(&swaps, 0.99),
            avg(&frames),
            percentile(&frames, 0.5),
            percentile(&frames, 0.99),
            max(&frames),
            avg(&self.cpu),
            max(&self.cpu),
            clock.period_ms,
            clock.spread_ms,
            clock.jitter_ms,
        )
    }
}

const WARMUP_FRAMES: u32 = 30;
const SAFETY_MS: f32 = 2.5;
const STEP_MS: f32 = 0.5;
const STEP_FRAMES: u32 = 10;
const BACKOFF_MS: f32 = 1.0;
// The cap only ever dropped, so one burst of load (another GPU app, busy cores) left the
// pacer at 0 ms for the rest of the session. After RECOVER_MS without a miss the cap rises
// by RECOVER_STEP_MS; a miss within PROBE_MS of a raise doubles the wait (up to 8x).
const RECOVER_MS: f32 = 4000.0;
const RECOVER_STEP_MS: f32 = 1.0;
const PROBE_MS: f32 = 2000.0;
const MAX_WAIT_MULT: u32 = 8;

pub struct Pacer {
    delay_ms: f32,
    cap_ms: f32,
    calm: u32,
    warmup: u32,
    quiet: u32,
    wait_mult: u32,
    probing: bool,
    pub reached_ms: f32,
    pub misses: u32,
}

impl Pacer {
    pub fn new() -> Self {
        Self {
            delay_ms: 0.0,
            cap_ms: f32::MAX,
            calm: 0,
            warmup: 0,
            quiet: 0,
            wait_mult: 1,
            probing: false,
            reached_ms: 0.0,
            misses: 0,
        }
    }

    pub fn delay_ms(&self) -> f32 {
        self.delay_ms
    }

    pub fn cap_ms(&self) -> Option<f32> {
        (self.cap_ms < f32::MAX).then_some(self.cap_ms)
    }

    pub fn update(&mut self, active: bool, frame_ms: f32, interval_ms: f32) {
        if !active {
            self.delay_ms = 0.0;
            self.calm = 0;
            self.warmup = 0;
            self.quiet = 0;
            return;
        }
        self.warmup += 1;
        if self.warmup < WARMUP_FRAMES || interval_ms <= 0.0 {
            return;
        }
        let top = (interval_ms - SAFETY_MS).max(0.0);
        let ceiling = top.min(self.cap_ms);
        self.quiet += 1;
        if frame_ms > interval_ms * 1.8 && self.delay_ms > 0.0 {
            self.misses += 1;
            if self.probing {
                self.wait_mult = (self.wait_mult * 2).min(MAX_WAIT_MULT);
            }
            self.probing = false;
            self.cap_ms = (self.delay_ms - BACKOFF_MS).max(0.0);
            self.delay_ms = self.cap_ms;
            self.calm = 0;
            self.quiet = 0;
        } else if self.delay_ms < ceiling {
            self.calm += 1;
            if self.calm >= STEP_FRAMES {
                self.calm = 0;
                self.delay_ms = (self.delay_ms + STEP_MS).min(ceiling);
            }
        } else {
            self.delay_ms = ceiling;
        }
        let frames = |ms: f32| (ms / interval_ms).ceil() as u32;
        if self.cap_ms < top && self.quiet >= frames(RECOVER_MS) * self.wait_mult {
            self.cap_ms += RECOVER_STEP_MS;
            if self.cap_ms >= top {
                self.cap_ms = f32::MAX;
            }
            self.quiet = 0;
            self.probing = true;
        } else if self.probing && self.quiet >= frames(PROBE_MS) {
            self.probing = false;
            self.wait_mult = 1;
        }
        self.reached_ms = self.reached_ms.max(self.delay_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: f32 = 6.94;

    fn run(p: &mut Pacer, frames: u32, frame_ms: f32) {
        for _ in 0..frames {
            p.update(true, frame_ms, INTERVAL);
        }
    }

    fn collapsed() -> Pacer {
        let mut p = Pacer::new();
        run(&mut p, 400, INTERVAL);
        assert!((p.delay_ms() - (INTERVAL - SAFETY_MS)).abs() < 0.01);
        for _ in 0..8 {
            p.update(true, INTERVAL * 2.0, INTERVAL);
            run(&mut p, 3, INTERVAL);
        }
        assert_eq!(p.delay_ms(), 0.0);
        assert_eq!(p.cap_ms(), Some(0.0));
        p
    }

    #[test]
    fn climbs_to_interval_minus_safety() {
        let mut p = Pacer::new();
        run(&mut p, 400, INTERVAL);
        assert!((p.delay_ms() - 4.44).abs() < 0.01);
    }

    #[test]
    fn miss_lowers_cap_and_delay() {
        let mut p = Pacer::new();
        run(&mut p, 400, INTERVAL);
        p.update(true, INTERVAL * 2.0, INTERVAL);
        assert_eq!(p.misses, 1);
        assert!((p.delay_ms() - 3.44).abs() < 0.01);
        run(&mut p, 100, INTERVAL);
        assert!(p.delay_ms() <= 3.44 + 0.01);
    }

    #[test]
    fn cap_recovers_after_calm_seconds() {
        let mut p = collapsed();
        run(&mut p, 400, INTERVAL);
        assert_eq!(p.delay_ms(), 0.0, "no recovery before the calm period");
        // 5 raises of 1 ms at 4 s each, plus the 0.5 ms delay ramp per 10 frames.
        run(&mut p, 5 * 600, INTERVAL);
        assert!(p.cap_ms().is_none());
        assert!((p.delay_ms() - (INTERVAL - SAFETY_MS)).abs() < 0.01);
    }

    #[test]
    fn delay_never_exceeds_interval_minus_safety() {
        let mut p = collapsed();
        for _ in 0..20_000 {
            p.update(true, INTERVAL, INTERVAL);
            assert!(p.delay_ms() <= INTERVAL - SAFETY_MS + 1e-4);
        }
    }

    #[test]
    fn miss_right_after_a_raise_backs_off_longer() {
        let mut p = collapsed();
        run(&mut p, 600, INTERVAL);
        // The first raise lifts the cap to 1 ms; a miss shortly after doubles the wait.
        let mut raised = false;
        for _ in 0..600 {
            p.update(true, INTERVAL, INTERVAL);
            if p.cap_ms() == Some(1.0) {
                raised = true;
                break;
            }
        }
        assert!(raised);
        run(&mut p, 20, INTERVAL);
        assert!(p.delay_ms() > 0.0);
        p.update(true, INTERVAL * 2.0, INTERVAL);
        assert_eq!(p.cap_ms(), Some(0.0));
        run(&mut p, 600 + 100, INTERVAL);
        assert_eq!(p.cap_ms(), Some(0.0), "wait doubled to 8 s");
        run(&mut p, 600, INTERVAL);
        assert_eq!(p.cap_ms(), Some(1.0));
    }

    #[test]
    fn inactive_resets_delay_but_not_cap() {
        let mut p = collapsed();
        p.update(false, INTERVAL, INTERVAL);
        assert_eq!(p.cap_ms(), Some(0.0));
        assert_eq!(p.delay_ms(), 0.0);
    }
}
