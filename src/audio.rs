use std::{collections::VecDeque, io::Cursor};

use kira::{
    AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Frame, Tween,
    sound::{
        PlaybackPosition, PlaybackState,
        static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings},
    },
};

use crate::sfx;

const SNAP_THRESHOLD: f64 = 0.05;
const FFMPEG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
const WINDOW: f64 = 3.0;
const MAX_SLEW: f64 = 0.0005;
const PERIODS: usize = 128;
const SFX_VOLUME_DB: f32 = -6.0;

pub fn decode(bytes: Vec<u8>) -> Result<StaticSoundData, String> {
    let shared: std::sync::Arc<[u8]> = bytes.into();
    match StaticSoundData::from_cursor(Cursor::new(shared.clone())) {
        Ok(data) => Ok(data),
        Err(first) => via_ffmpeg(&shared)
            .map_err(|e| format!("audio: {first} (and ffmpeg could not read it either: {e})")),
    }
}

fn via_ffmpeg(bytes: &[u8]) -> Result<StaticSoundData, String> {
    use std::{
        fs,
        process::Command,
        sync::atomic::{AtomicU32, Ordering},
    };

    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "rhythm-decode-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let result = (|| {
        let (input, output) = (dir.join("in"), dir.join("out.wav"));
        fs::write(&input, bytes).map_err(|e| e.to_string())?;
        let mut child = Command::new("ffmpeg")
            .args(["-v", "error", "-nostdin", "-y", "-i"])
            .arg(&input)
            .args(["-vn", "-ac", "2", "-ar", "48000", "-c:a", "pcm_s16le"])
            .arg(&output)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("ffmpeg not available: {e}"))?;
        let started = std::time::Instant::now();
        let status = loop {
            match child.try_wait().map_err(|e| e.to_string())? {
                Some(status) => break status,
                None if started.elapsed() > FFMPEG_TIMEOUT => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("ffmpeg took too long".into());
                }
                None => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        };
        if !status.success() {
            return Err("ffmpeg could not decode it".into());
        }
        let wav = fs::read(&output).map_err(|e| e.to_string())?;
        StaticSoundData::from_cursor(Cursor::new(wav)).map_err(|e| e.to_string())
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

pub fn from_mono(samples: &[f32], sample_rate: u32) -> StaticSoundData {
    StaticSoundData {
        sample_rate,
        frames: samples
            .iter()
            .map(|&s| Frame { left: s, right: s })
            .collect(),
        settings: StaticSoundSettings::default(),
        slice: None,
    }
}

fn tuned(frames: u32) -> Option<AudioManagerSettings<DefaultBackend>> {
    use kira::backend::cpal::{
        CpalBackendSettings,
        cpal::{
            self, BufferSize,
            traits::{DeviceTrait, HostTrait},
        },
    };
    let device = cpal::default_host().default_output_device()?;
    let mut config = device.default_output_config().ok()?.config();
    config.buffer_size = BufferSize::Fixed(frames);
    Some(AudioManagerSettings {
        backend_settings: CpalBackendSettings {
            device: Some(device),
            config: Some(config),
        },
        ..Default::default()
    })
}

fn open_manager(frames: u32) -> Option<AudioManager<DefaultBackend>> {
    let default = || AudioManager::<DefaultBackend>::new(AudioManagerSettings::default());
    let manager = match tuned(frames).filter(|_| frames > 0) {
        Some(settings) => AudioManager::<DefaultBackend>::new(settings).or_else(|e| {
            eprintln!("audio buffer {frames}: {e}; using the default");
            default()
        }),
        None => default(),
    };
    manager
        .inspect_err(|e| eprintln!("audio disabled: {e}"))
        .ok()
}

fn db(volume: f32) -> f32 {
    if volume <= 0.001 {
        -60.0
    } else {
        20.0 * volume.log10()
    }
}
#[derive(Clone, Copy)]
pub enum Sfx {
    Hit,
    Miss,
    Confirm,
    Clear,
    Fail,
    LevelUp,
}

#[derive(Clone, Copy, Default)]
pub struct ClockStats {
    pub period_ms: f32,
    pub spread_ms: f32,
    pub jitter_ms: f32,
}

pub struct Music {
    manager: Option<AudioManager<DefaultBackend>>,
    track: Option<StaticSoundData>,
    hit: StaticSoundData,
    miss: StaticSoundData,
    cues: [StaticSoundData; 4],
    handle: Option<StaticSoundHandle>,
    time: f64,
    reported: f64,
    paused: bool,
    local: f64,
    offset: f64,
    last_change: f64,
    samples: VecDeque<(f64, f64)>,
    periods: VecDeque<f64>,
    jitter_sq: f64,
    jitter_n: u32,
}

