use std::collections::VecDeque;

use macroquad::prelude::*;

use crate::{
    audio::{Music, Sfx},
    map::Map,
    perf::Stats,
    render::Space,
    scores::Best,
    settings::Settings,
};

pub const HIT_WINDOW: f64 = 0.055;
pub const HITBOX_PAD: f32 = 0.07;
pub const GRID_SIZE: f32 = 3.0;
pub const CURSOR_SIZE: f32 = 0.2625;
pub const BOUNDS: f32 = GRID_SIZE / 2.0 - CURSOR_SIZE / 2.0;
pub const POPUP_LIFE: f64 = 0.25;
pub const BREAK_LIFE: f64 = 0.5;
pub const TRAIL_TIME: f64 = 0.05;
pub const MAX_MULTIPLIER: u32 = 8;
const LEAD_IN: f64 = 2.0;
const RETRY_RUN_UP: f64 = 0.5;
const OUTRO: f64 = 1.5;
const IDLE_OPACITY: f32 = 0.62;

#[derive(Clone, Copy, PartialEq)]
pub enum NoteState {
    Pending,
    Hit,
    Missed,
}

pub fn autopilot(notes: &[crate::map::Note], t: f64) -> Vec2 {
    let i = notes.partition_point(|n| n.time <= t);
    let at = |n: &crate::map::Note| vec2(n.x, n.y);
    match (i.checked_sub(1).map(|j| notes[j]), notes.get(i)) {
        (Some(p), Some(n)) => {
            let s = (((t - p.time) / (n.time - p.time).max(1e-6)) as f32).clamp(0.0, 1.0);
            at(&p).lerp(at(n), s * s * (3.0 - 2.0 * s))
        }
        (Some(p), None) => at(&p),
        (None, Some(n)) => at(n),
        (None, None) => Vec2::ZERO,
    }
}

pub fn grade_for(accuracy: f64) -> &'static str {
    match accuracy {
        a if a >= 100.0 => "SS",
        a if a >= 95.0 => "S",
        a if a >= 90.0 => "A",
        a if a >= 80.0 => "B",
        a if a >= 70.0 => "C",
        _ => "D",
    }
}

pub fn start_time(first_note: f64, approach: f64, retry: bool) -> f64 {
    if retry {
        (first_note - approach - RETRY_RUN_UP).max(-LEAD_IN)
    } else {
        -LEAD_IN
    }
}

pub enum Verdict {
    Hit(f64),
    Miss,
    Pending,
}

fn segment_overlap(a: Vec2, b: Vec2, center: Vec2, reach: f32) -> Option<(f32, f32)> {
    let d = b - a;
    let (mut s0, mut s1) = (0.0_f32, 1.0_f32);
    for axis in 0..2 {
        let lo = center[axis] - reach;
        let hi = center[axis] + reach;
        if d[axis].abs() < 1e-9 {
            if a[axis] < lo || a[axis] > hi {
                return None;
            }
            continue;
        }
        let (u, v) = ((lo - a[axis]) / d[axis], (hi - a[axis]) / d[axis]);
        s0 = s0.max(u.min(v));
        s1 = s1.min(u.max(v));
    }
    (s0 <= s1).then_some((s0, s1))
}

