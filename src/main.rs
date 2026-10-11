mod audio;
mod calibrate;
mod chart;
mod demo;
mod game;
mod import;
mod look;
mod map;
mod perf;
mod postfx;
mod priority;
mod render;
#[cfg(test)]
mod robust;
mod scores;
mod settings;
mod sfx;
mod skin;
mod stats;
mod store;
mod wallpaper;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use macroquad::prelude::*;

use crate::{
    audio::Music,
    calibrate::Calibration,
    game::{Action, Game},
    map::{Map, Meta},
    perf::{Pacer, Perf},
    render::{Style, Text},
    scores::{Best, Scores},
    settings::{Exit, Settings, SettingsScreen},
    skin::Skin,
};

const MAPS_DIR: &str = "maps";
const ROW_H: f32 = 44.0;
/// Menus sleep this long before reading input, which drops them from the full refresh rate to about half of it (72 fps on a 144 Hz screen). Songs and calibration never use it.
const MENU_FRAME_DELAY: f64 = 0.0115;

type Stamp = Option<std::time::SystemTime>;
type MetaCache = HashMap<PathBuf, (Stamp, Option<Meta>)>;

static METAS: LazyLock<Mutex<MetaCache>> = LazyLock::new(Mutex::default);

fn stamp(path: &Path) -> Stamp {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn request_meta(paths: &[PathBuf]) {
    let todo: Vec<PathBuf> = {
        let mut metas = METAS.lock().unwrap_or_else(|e| e.into_inner());
        let fresh: Vec<PathBuf> = paths
            .iter()
            .filter(|p| metas.get(*p).is_none_or(|(seen, _)| *seen != stamp(p)))
            .cloned()
            .collect();
        for p in &fresh {
            metas.insert(p.clone(), (stamp(p), None));
        }
        fresh
    };
    if todo.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for path in todo {
            let seen = stamp(&path);
            let meta = Map::load(&path).ok().map(|m| m.meta());
            METAS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(path, (seen, meta));
        }
    });
}

fn meta_of(path: &Path) -> Option<Meta> {
    METAS.try_lock().ok()?.get(path)?.1.clone()
}

fn bench_from_env() -> Option<(bool, f64)> {
    let value = std::env::var("RHYTHM_BENCH").ok()?;
    let (kind, secs) = value.split_once(':')?;
    Some((kind == "menu", secs.parse().ok()?))
}

fn proc_stat_fields() -> Vec<String> {
    let text = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    text.rsplit_once(')')
        .map(|(_, rest)| rest.split_whitespace().map(String::from).collect())
        .unwrap_or_default()
}

fn realtime_threads() -> usize {
    std::fs::read_dir("/proc/self/task")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|t| std::fs::read_to_string(t.path().join("stat")).ok())
        .filter(|stat| {
            let policy = stat
                .rsplit_once(')')
                .and_then(|(_, rest)| rest.split_whitespace().nth(38)?.parse::<u32>().ok());
            matches!(policy, Some(1 | 2))
        })
        .count()
}

fn proc_cpu_seconds() -> f64 {
    let fields = proc_stat_fields();
    let tick = |i: usize| {
        fields
            .get(i)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (tick(11) + tick(12)) / 100.0
}

fn process_age_ms() -> f64 {
    let uptime: f64 = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|t| t.split_whitespace().next().and_then(|v| v.parse().ok()))
        .unwrap_or(0.0);
    let start = proc_stat_fields()
        .get(19)
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
        / 100.0;
    (uptime - start) * 1000.0
}

fn proc_status_mb(key: &str) -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix(key))
        .and_then(|v| {
            v.trim_start_matches(':')
                .split_whitespace()
                .next()?
                .parse::<f64>()
                .ok()
        })
        .unwrap_or(0.0)
        / 1024.0
}