impl Music {
    pub fn new(
        track: Option<StaticSoundData>,
        music_volume: f32,
        sfx_volume: f32,
        buffer_frames: u32,
    ) -> Self {
        let manager = open_manager(buffer_frames);
        let sfx = |s: Vec<f32>| {
            from_mono(&s, sfx::SAMPLE_RATE).volume(Decibels(SFX_VOLUME_DB + db(sfx_volume)))
        };
        Self {
            manager,
            track: track.map(|t| t.volume(Decibels(db(music_volume)))),
            hit: sfx(sfx::hit()),
            miss: sfx(sfx::miss()),
            cues: [
                sfx(sfx::confirm()),
                sfx(sfx::clear()),
                sfx(sfx::fail()),
                sfx(sfx::level_up()),
            ],
            handle: None,
            time: 0.0,
            reported: f64::NAN,
            paused: false,
            local: 0.0,
            offset: 0.0,
            last_change: 0.0,
            samples: VecDeque::new(),
            periods: VecDeque::new(),
            jitter_sq: 0.0,
            jitter_n: 0,
        }
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    pub fn duration(&self) -> Option<f64> {
        self.track.as_ref().map(|t| t.duration().as_secs_f64())
    }

    pub fn play_sfx(&mut self, which: Sfx) {
        let data = match which {
            Sfx::Hit => &self.hit,
            Sfx::Miss => &self.miss,
            Sfx::Confirm => &self.cues[0],
            Sfx::Clear => &self.cues[1],
            Sfx::Fail => &self.cues[2],
            Sfx::LevelUp => &self.cues[3],
        };
        if let Some(m) = &mut self.manager {
            let _ = m.play(data.clone());
        }
    }

    pub fn restart(&mut self, from: f64) {
        self.stop();
        self.time = from;
        self.reported = f64::NAN;
        self.paused = false;
        self.samples.clear();
        self.periods.clear();
        self.jitter_sq = 0.0;
        self.jitter_n = 0;
    }

    pub fn stop(&mut self) {
        if let Some(mut h) = self.handle.take() {
            h.stop(Tween::default());
        }
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if let Some(h) = &mut self.handle {
            if paused {
                h.pause(Tween::default());
            } else {
                h.resume(Tween::default());
            }
        }
    }

    pub fn clock_stats(&self) -> ClockStats {
        let mut p: Vec<f64> = self.periods.iter().copied().collect();
        p.sort_by(f64::total_cmp);
        let hi = self.samples.iter().map(|s| s.1).fold(f64::MIN, f64::max);
        let lo = self.samples.iter().map(|s| s.1).fold(f64::MAX, f64::min);
        ClockStats {
            period_ms: p.get(p.len() / 2).map_or(0.0, |v| (*v * 1000.0) as f32),
            spread_ms: if self.samples.is_empty() {
                0.0
            } else {
                ((hi - lo) * 1000.0) as f32
            },
            jitter_ms: if self.jitter_n == 0 {
                0.0
            } else {
                ((self.jitter_sq / f64::from(self.jitter_n)).sqrt() * 1000.0) as f32
            },
        }
    }

    pub fn update(&mut self, dt: f64) {
        if self.paused {
            return;
        }
        self.local += dt;
        let before = self.time;
        self.time += dt;
        if self.handle.is_none()
            && self.time >= 0.0
            && let (Some(m), Some(t)) = (&mut self.manager, &self.track)
        {
            match m.play(t.start_position(PlaybackPosition::Seconds(self.time))) {
                Ok(h) => {
                    self.handle = Some(h);
                    self.offset = self.time - self.local;
                    self.samples.clear();
                }
                Err(e) => eprintln!("playback failed: {e}"),
            }
        }
        let Some(h) = &self.handle else { return };
        if h.state() != PlaybackState::Playing {
            return;
        }
        let pos = h.position();
        if pos != self.reported {
            if self.reported.is_finite() {
                if self.periods.len() == PERIODS {
                    self.periods.pop_front();
                }
                self.periods.push_back(self.local - self.last_change);
            }
            self.last_change = self.local;
            self.reported = pos;
            self.samples.push_back((self.local, pos - self.local));
            while self
                .samples
                .front()
                .is_some_and(|s| self.local - s.0 > WINDOW)
            {
                self.samples.pop_front();
            }
        }
        if let Some(target) = self.samples.iter().map(|s| s.1).reduce(f64::max) {
            let diff = target - self.offset;
            self.offset += if diff.abs() > SNAP_THRESHOLD {
                diff
            } else {
                diff.clamp(-MAX_SLEW, MAX_SLEW)
            };
        }
        self.time = self.local + self.offset;
        let step = (self.time - before) - dt;
        self.jitter_sq += step * step;
        self.jitter_n += 1;
    }
}