pub fn judge_note(
    prev: Option<(f64, Vec2)>,
    (now, cursor): (f64, Vec2),
    note_time: f64,
    pos: Vec2,
) -> Verdict {
    if now < note_time {
        return Verdict::Pending;
    }
    let reach = 0.5 + HITBOX_PAD;
    let (from_time, from) = prev.filter(|(t, _)| *t < now).unwrap_or((now, cursor));
    let span = now - from_time;
    let lo = from_time.max(note_time);
    let hi = now.min(note_time + HIT_WINDOW);
    if lo <= hi {
        let (a, b) = if span > 0.0 {
            (
                ((lo - from_time) / span) as f32,
                ((hi - from_time) / span) as f32,
            )
        } else {
            (1.0, 1.0)
        };
        if let Some((s0, s1)) = segment_overlap(from, cursor, pos, reach) {
            let s = s0.max(a);
            if s <= s1.min(b) {
                return Verdict::Hit(from_time + f64::from(s) * span - note_time);
            }
        }
    }
    if now - note_time > HIT_WINDOW {
        Verdict::Miss
    } else {
        Verdict::Pending
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Phase {
    Ready,
    Playing,
    Paused,
    Finished,
}

pub enum Action {
    Continue,
    Back,
}

#[derive(Clone)]
pub struct Effect {
    pub x: f32,
    pub pos: Vec2,
    pub born: f64,
    pub note: usize,
    pub score: Option<u32>,
}

#[derive(Clone)]
pub struct Hud {
    pub multiplier_progress: f32,
    pub multiplier_color: Color,
    pub hits_opacity: f32,
    pub misses_opacity: f32,
    pub health: f32,
    pub space_color: Color,
    pub score_shown: f32,
    pub mult_pulse: f32,
    pub last_multiplier: u32,
}

pub struct Game {
    pub map: Map,
    pub music: Music,
    pub palette: Vec<Color>,
    pub phase: Phase,
    pub phase_since: f64,
    pub space: Space,
    pub stats: Stats,
    pub settings: Settings,
    pub run: Run,
    pub watching: Option<crate::stats::Replay>,
    saved: Option<Run>,
    pub sensitivity: f32,
    pub offset: f64,
    pub key: String,
    pub best: Option<Best>,
    pub new_best: bool,
    pub recorded: bool,
    pub autoplay: bool,
    pub load_ms: [f32; 3],
    last_mouse: Option<Vec2>,
    last_tick: f64,
}

#[derive(Clone)]
pub struct Run {
    pub states: Vec<NoteState>,
    pub first: usize,
    pub failed: bool,
    pub cursor: Vec2,
    pub trail: VecDeque<(f64, Vec2)>,
    pub score: u64,
    pub combo: u32,
    pub max_combo: u32,
    pub multiplier: u32,
    pub multiplier_progress: u32,
    pub multiplier_increment: u32,
    pub hits: u32,
    pub misses: u32,
    pub health: f64,
    pub health_step: f64,
    pub last_hit: Option<usize>,
    pub effects: Vec<Effect>,
    pub hud: Hud,
    pub prev: Option<(f64, Vec2)>,
    pub sweep_hits: u32,
    pub lates_ms: Vec<f32>,
    pub replay: crate::stats::Replay,
    pub started_at: f64,
}

impl Run {
    fn new(notes: usize, space_color: Color) -> Self {
        Self {
            states: vec![NoteState::Pending; notes],
            multiplier_increment: (notes as u32 / 200).max(2),
            first: 0,
            failed: false,
            cursor: Vec2::ZERO,
            trail: VecDeque::new(),
            score: 0,
            combo: 0,
            max_combo: 0,
            multiplier: 1,
            multiplier_progress: 0,
            hits: 0,
            misses: 0,
            health: 100.0,
            health_step: 15.0,
            last_hit: None,
            effects: Vec::new(),
            prev: None,
            sweep_hits: 0,
            lates_ms: Vec::new(),
            replay: crate::stats::Replay::new(),
            started_at: 0.0,
            hud: Hud {
                multiplier_progress: 0.0,
                multiplier_color: WHITE,
                hits_opacity: IDLE_OPACITY,
                misses_opacity: IDLE_OPACITY,
                health: 1.0,
                space_color,
                score_shown: 0.0,
                mult_pulse: 0.0,
                last_multiplier: 1,
            },
        }
    }

    pub fn judged(&self) -> u32 {
        self.hits + self.misses
    }

    pub fn accuracy(&self) -> f64 {
        match self.judged() {
            0 => 100.0,
            n => (self.hits as f64 / n as f64 * 10_000.0).floor() / 100.0,
        }
    }

    pub fn grade(&self) -> &'static str {
        if self.failed {
            return "F";
        }
        grade_for(self.accuracy())
    }
}

impl Game {
    pub fn new(map: Map, music: Music, palette: Vec<Color>, settings: Settings) -> Self {
        let run = Run::new(
            map.notes.len(),
            *palette.last().expect("palette is not empty"),
        );
        Self {
            map,
            music,
            palette,
            phase: Phase::Ready,
            phase_since: get_time(),
            space: settings.space,
            stats: Stats::default(),
            run,
            watching: None,
            saved: None,
            sensitivity: settings.sensitivity,
            offset: f64::from(settings.offset_ms) / 1000.0,
            key: String::new(),
            best: None,
            new_best: false,
            recorded: false,
            autoplay: false,
            load_ms: [0.0; 3],
            last_mouse: None,
            last_tick: get_time(),
            settings,
        }
    }

    pub fn note_color(&self, i: usize) -> Color {
        self.palette[i % self.palette.len()]
    }

    #[allow(dead_code)]
    pub fn summary(&self) -> Option<crate::stats::Summary> {
        crate::stats::summarize(&self.run.lates_ms)
    }

    pub fn watch_replay(&mut self) {
        if self.phase != Phase::Finished || self.run.replay.is_empty() {
            return;
        }
        let replay = self.run.replay.clone();
        let started_at = self.run.started_at;
        self.saved = Some(self.run.clone());
        self.watching = Some(replay);
        self.start_at(started_at, false);
        self.recorded = true;
        self.new_best = false;
    }

    fn end_replay(&mut self) {
        if self.watching.take().is_some() {
            if let Some(saved) = self.saved.take() {
                self.run = saved;
            }
            self.phase = Phase::Finished;
            self.phase_since = get_time();
            self.music.set_paused(true);
            self.grab(false);
        }
    }

    pub fn approach_time(&self) -> f64 {
        (self.settings.approach_distance / self.settings.approach_rate) as f64
    }
    pub fn settings_out(&self) -> Settings {
        Settings {
            sensitivity: self.sensitivity,
            offset_ms: (self.offset * 1000.0) as f32,
            ..self.settings.clone()
        }
    }

    pub fn now(&self) -> f64 {
        self.music.time() - self.offset
    }

    pub fn length(&self) -> f64 {
        self.map.end_time().max(
            self.music
                .duration()
                .unwrap_or(0.0)
                .min(self.map.end_time() + 5.0),
        )
    }

    fn end_time(&self) -> f64 {
        self.map.end_time() + OUTRO
    }

    pub fn update(&mut self) -> Action {
        let t = get_time();
        let dt = t - self.last_tick;
        self.last_tick = t;
        if let Some(action) = self.handle_keys() {
            return action;
        }
        if self.phase == Phase::Playing {
            self.music.update(dt);
            self.aim();
            self.judge();
            self.animate_hud(dt as f32);
            if self.run.first == self.run.states.len() && self.now() > self.end_time() {
                self.finish();
            }
        }
        let now = self.now();
        self.run
            .effects
            .retain(|e| now - e.born < BREAK_LIFE.max(POPUP_LIFE));
        Action::Continue
    }

    fn handle_keys(&mut self) -> Option<Action> {
        if is_key_pressed(KeyCode::LeftBracket) {
            self.offset -= 0.005;
        }
        if is_key_pressed(KeyCode::RightBracket) {
            self.offset += 0.005;
        }
        if is_key_pressed(KeyCode::Minus) {
            self.sensitivity = (self.sensitivity - 0.05).max(0.05);
        }
        if is_key_pressed(KeyCode::Equal) {
            self.sensitivity += 0.05;
        }
        let click = is_mouse_button_pressed(MouseButton::Left);
        match self.phase {
            Phase::Ready | Phase::Finished => {
                if self.phase == Phase::Finished && is_key_pressed(KeyCode::V) {
                    self.watch_replay();
                    return None;
                }
                if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
                    self.music.stop();
                    return Some(Action::Back);
                }
                if click || is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::R) {
                    self.start(self.phase != Phase::Ready);
                }
            }
            Phase::Playing => {
                if self.watching.is_some()
                    && (is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Space))
                {
                    self.end_replay();
                    return None;
                }
                if is_key_pressed(KeyCode::R) {
                    self.start(self.phase != Phase::Ready);
                } else if is_key_pressed(KeyCode::Escape) {
                    self.phase = Phase::Paused;
                    self.phase_since = get_time();
                    self.music.set_paused(true);
                    self.grab(false);
                }
            }
            Phase::Paused => {
                if is_key_pressed(KeyCode::Q) {
                    self.music.stop();
                    return Some(Action::Back);
                }
                if is_key_pressed(KeyCode::R) {
                    self.start(self.phase != Phase::Ready);
                } else if click || is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Space)
                {
                    self.phase = Phase::Playing;
                    self.phase_since = get_time();
                    self.music.set_paused(false);
                    self.grab(true);
                }
            }
        }
        None
    }

    pub fn autostart(&mut self) {
        self.autoplay = true;
        self.start(false);
    }

    fn start(&mut self, retry: bool) {
        self.watching = None;
        self.saved = None;
        let first = self.map.notes.first().map_or(0.0, |n| n.time);
        self.start_at(start_time(first, self.approach_time(), retry), true);
    }

    fn start_at(&mut self, started_at: f64, grab_mouse: bool) {
        self.recorded = false;
        self.new_best = false;
        self.run = Run::new(
            self.map.notes.len(),
            *self.palette.last().expect("palette is not empty"),
        );
        self.run.started_at = started_at;
        self.music.restart(started_at);
        self.music.play_sfx(Sfx::Confirm);
        self.phase = Phase::Playing;
        self.phase_since = get_time();
        if grab_mouse && !self.autoplay {
            self.grab(true);
        }
    }

    fn finish(&mut self) {
        if self.watching.is_some() {
            self.end_replay();
            return;
        }
        self.phase = Phase::Finished;
        self.phase_since = get_time();
        self.music.set_paused(true);
        self.music.play_sfx(if self.run.failed {
            Sfx::Fail
        } else {
            Sfx::Clear
        });
        self.grab(false);
    }

    fn grab(&mut self, on: bool) {
        set_cursor_grab(on);
        show_mouse(!on);
        self.last_mouse = None;
        self.run.prev = None;
    }

    fn aim(&mut self) {
        let mouse = Vec2::from(mouse_position());
        if let Some(replay) = &self.watching {
            self.run.cursor = replay
                .sample_at(self.now())
                .clamp(Vec2::splat(-BOUNDS), Vec2::splat(BOUNDS));
        } else if self.autoplay {
            self.run.cursor = autopilot(&self.map.notes, self.now());
        } else if let Some(last) = self.last_mouse {
            let d = (mouse - last) * self.sensitivity / 120.0;
            self.run.cursor = (self.run.cursor + vec2(d.x, -d.y))
                .clamp(Vec2::splat(-BOUNDS), Vec2::splat(BOUNDS));
        }
        self.last_mouse = Some(mouse);
        let now = self.now();
        if self.watching.is_none() {
            self.run.replay.record(now, self.run.cursor);
        }
        self.run.trail.push_front((now, self.run.cursor));
        while self
            .run
            .trail
            .back()
            .is_some_and(|(t, _)| now - t > TRAIL_TIME)
        {
            self.run.trail.pop_back();
        }
    }

    fn judge(&mut self) {
        let now = self.now();
        for i in self.run.first..self.run.states.len() {
            if self.run.states[i] != NoteState::Pending {
                continue;
            }
            let note = self.map.notes[i];
            if now < note.time {
                break;
            }
            let pos = vec2(note.x, note.y);
            match judge_note(self.run.prev, (now, self.run.cursor), note.time, pos) {
                Verdict::Hit(late) => {
                    let d = (self.run.cursor - pos).abs();
                    let reach = 0.5 + HITBOX_PAD;
                    if d.x > reach || d.y > reach {
                        self.run.sweep_hits += 1;
                    }
                    self.hit(i, late);
                }
                Verdict::Miss => self.miss(i),
                Verdict::Pending => {}
            }
            if self.run.failed {
                return;
            }
        }
        self.run.prev = Some((now, self.run.cursor));
        while self
            .run
            .states
            .get(self.run.first)
            .is_some_and(|s| *s != NoteState::Pending)
        {
            self.run.first += 1;
        }
    }

    fn hit(&mut self, i: usize, late: f64) {
        let late_ms = (late * 1000.0) as f32;
        let factor = 1.0 - (late_ms - 25.0).max(0.0) / 150.0;
        let score = (100.0 * self.run.multiplier as f32 * factor) as u32;
        self.run.lates_ms.push(late_ms);
        self.run.states[i] = NoteState::Hit;
        self.run.hits += 1;
        self.run.combo += 1;
        self.run.max_combo = self.run.max_combo.max(self.run.combo);
        self.run.score += score as u64;
        self.run.multiplier_progress += 1;
        if self.run.multiplier_progress == self.run.multiplier_increment
            && self.run.multiplier < MAX_MULTIPLIER
        {
            self.run.multiplier += 1;
            self.music.play_sfx(Sfx::LevelUp);
            self.run.multiplier_progress = if self.run.multiplier == MAX_MULTIPLIER {
                self.run.multiplier_increment
            } else {
                0
            };
        }
        self.run.health_step = (self.run.health_step / 1.45).max(15.0);
        self.run.health = (self.run.health + self.run.health_step / 1.75).min(100.0);
        self.run.last_hit = Some(i);
        self.run.hud.hits_opacity = 1.0;
        self.music.play_sfx(Sfx::Hit);
        self.effect(i, Some(score));
    }

    fn miss(&mut self, i: usize) {
        self.run.states[i] = NoteState::Missed;
        self.run.misses += 1;
        self.run.combo = 0;
        self.run.multiplier_progress = 0;
        self.run.multiplier = (self.run.multiplier.saturating_sub(1)).max(1);
        self.run.health = (self.run.health - self.run.health_step).max(0.0);
        self.run.health_step = (self.run.health_step * 1.2).min(100.0);
        self.run.hud.misses_opacity = 1.0;
        self.music.play_sfx(Sfx::Miss);
        self.effect(i, None);
        if self.run.health <= 0.0 && !self.settings.no_fail {
            self.run.failed = true;
            self.finish();
        }
    }

    fn effect(&mut self, i: usize, score: Option<u32>) {
        let n = self.map.notes[i];
        self.run.effects.push(Effect {
            x: n.x,
            pos: vec2(n.x, n.y),
            born: self.now(),
            note: i,
            score,
        });
    }

    fn animate_hud(&mut self, dt: f32) {
        let run = &mut self.run;
        let hud = &mut run.hud;
        let target = run.multiplier_progress as f32 / run.multiplier_increment as f32;
        hud.multiplier_progress += (target - hud.multiplier_progress) * (dt * 16.0).min(1.0);
        let color = if run.multiplier == MAX_MULTIPLIER {
            Color::from_rgba(255, 140, 0, 255)
        } else {
            WHITE
        };
        hud.multiplier_color = lerp_color(hud.multiplier_color, color, (dt * 2.0).min(1.0));
        hud.hits_opacity = (hud.hits_opacity - dt * 1.5).max(IDLE_OPACITY);
        hud.misses_opacity = (hud.misses_opacity - dt * 1.5).max(IDLE_OPACITY);
        hud.health += (run.health as f32 / 100.0 - hud.health) * (dt * 60.0).min(1.0);
        let shown = run.score as f32;
        hud.score_shown += (shown - hud.score_shown) * (dt * 14.0).min(1.0);
        if (shown - hud.score_shown).abs() < 0.5 {
            hud.score_shown = shown;
        }
        if run.multiplier > hud.last_multiplier {
            hud.mult_pulse = 1.0;
        }
        hud.last_multiplier = run.multiplier;
        hud.mult_pulse = (hud.mult_pulse - dt * 3.0).max(0.0);
        if let Some(i) = run.last_hit {
            let c = self.palette[i % self.palette.len()];
            hud.space_color = lerp_color(hud.space_color, c, (dt * 8.0).min(1.0));
        }
    }
}

