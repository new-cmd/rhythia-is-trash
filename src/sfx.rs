use std::f32::consts::TAU;

pub const SAMPLE_RATE: u32 = 44_100;
const SR: f32 = SAMPLE_RATE as f32;

struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

fn render(seconds: f32, mut f: impl FnMut(f32) -> f32) -> Vec<f32> {
    (0..(seconds * SR) as usize)
        .map(|i| f(i as f32 / SR).clamp(-1.0, 1.0))
        .collect()
}

pub fn hit() -> Vec<f32> {
    let mut noise = Noise(0x1234_5678);
    render(0.1, |t| {
        let attack = (t * SR / 16.0).min(1.0);
        let fm = (TAU * 3200.0 * t).sin() * 0.5 * (-t * 90.0).exp();
        let glass = (TAU * 1600.0 * t + fm).sin() * 0.26 * (-t * 65.0).exp();
        let body = (TAU * 800.0 * t).sin() * 0.15 * (-t * 80.0).exp();
        let contact = noise.next() * 0.12 * (-t * 500.0).exp();
        (glass + body + contact) * attack
    })
}

pub fn miss() -> Vec<f32> {
    let mut phase = 0.0;
    render(0.14, |t| {
        let attack = (t * SR / 32.0).min(1.0);
        phase += TAU * (130.0 + 110.0 * (-t * 30.0).exp()) / SR;
        let fundamental = phase.sin();
        let velvet = (phase * 1.5).sin() * 0.18;
        (fundamental + velvet) * 0.34 * attack * (-t * 36.0).exp()
    })
}
pub fn confirm() -> Vec<f32> {
    chime(&[880.0, 1320.0], 0.07, 0.24)
}

pub fn clear() -> Vec<f32> {
    chime(&[523.25, 659.25, 783.99, 1046.5], 0.065, 0.65)
}

pub fn fail() -> Vec<f32> {
    chime(&[440.0, 349.23, 261.63], 0.09, 0.5)
}
pub fn level_up() -> Vec<f32> {
    chime(&[784.0, 1174.66, 1568.0], 0.045, 0.32)
}

fn chime(notes: &[f32], stagger: f32, duration: f32) -> Vec<f32> {
    render(duration, |t| {
        let release = ((duration - t) / 0.04).clamp(0.0, 1.0);
        notes
            .iter()
            .enumerate()
            .map(|(i, freq)| {
                let x = t - i as f32 * stagger;
                if x < 0.0 {
                    return 0.0;
                }
                let attack = (x * 400.0).min(1.0);
                let fundamental = (TAU * freq * x).sin();
                let overtone = (TAU * freq * 2.0 * x).sin() * 0.15;
                (fundamental + overtone) * attack * (-x * 12.0).exp() * 0.14
            })
            .sum::<f32>()
            * release
    })
}
