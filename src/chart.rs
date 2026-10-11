use std::f32::consts::PI;

use kira::sound::static_sound::StaticSoundData;

use crate::map::Note;

const FFT: usize = 1024;
const MIN_GAP: f64 = 0.12;
const SMOOTH_RADIUS: f64 = 0.5;
const PEAK_RADIUS: usize = 3;
const SENSITIVITY: f32 = 1.0;
const FLOOR: f32 = 0.04;

pub struct Onset {
    pub time: f64,
    pub strength: f32,
}

pub fn generate(data: &StaticSoundData) -> Vec<Note> {
    let mono: Vec<f32> = data
        .frames
        .iter()
        .map(|f| (f.left + f.right) * 0.5)
        .collect();
    let onsets = onsets(&mono, data.sample_rate);
    layout(&onsets, mono.len() as u64 ^ u64::from(data.sample_rate))
}

struct Fft {
    window: Vec<f32>,
    twiddle: Vec<(f32, f32)>,
    reverse: Vec<usize>,
}

impl Fft {
    fn new() -> Self {
        let bits = FFT.trailing_zeros();
        Self {
            window: (0..FFT)
                .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FFT as f32).cos())
                .collect(),
            twiddle: (0..FFT / 2)
                .map(|k| {
                    let a = -2.0 * PI * k as f32 / FFT as f32;
                    (a.cos(), a.sin())
                })
                .collect(),
            reverse: (0..FFT)
                .map(|i| i.reverse_bits() >> (usize::BITS - bits))
                .collect(),
        }
    }

    fn magnitudes(&self, samples: &[f32], out: &mut [f32]) {
        let mut re = vec![0.0_f32; FFT];
        let mut im = vec![0.0_f32; FFT];
        for i in 0..FFT {
            re[self.reverse[i]] = samples.get(i).copied().unwrap_or(0.0) * self.window[i];
        }
        let mut size = 2;
        while size <= FFT {
            let half = size / 2;
            let step = FFT / size;
            for start in (0..FFT).step_by(size) {
                for k in 0..half {
                    let (wr, wi) = self.twiddle[k * step];
                    let (a, b) = (start + k, start + k + half);
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            size *= 2;
        }
        for (k, m) in out.iter_mut().enumerate() {
            *m = (re[k] * re[k] + im[k] * im[k]).sqrt();
        }
    }
}

fn flux(mono: &[f32], hop: usize) -> Vec<f32> {
    let fft = Fft::new();
    let bins = FFT / 2;
    let mut prev = vec![0.0_f32; bins];
    let mut mags = vec![0.0_f32; bins];
    let frames = mono.len().saturating_sub(FFT) / hop + 1;
    let mut out = Vec::with_capacity(frames);
    for f in 0..frames {
        let at = f * hop;
        fft.magnitudes(&mono[at..(at + FFT).min(mono.len())], &mut mags);
        let mut sum = 0.0;
        for k in 1..bins {
            let level = (1.0 + 20.0 * mags[k] / FFT as f32).ln();
            sum += (level - prev[k]).max(0.0);
            prev[k] = level;
        }
        out.push(sum);
    }
    out
}

pub fn onsets(mono: &[f32], sample_rate: u32) -> Vec<Onset> {
    let hop = (sample_rate as usize / 100).max(1);
    let hop_s = hop as f64 / f64::from(sample_rate);
    let curve = flux(mono, hop);
    if curve.is_empty() {
        return Vec::new();
    }
    let radius = (SMOOTH_RADIUS / hop_s) as usize;
    let mut prefix = vec![0.0_f64];
    for v in &curve {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + f64::from(*v));
    }
    let local_mean = |i: usize| {
        let lo = i.saturating_sub(radius);
        let hi = (i + radius + 1).min(curve.len());
        ((prefix[hi] - prefix[lo]) / (hi - lo) as f64) as f32
    };
    let peak = curve.iter().copied().fold(0.0_f32, f32::max);
    if peak <= 0.0 {
        return Vec::new();
    }
    let mut found: Vec<Onset> = (0..curve.len())
        .filter(|&i| {
            let lo = i.saturating_sub(PEAK_RADIUS);
            let hi = (i + PEAK_RADIUS + 1).min(curve.len());
            let v = curve[i];
            v >= peak * FLOOR
                && v > local_mean(i) * SENSITIVITY
                && curve[lo..hi].iter().all(|o| *o <= v)
                && (lo..i).all(|j| curve[j] < v)
        })
        .map(|i| Onset {
            time: (i * hop + FFT / 2) as f64 / f64::from(sample_rate),
            strength: curve[i] / peak,
        })
        .collect();
    found.sort_by(|a, b| b.strength.total_cmp(&a.strength));
    let mut kept: Vec<Onset> = Vec::with_capacity(found.len());
    for o in found {
        if kept.iter().all(|k| (k.time - o.time).abs() >= MIN_GAP) {
            kept.push(o);
        }
    }
    kept.sort_by(|a, b| a.time.total_cmp(&b.time));
    kept
}

fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn reach(gap: f64) -> i32 {
    if gap < 0.22 {
        1
    } else if gap < 0.5 {
        2
    } else {
        3
    }
}

pub fn layout(onsets: &[Onset], seed: u64) -> Vec<Note> {
    let mut notes = Vec::with_capacity(onsets.len());
    let mut at = (1_i32, 1_i32);
    let mut last_time = f64::NEG_INFINITY;
    for (i, o) in onsets.iter().enumerate() {
        let r = reach(o.time - last_time);
        let mut options: Vec<(i32, i32)> = (0..3)
            .flat_map(|y| (0..3).map(move |x| (x, y)))
            .filter(|&(x, y)| (x, y) != at && (x - at.0).abs().max((y - at.1).abs()) <= r)
            .collect();
        options.sort_by_key(|&(x, y)| (x - at.0).abs().max((y - at.1).abs()));
        let roll = (mix(seed ^ (i as u64).wrapping_mul(0x9e37_79b9)) % 1000) as f32 / 1000.0;
        let far = (roll * 0.6 + o.strength * 0.4).clamp(0.0, 0.999);
        at = options[(far * options.len() as f32) as usize];
        last_time = o.time;
        notes.push(Note {
            time: o.time,
            x: at.0 as f32 - 1.0,
            y: 1.0 - at.1 as f32,
        });
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    fn clicks(times: &[f64], seconds: f64) -> Vec<f32> {
        let mut out = vec![0.0_f32; (seconds * f64::from(RATE)) as usize];
        for t in times {
            let start = (t * f64::from(RATE)) as usize;
            for k in 0..2000 {
                let decay = (-(k as f32) / 300.0).exp();
                let tone = (2.0 * PI * 880.0 * k as f32 / RATE as f32).sin();
                if let Some(s) = out.get_mut(start + k) {
                    *s += 0.8 * decay * tone;
                }
            }
        }
        out
    }

    #[test]
    fn fft_puts_a_sine_in_its_bin() {
        let fft = Fft::new();
        let samples: Vec<f32> = (0..FFT)
            .map(|i| (2.0 * PI * 64.0 * i as f32 / FFT as f32).sin())
            .collect();
        let mut mags = vec![0.0; FFT / 2];
        fft.magnitudes(&samples, &mut mags);
        let best = mags
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(k, _)| k);
        assert_eq!(best, Some(64));
    }

    #[test]
    fn finds_clicks_where_they_are() {
        let times = [1.0, 1.5, 2.0, 2.25, 3.0, 4.2];
        let found = onsets(&clicks(&times, 6.0), RATE);
        assert_eq!(
            found.len(),
            times.len(),
            "{:?}",
            found.iter().map(|o| o.time).collect::<Vec<_>>()
        );
        for (o, t) in found.iter().zip(times) {
            assert!((o.time - t).abs() < 0.04, "onset {} vs click {t}", o.time);
        }
    }

    #[test]
    fn silence_has_no_onsets() {
        assert!(onsets(&vec![0.0; RATE as usize * 3], RATE).is_empty());
        assert!(onsets(&[], RATE).is_empty());
    }

    #[test]
    fn dense_input_never_exceeds_the_minimum_gap() {
        let times: Vec<f64> = (0..80).map(|i| 0.5 + f64::from(i) * 0.05).collect();
        let found = onsets(&clicks(&times, 5.0), RATE);
        for pair in found.windows(2) {
            assert!(pair[1].time - pair[0].time >= MIN_GAP - 1e-9);
        }
    }

    #[test]
    fn layout_is_deterministic_on_the_grid_and_moves_every_note() {
        let onsets: Vec<Onset> = (0..200)
            .map(|i| Onset {
                time: f64::from(i) * 0.18 + f64::from(i % 7) * 0.2,
                strength: (i % 5) as f32 / 5.0,
            })
            .collect();
        let a = layout(&onsets, 7);
        let b = layout(&onsets, 7);
        assert_eq!(a.len(), 200);
        let mut seen = std::collections::HashSet::new();
        for (i, n) in a.iter().enumerate() {
            assert_eq!((n.x, n.y), (b[i].x, b[i].y));
            assert!([-1.0, 0.0, 1.0].contains(&n.x) && [-1.0, 0.0, 1.0].contains(&n.y));
            seen.insert((n.x as i32, n.y as i32));
            if i > 0 {
                assert_ne!(
                    (n.x, n.y),
                    (a[i - 1].x, a[i - 1].y),
                    "note {i} repeats the cell"
                );
                let gap = onsets[i].time - onsets[i - 1].time;
                let step = (n.x - a[i - 1].x).abs().max((n.y - a[i - 1].y).abs()) as i32;
                assert!(step <= reach(gap), "note {i}: step {step} for gap {gap}");
            }
        }
        assert_eq!(seen.len(), 9, "all nine cells get used");
    }
}