fn clock(seconds: f64) -> String {
    let s = seconds.round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn window_conf() -> Conf {
    let saved = Settings::load();
    if saved.short_gl_queue {
        // SAFETY: runs before the window thread or audio threads exist.
        unsafe { std::env::set_var("__GL_MaxFramesAllowed", "1") };
    }
    Conf {
        window_title: std::env::var("RHYTHM_TITLE").unwrap_or_else(|_| "rhythia is trash".into()),
        window_width: 1280,
        window_height: 720,
        sample_count: 1,
        fullscreen: saved.fullscreen,
        window_resizable: true,
        platform: macroquad::miniquad::conf::Platform {
            swap_interval: std::env::var_os("RHYTHM_VSYNC")
                .is_some_and(|v| v == "0")
                .then_some(0),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn palette() -> Vec<Color> {
    look::active_palette()
}

/// Imports run on their own thread so dropping many files one after another never freezes the window.
struct Importer {
    jobs: std::sync::mpsc::Sender<Vec<PathBuf>>,
    done: std::sync::mpsc::Receiver<import::Report>,
    pending: usize,
}

impl Importer {
    fn new() -> Self {
        let (jobs, queue) = std::sync::mpsc::channel::<Vec<PathBuf>>();
        let (finished, done) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for paths in queue {
                let report = import::import(Path::new(MAPS_DIR), &paths);
                if finished.send(report).is_err() {
                    break;
                }
            }
        });
        Self {
            jobs,
            done,
            pending: 0,
        }
    }

    fn submit(&mut self, paths: Vec<PathBuf>) {
        if self.jobs.send(paths).is_ok() {
            self.pending += 1;
        }
    }

    fn poll(&mut self) -> Vec<import::Report> {
        let reports: Vec<import::Report> = self.done.try_iter().collect();
        self.pending = self.pending.saturating_sub(reports.len());
        reports
    }
}

#[derive(Clone)]
enum Entry {
    Demo,
    File(PathBuf),
}

impl Entry {
    fn label(&self) -> String {
        match self {
            Entry::Demo => "Demo - 128 BPM (built in)".into(),
            Entry::File(p) => p
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        }
    }

    fn load(&self, settings: &Settings, scores: &Scores) -> Result<Game, String> {
        let started = std::time::Instant::now();
        let ms = |t: std::time::Instant| t.elapsed().as_secs_f32() * 1000.0;
        let mut parse_ms = 0.0;
        let (map, track) = match self {
            Entry::Demo => {
                let d = demo::build();
                (d.map, Some(audio::from_mono(&d.samples, demo::SAMPLE_RATE)))
            }
            Entry::File(path) => {
                let mut map = Map::load(path)?;
                parse_ms = ms(started);
                let track = map.audio.take().map(audio::decode).transpose()?;
                if track.is_none() {
                    eprintln!("{}: no audio found, playing silent", path.display());
                }
                (map, track)
            }
        };
        let decode_ms = ms(started) - parse_ms;
        let music = Music::new(
            track,
            settings.music_volume,
            settings.sfx_volume,
            settings.audio_buffer as u32,
        );
        let mut game = Game::new(map, music, palette(), settings.clone());
        game.key = self.label();
        game.best = scores.best(&game.key);
        game.load_ms = [parse_ms, decode_ms, ms(started) - parse_ms - decode_ms];
        Ok(game)
    }
}

enum Pick {
    Quit,
    Play(Entry),
    Settings,
}

struct Select {
    entries: Vec<Entry>,
    selected: usize,
    scroll: f32,
    error: Option<String>,
    notice: Option<String>,
    entered: f64,
    highlight: f32,
    last_mouse: Vec2,
    removed: Vec<import::Removed>,
}

impl Select {
    fn new() -> Self {
        let _ = std::fs::create_dir_all(MAPS_DIR);
        let found = map::scan(Path::new(MAPS_DIR));
        request_meta(&found);
        let mut entries = vec![Entry::Demo];
        entries.extend(found.into_iter().map(Entry::File));
        Self {
            entries,
            selected: 0,
            scroll: 0.0,
            error: None,
            notice: None,
            entered: get_time(),
            highlight: 0.0,
            last_mouse: Vec2::from(mouse_position()),
            removed: Vec::new(),
        }
    }
    fn rescan(&mut self, focus: Option<&Path>) {
        let found = map::scan(Path::new(MAPS_DIR));
        request_meta(&found);
        self.entries = vec![Entry::Demo];
        self.entries.extend(found.into_iter().map(Entry::File));
        if let Some(i) = focus.and_then(|f| {
            self.entries
                .iter()
                .position(|e| matches!(e, Entry::File(p) if p == f))
        }) {
            self.selected = i;
        }
    }

    fn apply(&mut self, report: &import::Report) {
        self.rescan(report.added.last().map(PathBuf::as_path));
        self.error = report.error();
        self.notice = if self.error.is_none() {
            report.notice()
        } else {
            None
        };
    }

    fn remove_selected(&mut self) {
        let Some(Entry::File(path)) = self.entries.get(self.selected).cloned() else {
            self.error = Some("the built-in demo can't be removed".into());
            return;
        };
        match import::remove(Path::new(MAPS_DIR), &path) {
            Ok(gone) => {
                self.notice = Some(format!(
                    "removed {}   Z undo, or drag it back in",
                    gone.label
                ));
                self.error = None;
                self.removed.push(gone);
                self.rescan(None);
                self.selected = self.selected.min(self.entries.len() - 1);
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn undo_remove(&mut self) {
        let Some(gone) = self.removed.pop() else {
            return;
        };
        match import::restore(&gone) {
            Ok(path) => {
                self.rescan(Some(&path));
                self.notice = Some(format!("restored {}", gone.label));
                self.error = None;
            }
            Err(e) => {
                self.removed.push(gone);
                self.error = Some(e);
            }
        }
    }

    fn update(&mut self) -> Option<Pick> {
        if is_key_pressed(KeyCode::Delete) {
            self.remove_selected();
        }
        if is_key_pressed(KeyCode::Z) {
            self.undo_remove();
        }
        let n = self.entries.len();
        if is_key_pressed(KeyCode::Escape) {
            return Some(Pick::Quit);
        }
        if is_key_pressed(KeyCode::Tab) {
            return Some(Pick::Settings);
        }
        if is_key_pressed(KeyCode::Down) {
            self.selected = (self.selected + 1) % n;
        }
        if is_key_pressed(KeyCode::Up) {
            self.selected = (self.selected + n - 1) % n;
        }
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 {
            self.selected =
                (self.selected as i32 - wheel.signum() as i32).clamp(0, n as i32 - 1) as usize;
        }
        let mouse = Vec2::from(mouse_position());
        let right = screen_width() - 48.0;
        if is_mouse_button_pressed(MouseButton::Left)
            && mouse.x >= right - 150.0
            && mouse.x <= right
            && (52.0..=104.0).contains(&mouse.y)
        {
            return Some(Pick::Settings);
        }
        let bottom = screen_height() - 86.0;
        let inside = mouse.x >= 48.0
            && mouse.x <= screen_width() - 48.0
            && mouse.y >= self.top()
            && mouse.y < bottom;
        if inside {
            let row = ((mouse.y - self.top() + self.scroll) / ROW_H).floor() as usize;
            if row < n {
                if mouse != self.last_mouse {
                    self.selected = row;
                }
                if is_mouse_button_pressed(MouseButton::Left) {
                    self.selected = row;
                    self.last_mouse = mouse;
                    return Some(Pick::Play(self.entries[row].clone()));
                }
            }
        }
        self.last_mouse = mouse;
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
            return Some(Pick::Play(self.entries[self.selected].clone()));
        }
        let room = (bottom - self.top()).max(ROW_H);
        let target = (self.selected as f32 * ROW_H - room * 0.45)
            .clamp(0.0, (n as f32 * ROW_H - room).max(0.0));
        let ease = 1.0 - (-get_frame_time() * 16.0).exp();
        self.scroll += (target - self.scroll) * ease;
        self.highlight += (self.selected as f32 - self.highlight) * ease;
        None
    }

    fn top(&self) -> f32 {
        screen_height() * 0.24
    }

    fn draw(
        &self,
        skin: &Skin,
        fx: &mut postfx::PostFx,
        settings: &Settings,
        scores: &Scores,
        status: Option<&str>,
    ) {
        let cam = fx.begin(&render::camera(Vec2::ZERO, settings));
        render::draw_space(
            skin,
            &cam,
            palette()[1],
            settings.space,
            settings.approach_rate,
        );
        gl_use_default_material();
        fx.present();
        let text = Text::screen(skin);
        let w = text.screen.x;
        let intro = (1.0 - (-((get_time() - self.entered) as f32) * 8.0).exp()).clamp(0.0, 1.0);
        let tint = |alpha| Color::new(1.0, 1.0, 1.0, alpha * intro);
        let slide = (1.0 - intro) * 12.0;
        text.draw(
            "rhythia is trash",
            vec2(64.0 + slide, 86.0),
            Style::new(46.0, tint(1.0)).bold(true),
        );
        text.draw(
            "FREE FOREVER   AIM / FLOW / REPEAT",
            vec2(66.0 + slide, 118.0),
            Style::new(13.0, tint(0.45)),
        );
        let count = format!(
            "{} SONG{}",
            self.entries.len(),
            if self.entries.len() == 1 { "" } else { "S" }
        );
        let right_aligned = |size: f32, alpha: f32| Style {
            align: render::Align::Right,
            ..Style::new(size, tint(alpha))
        };
        text.draw(
            "SETTINGS  TAB",
            vec2(w - 48.0, 74.0),
            right_aligned(14.0, 0.7).bold(true),
        );
        text.draw(&count, vec2(w - 48.0, 98.0), right_aligned(13.0, 0.45));
        let bottom = screen_height() - 86.0;
        let y = self.top() + self.highlight * ROW_H - self.scroll;
        let sy = y.max(self.top());
        let ey = (y + ROW_H - 6.0).min(bottom);
        if ey > sy {
            draw_rectangle(
                48.0,
                sy,
                w - 96.0,
                ey - sy,
                Color::new(1.0, 0.2, 0.45, 0.07 * intro),
            );
            draw_rectangle(48.0, sy, 3.0, ey - sy, Color::new(1.0, 0.1, 0.4, intro));
        }
        for (i, e) in self.entries.iter().enumerate() {
            let y = self.top() + i as f32 * ROW_H - self.scroll;
            if y < self.top() || y + ROW_H > bottom {
                continue;
            }
            let on = i == self.selected;
            let color = tint(if on { 1.0 } else { 0.5 });
            let center = y + (ROW_H - 6.0) / 2.0;
            text.draw(
                &format!("{:02}", i + 1),
                vec2(68.0, center),
                Style::new(12.0, tint(0.3)).middle(),
            );
            let meta = match e {
                Entry::File(p) => meta_of(p),
                Entry::Demo => None,
            };
            let name = meta
                .as_ref()
                .map(|m| m.title.clone())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| e.label());
            if let Some(meta) = &meta {
                let mut detail = format!("{} notes   {}", meta.notes, clock(meta.length));
                if !meta.mappers.is_empty() {
                    detail = format!("{detail}   {}", meta.mappers);
                }
                text.draw(
                    &text.fit(&detail, 13.0, 300.0),
                    vec2(w - 200.0, center),
                    Style {
                        align: render::Align::Right,
                        ..Style::new(13.0, tint(if on { 0.6 } else { 0.3 }))
                    }
                    .middle(),
                );
            }
            let label = text.fit(&name, 20.0, w - 560.0);
            text.draw(
                &label,
                vec2(104.0 + if on { 4.0 } else { 0.0 }, center),
                Style::new(20.0, color).bold(on).middle(),
            );
            if let Some(Best { accuracy, .. }) = scores.best(&e.label()) {
                text.draw(
                    &format!("{accuracy:.2}%  {}", game::grade_for(accuracy)),
                    vec2(w - 100.0, center),
                    Style {
                        align: render::Align::Right,
                        ..Style::new(14.0, tint(if on { 0.8 } else { 0.4 }))
                    }
                    .middle(),
                );
            }
            if on {
                let x = w - 78.0;
                draw_triangle(
                    vec2(x - 4.0, center - 5.0),
                    vec2(x - 4.0, center + 5.0),
                    vec2(x + 4.0, center),
                    color,
                );
            }
        }
        let footer = format!(
            "Enter play   Tab settings   Del remove   Esc quit   F2 glow {}   F3 space {}",
            if fx.enabled { "ON" } else { "OFF" },
            settings.space.name()
        );
        text.draw(
            &footer,
            vec2(64.0, screen_height() - 56.0),
            Style::new(14.0, tint(0.55)),
        );
        let message = status
            .or(self.error.as_deref())
            .or(self.notice.as_deref())
            .unwrap_or("Drag maps (.sspm .phxm .rhm .txt) or songs (.mp3 .ogg .wav .flac) onto this window");
        let c = if self.error.is_some() && status.is_none() {
            Color::new(1.0, 0.45, 0.5, intro)
        } else if self.notice.is_some() && status.is_none() {
            Color::new(0.55, 1.0, 0.7, intro)
        } else {
            tint(0.3)
        };
        text.draw(
            &text.fit(message, 13.0, w - 128.0),
            vec2(64.0, screen_height() - 28.0),
            Style::new(13.0, c),
        );
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    priority::raise_main_thread();
    let window_ms = process_age_ms();
    let bench = bench_from_env();
    let skin = Skin::load();
    let mut fx = postfx::PostFx::new();
    let mut settings = Settings::load();
    look::set_active(settings.look);
    let mut perf = Perf::new();
    let mut scores = Scores::load();
    let mut pacer = Pacer::new();
    let report = std::env::var_os("RHYTHM_PERF").is_some();
    if report {
        use macroquad::miniquad::gl;
        for (name, id) in [
            ("vendor", gl::GL_VENDOR),
            ("renderer", 0x1F01),
            ("version", gl::GL_VERSION),
        ] {
            // SAFETY: the GL context exists once main runs; glGetString returns a static C string.
            let value = unsafe { std::ffi::CStr::from_ptr(gl::glGetString(id).cast()) };
            eprintln!("gl {name}: {}", value.to_string_lossy());
        }
    }
    let mut select = Select::new();
    let mut importer = Importer::new();
    let mut config: Option<SettingsScreen> = None;
    let mut calibration: Option<Calibration> = None;
    let mut shown_fullscreen = settings.fullscreen;
    let mut was_playing = false;
    let mut shown_bypass = false;
    let mut game = match std::env::args_os().nth(1) {
        Some(path) => match Entry::File(PathBuf::from(path)).load(&settings, &scores) {
            Ok(g) => Some(g),
            Err(e) => {
                select.error = Some(e);
                None
            }
        },
        None => None,
    };
    let first_frame_ms = process_age_ms();
    let mut bench_run: Option<(std::time::Instant, f64)> = None;
    let mut play_frames = 0u32;
    let booted = std::time::Instant::now();
    let mut play_since: Option<std::time::Instant> = None;
    let mut frames_seen = 0u32;
    loop {
        let frame_ms = perf.tick();
        frames_seen += 1;
        if let (Some((_, secs)), Some((began, cpu0))) = (bench, bench_run) {
            let wall = began.elapsed().as_secs_f64();
            if wall >= secs {
                let clock = game
                    .as_ref()
                    .map(|g| g.music.clock_stats())
                    .unwrap_or_default();
                let mut v = perf.json(clock);
                v["cpu_pct"] = ((proc_cpu_seconds() - cpu0) / wall * 100.0).into();
                v["rss_mb"] = proc_status_mb("VmRSS").into();
                v["hwm_mb"] = proc_status_mb("VmHWM").into();
                v["window_ms"] = window_ms.into();
                v["first_frame_ms"] = first_frame_ms.into();
                v["pacer_ms"] = pacer.delay_ms().into();
                v["pacer_misses"] = pacer.misses.into();
                v["wall_s"] = wall.into();
                v["nice"] = proc_stat_fields()
                    .get(16)
                    .and_then(|n| n.parse::<i64>().ok())
                    .into();
                v["realtime_threads"] = realtime_threads().into();
                if let Some(g) = &game {
                    v["load_ms"] = serde_json::json!({
                        "parse": g.load_ms[0], "decode": g.load_ms[1], "music": g.load_ms[2],
                    });
                    v["sweep_hits"] = g.run.sweep_hits.into();
                    v["hits"] = g.run.hits.into();
                    v["misses"] = g.run.misses.into();
                }
                println!("BENCH {v}");
                break;
            }
        }
        let playing = config.is_none()
            && calibration.is_none()
            && game
                .as_ref()
                .is_some_and(|g| g.phase == game::Phase::Playing);
        if playing && !was_playing {
            perf.reset();
        }
        if let Some((menu, _)) = bench
            && bench_run.is_none()
        {
            play_frames += u32::from(playing);
            if playing && play_since.is_none() {
                play_since = Some(std::time::Instant::now());
            }
            let settled = |since: std::time::Instant| since.elapsed().as_secs_f64() > 2.5;
            if (menu && frames_seen > 120 && settled(booted))
                || (!menu && play_frames > 180 && play_since.is_some_and(settled))
            {
                perf.reset();
                bench_run = Some((std::time::Instant::now(), proc_cpu_seconds()));
            }
            if !menu
                && let Some(g) = &mut game
                && g.phase == game::Phase::Ready
            {
                g.autostart();
            }
        }
        was_playing = playing;
        pacer.update(
            playing && settings.low_latency,
            frame_ms,
            perf.stats().p50_ms,
        );
        let delay = if game.is_none() && calibration.is_none() {
            MENU_FRAME_DELAY
        } else {
            f64::from(pacer.delay_ms()) / 1000.0
        };
        macroquad::miniquad::window::set_frame_delay(delay);
        if is_key_pressed(KeyCode::F3) {
            settings.space = settings.space.next();
        }
        if is_key_pressed(KeyCode::F11) {
            settings.fullscreen = !settings.fullscreen;
        }
        let want_bypass = settings.bypass_compositor && game.is_some() && calibration.is_none();
        if want_bypass != shown_bypass {
            macroquad::miniquad::window::set_bypass_compositor(want_bypass);
            shown_bypass = want_bypass;
        }
        if settings.fullscreen != shown_fullscreen {
            set_fullscreen(settings.fullscreen);
            shown_fullscreen = settings.fullscreen;
        }
        if is_key_pressed(KeyCode::F2) {
            settings.glow = !settings.glow;
        }
        fx.enabled = settings.glow;
        look::set_active(settings.look);
        wallpaper::sync(&settings.wallpaper, settings.wallpaper_dim);
        if let Some(c) = &mut calibration {
            if c.update(&mut settings) {
                settings.save();
                calibration = None;
                config = Some(SettingsScreen::new());
                next_frame().await;
                continue;
            }
            c.draw(&skin, &mut fx, &settings);
        } else if let Some(screen) = &mut config {
            match screen.update(&mut settings) {
                Exit::Stay => screen.draw(&skin, &mut fx, &settings),
                Exit::Back => {
                    settings.save();
                    config = None;
                    select.entered = get_time();
                    next_frame().await;
                    continue;
                }
                Exit::Calibrate => {
                    settings.save();
                    config = None;
                    calibration = Some(Calibration::new(&settings));
                    next_frame().await;
                    continue;
                }
            }
        } else if let Some(g) = &mut game {
            let started = std::time::Instant::now();
            let step = g.update();
            if g.phase == game::Phase::Finished && !g.recorded {
                g.recorded = true;
                if !g.run.failed {
                    g.new_best = scores.record(
                        &g.key,
                        Best {
                            score: g.run.score,
                            accuracy: g.run.accuracy(),
                            max_combo: g.run.max_combo,
                        },
                    );
                    if g.new_best {
                        scores.save();
                    }
                }
                g.best = scores.best(&g.key);
            }
            let update_ms = started.elapsed().as_secs_f32() * 1000.0;
            perf.update(update_ms);
            match step {
                Action::Continue => {
                    g.space = settings.space;
                    g.stats = perf.stats();
                    g.stats.delay_ms = pacer.delay_ms();
                    g.draw(&skin, &mut fx);
                    perf.cpu(started);
                    let cpu_ms = started.elapsed().as_secs_f32() * 1000.0;
                    if report && (frame_ms > 12.0 || cpu_ms > 4.0) {
                        eprintln!(
                            "spike: frame {frame_ms:.1} ms, cpu {cpu_ms:.1} ms (update {update_ms:.1}, draw {:.1}) at song time {:.2}s",
                            cpu_ms - update_ms,
                            g.now()
                        );
                    }
                }
                Action::Back => {
                    if report {
                        eprintln!(
                            "{} | pacing reached {:.2} ms, cap {}, misses {}, low_latency {}, short_gl_queue {}, bypass {} | sweep-only hits {}",
                            perf.report(g.music.clock_stats()),
                            pacer.reached_ms,
                            pacer
                                .cap_ms()
                                .map_or_else(|| "none".into(), |c| format!("{c:.2} ms")),
                            pacer.misses,
                            settings.low_latency,
                            settings.short_gl_queue,
                            shown_bypass,
                            g.run.sweep_hits
                        );
                    }
                    perf.reset();
                    let out = g.settings_out();
                    settings.sensitivity = out.sensitivity;
                    settings.offset_ms = out.offset_ms;
                    settings.save();
                    game = None;
                    select = Select {
                        error: None,
                        ..Select::new()
                    };
                    next_frame().await;
                    continue;
                }
            }
        } else {
            let dropped: Vec<PathBuf> = get_dropped_files()
                .into_iter()
                .filter_map(|f| f.path)
                .collect();
            let (pictures, rest): (Vec<PathBuf>, Vec<PathBuf>) =
                dropped.into_iter().partition(|p| wallpaper::is_image(p));
            for picture in &pictures {
                match wallpaper::import(picture) {
                    Ok(name) => {
                        settings.wallpaper = name.clone();
                        settings.save();
                        select.notice =
                            Some(format!("wallpaper set to {name} (Tab, Look to change)"));
                        select.error = None;
                    }
                    Err(e) => select.error = Some(e),
                }
            }
            if !rest.is_empty() {
                importer.submit(rest);
            }
            for report in importer.poll() {
                select.apply(&report);
            }
            match select.update() {
                Some(Pick::Quit) => break,
                Some(Pick::Settings) => {
                    config = Some(SettingsScreen::new());
                    next_frame().await;
                    continue;
                }
                Some(Pick::Play(entry)) => {
                    select.draw(&skin, &mut fx, &settings, &scores, Some("loading..."));
                    next_frame().await;
                    match entry.load(&settings, &scores) {
                        Ok(g) => game = Some(g),
                        Err(e) => select.error = Some(e),
                    }
                    next_frame().await;
                    continue;
                }
                None => {
                    let status = (importer.pending > 0).then_some("importing songs...");
                    select.draw(&skin, &mut fx, &settings, &scores, status);
                }
            }
        }
        if is_key_pressed(KeyCode::F12) {
            if let Err(e) = std::fs::create_dir_all("screenshots") {
                eprintln!("screenshot: {e}");
            } else {
                let time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                get_screen_data().export_png(&format!("screenshots/{time}.png"));
            }
        }
        let waited = std::time::Instant::now();
        next_frame().await;
        perf.swap(waited.elapsed().as_secs_f32() * 1000.0, pacer.delay_ms());
    }
    settings.save();
}
