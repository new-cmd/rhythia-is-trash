use macroquad::prelude::*;

use crate::{
    audio::{Music, Sfx},
    postfx::PostFx,
    render::{self, Align, Style, Text},
    settings::Settings,
    skin::Skin,
};

const BEAT: f64 = 0.5;
const FLASH_LIFE: f64 = 0.16;
const LEAD: f64 = 0.6;
const REPEAT_DELAY: f32 = 0.35;
const REPEAT_RATE: f32 = 0.05;

pub struct Calibration {
    music: Music,
    started: f64,
    entered: f64,
    beat: u32,
    clicked: bool,
    flashed: bool,
    flash_at: f64,
    held: f32,
}

impl Calibration {
    pub fn new(s: &Settings) -> Self {
        let now = get_time();
        Self {
            music: Music::new(None, 1.0, s.sfx_volume.max(0.3), s.audio_buffer as u32),
            started: now + LEAD,
            entered: now,
            beat: 0,
            clicked: false,
            flashed: false,
            flash_at: f64::MIN,
            held: 0.0,
        }
    }

    pub fn update(&mut self, s: &mut Settings) -> bool {
        if is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::Tab)
        {
            return true;
        }
        if is_key_pressed(KeyCode::Backspace) {
            s.zero_offset();
        }
        let dir = i32::from(is_key_down(KeyCode::Right)) - i32::from(is_key_down(KeyCode::Left));
        if dir == 0 {
            self.held = 0.0;
        } else {
            let pressed = is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::Left);
            let mut fire = pressed;
            if pressed {
                self.held = 0.0;
            } else {
                self.held += get_frame_time();
                if self.held >= REPEAT_DELAY {
                    self.held -= REPEAT_RATE;
                    fire = true;
                }
            }
            if fire {
                let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
                s.adjust_offset(dir as f32 * if shift { 5.0 } else { 1.0 });
            }
        }
        let now = get_time() - self.started;
        if now < 0.0 {
            return false;
        }
        let offset = f64::from(s.offset_ms) / 1000.0;
        let base = f64::from(self.beat) * BEAT;
        if !self.clicked && now >= base + (-offset).max(0.0) {
            self.music.play_sfx(Sfx::Hit);
            self.clicked = true;
        }
        if !self.flashed && now >= base + offset.max(0.0) {
            self.flash_at = get_time();
            self.flashed = true;
        }
        if self.clicked && self.flashed && now >= base + BEAT {
            self.beat += 1;
            self.clicked = false;
            self.flashed = false;
        }
        false
    }

    pub fn draw(&self, skin: &Skin, fx: &mut PostFx, s: &Settings) {
        let cam = fx.begin(&render::camera(Vec2::ZERO, s));
        render::draw_space(skin, &cam, crate::palette()[1], s.space, s.approach_rate);
        gl_use_default_material();
        fx.present();
        let text = Text::screen(skin);
        let (w, h) = (text.screen.x, text.screen.y);
        let intro = (1.0 - (-((get_time() - self.entered) as f32) * 8.0).exp()).clamp(0.0, 1.0);
        let tint = |a: f32| Color::new(1.0, 1.0, 1.0, a * intro);
        text.draw(
            "calibrate",
            vec2(64.0, 86.0),
            Style::new(46.0, tint(1.0)).bold(true),
        );
        text.draw(
            "make the flash and the click land at the same moment",
            vec2(66.0, 118.0),
            Style::new(13.0, tint(0.45)),
        );
        let glow = (1.0 - (get_time() - self.flash_at) / FLASH_LIFE).clamp(0.0, 1.0) as f32;
        let size = h * 0.2 * (1.0 + 0.08 * glow);
        let center = vec2(w / 2.0, h * 0.42);
        let pink = crate::palette()[0];
        draw_rectangle(
            center.x - size / 2.0,
            center.y - size / 2.0,
            size,
            size,
            Color::new(pink.r, pink.g, pink.b, (0.08 + 0.85 * glow) * intro),
        );
        draw_rectangle_lines(
            center.x - size / 2.0,
            center.y - size / 2.0,
            size,
            size,
            4.0,
            tint(0.35 + 0.65 * glow),
        );
        let value = Style {
            align: Align::Center,
            ..Style::new(44.0, tint(1.0))
        };
        text.draw(
            &format!("{:+.0} ms", s.offset_ms),
            vec2(w / 2.0, h * 0.72),
            value.bold(true),
        );
        let hint = Style {
            align: Align::Center,
            ..Style::new(16.0, tint(0.6))
        };
        text.draw(
            "flash first: press Right      click first: press Left",
            vec2(w / 2.0, h * 0.79),
            hint,
        );
        text.draw(
            "Left/Right adjust (Shift x5)   Backspace zero   Enter or Esc save and back",
            vec2(64.0, h - 40.0),
            Style::new(14.0, tint(0.55)),
        );
    }
}
