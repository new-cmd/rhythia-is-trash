use std::f64::consts::TAU;

use crate::map::{Map, Note};

pub const SAMPLE_RATE: u32 = 44_100;
const SR: f64 = SAMPLE_RATE as f64;
const BPM: f64 = 128.0;
const BARS: usize = 32;
const ROOTS: [f64; 4] = [55.0, 43.65, 65.41, 49.0];

pub struct Demo {
    pub map: Map,
    pub samples: Vec<f32>,
}

pub fn build() -> Demo {
    let beat = 60.0 / BPM;
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut out = vec![0.0f32; ((BARS * 4) as f64 * beat + 2.0) as usize * SAMPLE_RATE as usize];
    for b in 0..BARS * 4 {
        let t = b as f64 * beat;
        let bar = b / 4;
        kick(&mut out, t);
        if bar >= 8 && b % 2 == 1 {
            snare(&mut out, t, &mut rng);
        }
        for half in 0..2 {
            let t8 = t + half as f64 * beat / 2.0;
            if half == 1 || bar >= 4 {
                hat(&mut out, t8, &mut rng);
            }
            bass(&mut out, t8, ROOTS[bar % 4], beat / 2.0 * 0.95);
        }
    }
    for s in &mut out {
        *s = (*s * 0.9).tanh();
    }
    Demo {
        map: Map::new("Demo - 128 BPM", "", chart(beat, &mut rng), None),
        samples: out,
    }
}

fn chart(beat: f64, rng: &mut Rng) -> Vec<Note> {
    let mut notes = Vec::new();
    let mut cell = (1, 1);
    for bar in 1..BARS {
        let step = match bar {
            1..=8 => 2,
            9..=16 if bar % 2 == 1 => 2,
            25.. if bar % 4 == 3 => 2,
            _ => 1,
        };
        for eighth in (0..8).step_by(step) {
            cell = next_cell(cell, rng);
            let time = (bar * 4) as f64 * beat + eighth as f64 * beat / 2.0;
            notes.push(Note {
                time,
                x: cell.0 as f32 - 1.0,
                y: 1.0 - cell.1 as f32,
            });
        }
    }
    notes
}

fn next_cell(from: (i32, i32), rng: &mut Rng) -> (i32, i32) {
    loop {
        let to = (rng.below(3) as i32, rng.below(3) as i32);
        let dist = (to.0 - from.0).abs().max((to.1 - from.1).abs());
        if dist == 1 || (dist == 2 && rng.below(4) == 0) {
            return to;
        }
    }
}

fn voice(out: &mut [f32], start: f64, dur: f64, mut sample: impl FnMut(f64) -> f64) {
    let a = (start * SR) as usize;
    let b = (((start + dur) * SR) as usize).min(out.len());
    for (i, s) in out[a.min(b)..b].iter_mut().enumerate() {
        *s += sample(i as f64 / SR) as f32;
    }
}

fn kick(out: &mut [f32], t: f64) {
    let mut phase = 0.0;
    voice(out, t, 0.35, |x| {
        phase += TAU * (45.0 + 85.0 * (-x * 30.0).exp()) / SR;
        phase.sin() * (-x * 9.0).exp() * 0.9
    });
}

fn snare(out: &mut [f32], t: f64, rng: &mut Rng) {
    voice(out, t, 0.2, |x| {
        rng.signed() * 0.35 * (-x * 22.0).exp() + (TAU * 185.0 * x).sin() * 0.25 * (-x * 30.0).exp()
    });
}

fn hat(out: &mut [f32], t: f64, rng: &mut Rng) {
    let mut prev = 0.0;
    voice(out, t, 0.08, |x| {
        let n = rng.signed();
        let hp = n - prev;
        prev = n;
        hp * 0.1 * (-x * 60.0).exp()
    });
}

fn bass(out: &mut [f32], t: f64, freq: f64, dur: f64) {
    voice(out, t, dur, |x| {
        let saw = 2.0 * (x * freq).fract() - 1.0;
        saw * 0.18 * (-x * 6.0).exp() * (x * 200.0).min(1.0)
    });
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn signed(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
}
