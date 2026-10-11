use macroquad::prelude::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub struct Summary {
    pub count: usize,
    pub mean_ms: f32,
    pub spread_ms: f32,
    pub p95_ms: f32,
    pub max_ms: f32,
}

#[allow(dead_code)]
pub fn summarize(lates_ms: &[f32]) -> Option<Summary> {
    let mut values: Vec<f32> = lates_ms.iter().copied().filter(|v| v.is_finite()).collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let count = values.len();
    let mean_ms = values.iter().sum::<f32>() / count as f32;
    let spread_ms =
        (values.iter().map(|v| (v - mean_ms).powi(2)).sum::<f32>() / count as f32).sqrt();
    let p95_ms = values[((count as f32 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(count - 1)];
    Some(Summary {
        count,
        mean_ms,
        spread_ms,
        p95_ms,
        max_ms: values[count - 1],
    })
}

#[derive(Clone, Debug, Default)]
pub struct Replay {
    samples: Vec<(f32, Vec2)>,
}

#[allow(dead_code)]
impl Replay {
    const CAP: usize = 400_000;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, t: f64, pos: Vec2) {
        if !t.is_finite() || self.samples.len() >= Self::CAP {
            return;
        }
        let t = t as f32;
        if let Some(&(last_t, last_pos)) = self.samples.last() {
            if t <= last_t {
                return;
            }
            if t - last_t <= 0.004 && (pos - last_pos).length() <= 0.0005 {
                return;
            }
        }
        self.samples.push((t, pos));
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn sample_at(&self, t: f64) -> Vec2 {
        let Some(&(first_t, first_pos)) = self.samples.first() else {
            return Vec2::ZERO;
        };
        if !t.is_finite() || t <= f64::from(first_t) {
            return first_pos;
        }
        let &(last_t, last_pos) = self.samples.last().expect("nonempty replay");
        if t >= f64::from(last_t) {
            return last_pos;
        }
        let i = self
            .samples
            .partition_point(|(sample_t, _)| f64::from(*sample_t) < t);
        let (a_t, a) = self.samples[i - 1];
        let (b_t, b) = self.samples[i];
        a.lerp(b, ((t - f64::from(a_t)) / f64::from(b_t - a_t)) as f32)
    }

    pub fn span(&self) -> Option<(f64, f64)> {
        Some((
            f64::from(self.samples.first()?.0),
            f64::from(self.samples.last()?.0),
        ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sampling_at_a_non_finite_time_is_safe() {
        let mut r = Replay::new();
        r.record(1.0, vec2(0.0, 0.0));
        r.record(2.0, vec2(1.0, 1.0));
        assert_eq!(r.sample_at(f64::NAN), vec2(0.0, 0.0));
        assert_eq!(r.sample_at(f64::INFINITY), vec2(0.0, 0.0));
    }

    use super::*;
    use macroquad::prelude::vec2;

    #[test]
    fn summary_filters_non_finite_and_uses_population_spread() {
        assert!(summarize(&[]).is_none());
        assert!(summarize(&[f32::NAN, f32::INFINITY]).is_none());
        let s = summarize(&[0.0, 2.0, f32::NAN]).unwrap();
        assert_eq!(s.count, 2);
        assert_eq!(s.mean_ms, 1.0);
        assert_eq!(s.spread_ms, 1.0);
        assert_eq!(s.p95_ms, 2.0);
        assert_eq!(s.max_ms, 2.0);
        assert_eq!(summarize(&[-3.0]).unwrap().spread_ms, 0.0);
    }

    #[test]
    fn replay_interpolates_and_clamps() {
        let mut r = Replay::new();
        r.record(1.0, vec2(0.0, 2.0));
        r.record(2.0, vec2(4.0, 6.0));
        assert_eq!(r.sample_at(0.0), vec2(0.0, 2.0));
        assert_eq!(r.sample_at(1.5), vec2(2.0, 4.0));
        assert_eq!(r.sample_at(3.0), vec2(4.0, 6.0));
        assert_eq!(r.span(), Some((1.0, 2.0)));
    }

    #[test]
    fn replay_enforces_order_deduplicates_and_caps() {
        let mut r = Replay::new();
        r.record(1.0, Vec2::ZERO);
        r.record(1.002, vec2(0.0001, 0.0));
        r.record(0.9, vec2(1.0, 0.0));
        assert_eq!(r.len(), 1);
        r.record(1.01, vec2(1.0, 0.0));
        assert_eq!(r.len(), 2);
        let mut capped = Replay::new();
        for i in 0..401_000 {
            capped.record(i as f64 * 0.01, vec2(i as f32, 0.0));
        }
        assert_eq!(capped.len(), 400_000);
        assert!(!capped.is_empty());
    }
}