pub fn prism(hue: f32) -> Color {
    let h = hue.rem_euclid(1.0) * 6.0;
    let (v, s, f) = (1.0, 0.68, h.fract());
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match h as u32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color::new(r, g, b, 1.0)
}

pub fn note_hue(i: usize) -> f32 {
    0.93 + i as f32 * 0.037
}

pub fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::from_vec(a.to_vec().lerp(b.to_vec(), t))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE_TIME: f64 = 10.0;
    const FRAME: f64 = 0.007;

    fn path(t: f64) -> Vec2 {
        vec2(((t - NOTE_TIME) * 300.0) as f32, 0.0)
    }

    fn point_hit(t: f64) -> bool {
        let d = path(t).abs();
        d.x <= 0.5 + HITBOX_PAD && d.y <= 0.5 + HITBOX_PAD
    }

    fn run_frames(offset: f64, judge: impl Fn(Option<(f64, Vec2)>, f64) -> Verdict) -> Verdict {
        let mut prev = None;
        let mut t = NOTE_TIME - 0.05 + offset;
        while t < NOTE_TIME + 0.2 {
            match judge(prev, t) {
                Verdict::Pending => {}
                v => return v,
            }
            prev = Some((t, path(t)));
            t += FRAME;
        }
        Verdict::Pending
    }

    #[test]
    fn autopilot_stands_on_every_note_at_its_time() {
        let notes: Vec<crate::map::Note> = (0..20)
            .map(|i| crate::map::Note {
                time: 1.0 + f64::from(i) * 0.137,
                x: ((i * 7) % 3) as f32 - 1.0,
                y: ((i * 5) % 3) as f32 - 1.0,
            })
            .collect();
        for n in &notes {
            let d = (autopilot(&notes, n.time) - vec2(n.x, n.y)).abs();
            assert!(d.x < 1e-5 && d.y < 1e-5);
        }
    }

    #[test]
    fn retry_skips_the_intro_but_a_first_start_does_not() {
        assert_eq!(start_time(10.0, 0.6, true), 8.9);
        assert_eq!(start_time(10.0, 0.6, false), -LEAD_IN);
        assert!((start_time(1.0, 0.6, true) - -0.1).abs() < 1e-9);
        assert!((start_time(0.2, 0.6, true) - -0.9).abs() < 1e-9);
        assert_eq!(start_time(0.2, 3.0, true), -LEAD_IN);
        for first in [0.0, 0.5, 3.0, 12.0, 90.0] {
            let gap = first - start_time(first, 0.6, true);
            assert!(
                gap <= (0.6 + RETRY_RUN_UP).max(first + LEAD_IN) + 1e-9,
                "{first}"
            );
        }
    }

    #[test]
    fn flick_across_note_hits_at_every_frame_phase() {
        let mut point_only_misses = 0;
        for k in 0..70 {
            let offset = f64::from(k) * 0.0001;
            let swept = run_frames(offset, |prev, t| {
                judge_note(prev, (t, path(t)), NOTE_TIME, Vec2::ZERO)
            });
            assert!(matches!(swept, Verdict::Hit(_)), "offset {offset}");
            let mut hit = false;
            let mut t = NOTE_TIME - 0.05 + offset;
            while t < NOTE_TIME + HIT_WINDOW {
                hit |= t >= NOTE_TIME && point_hit(t);
                t += FRAME;
            }
            if !hit {
                point_only_misses += 1;
            }
        }
        assert!(point_only_misses > 0);
    }

    #[test]
    fn path_reaching_note_after_window_is_a_miss() {
        let pos = vec2(0.0, 0.0);
        let late = |t: f64| vec2(((t - NOTE_TIME - 0.08) * 300.0) as f32, 0.0);
        let mut prev = None;
        let mut t = NOTE_TIME - 0.05;
        let mut verdict = Verdict::Pending;
        while t < NOTE_TIME + 0.3 && matches!(verdict, Verdict::Pending) {
            verdict = judge_note(prev, (t, late(t)), NOTE_TIME, pos);
            prev = Some((t, late(t)));
            t += FRAME;
        }
        assert!(matches!(verdict, Verdict::Miss));
    }

    #[test]
    fn hit_time_is_where_the_path_enters() {
        let prev = Some((NOTE_TIME + 0.01, vec2(-2.0, 0.0)));
        let Verdict::Hit(late) = judge_note(
            prev,
            (NOTE_TIME + 0.02, vec2(2.0, 0.0)),
            NOTE_TIME,
            Vec2::ZERO,
        ) else {
            panic!("expected a hit");
        };
        assert!((late - (0.01 + 0.01 * (1.43 / 4.0))).abs() < 1e-4, "{late}");
    }

    #[test]
    fn replay_rejudges_recorded_path_identically() {
        let mut replay = crate::stats::Replay::new();
        let mut samples = Vec::new();
        let mut t = NOTE_TIME - 0.05;
        let mut prev = None;
        let mut original = Verdict::Pending;
        while t < NOTE_TIME + 0.2 && matches!(original, Verdict::Pending) {
            let pos = path(t);
            replay.record(t, pos);
            samples.push((t, pos));
            original = judge_note(prev, (t, pos), NOTE_TIME, Vec2::ZERO);
            prev = Some((t, pos));
            t += FRAME;
        }
        let mut prev = None;
        let mut reproduced = Verdict::Pending;
        for (t, _) in samples {
            let pos = replay.sample_at(t);
            reproduced = judge_note(prev, (t, pos), NOTE_TIME, Vec2::ZERO);
            prev = Some((t, pos));
            if !matches!(reproduced, Verdict::Pending) {
                break;
            }
        }
        match (original, reproduced) {
            (Verdict::Hit(a), Verdict::Hit(b)) => assert!((a - b).abs() < 1e-5),
            (Verdict::Miss, Verdict::Miss) | (Verdict::Pending, Verdict::Pending) => {}
            _ => panic!("replay verdict changed"),
        }
    }

    #[test]
    fn start_time_round_trips_as_the_music_restart_argument() {
        let started_at = start_time(10.0, 0.6, true);
        let mut replay_started_at = started_at;
        assert_eq!(replay_started_at, started_at);
        replay_started_at = start_time(10.0, 0.6, false);
        assert_eq!(replay_started_at, -LEAD_IN);
    }

    #[test]
    fn standing_on_note_before_its_time_hits_at_note_time() {
        let prev = Some((NOTE_TIME - 0.004, Vec2::ZERO));
        let Verdict::Hit(late) =
            judge_note(prev, (NOTE_TIME + 0.003, Vec2::ZERO), NOTE_TIME, Vec2::ZERO)
        else {
            panic!("expected a hit");
        };
        assert!((late - 0.0).abs() < 1e-9, "{late}");
    }
}
