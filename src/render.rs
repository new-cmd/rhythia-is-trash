use std::f32::consts::TAU;

use macroquad::prelude::*;

use crate::{
    game::{
        BREAK_LIFE, CURSOR_SIZE, Game, HIT_WINDOW, NoteState, POPUP_LIFE, Phase, TRAIL_TIME,
        lerp_color, note_hue, prism,
    },
    postfx::PostFx,
    settings::Settings,
    skin::{NOTE_GLOW_SCALE, SQUIRCLE_DEPTH, Skin},
};

const CAM_Z: f32 = 3.75;
const FADE_IN: f32 = 0.1;
const OPACITY_EXPONENT: f32 = 1.25;
const NOTE_GLOW: f32 = 0.3;
const FOG_BEGIN: f32 = 10.0;
const FOG_END: f32 = 60.0;
const FOG_CURVE: f32 = 0.15;
const SPACE_HEIGHT: f32 = 10.0;
const SPACE_HALF: f32 = 60.0;
const SPACE_TILES: f32 = 34.0;
const SPACE_NEAR_Z: f32 = -8.0;
const SPACE_SCROLL: f32 = 3.0;
const TILE_BASE: f32 = 42.0 / 255.0;
const TILE_LINE: f32 = 211.0 / 255.0;
const TILE_LINE_WIDTH: f32 = 2.0 / 128.0;
const GRID_EDGE: f32 = 392.0 / 256.0;
const GRID_THICK: f32 = 8.0 / 256.0;
const GRID_ARM: f32 = 130.0 / 256.0;
const PANEL_W: f32 = 4.0;
const PANEL_H: f32 = 10.0;
const PANEL_PX: Vec2 = vec2(250.0, 625.0);
const BAR_HALF_W: f32 = 5.12;
const BAR_HALF_H: f32 = 0.08;
const RING_GRAY: Color = Color::new(0.155, 0.155, 0.155, 1.0);
const HEALTH: Color = Color::new(1.0, 216.0 / 255.0, 230.0 / 255.0, 1.0);
const HEALTH_LOW: Color = Color::new(1.0, 59.0 / 255.0, 107.0 / 255.0, 1.0);
const GRADE_S: Color = Color::new(1.0, 230.0 / 255.0, 168.0 / 255.0, 1.0);
const GRADE_A: Color = Color::new(124.0 / 255.0, 242.0 / 255.0, 196.0 / 255.0, 1.0);
const GRADE_B: Color = Color::new(143.0 / 255.0, 184.0 / 255.0, 1.0, 1.0);
const PROGRESS_BG: Color = Color::new(40.0 / 255.0, 40.0 / 255.0, 40.0 / 255.0, 1.0);
const HITS_ICON: Color = Color::new(160.0 / 255.0, 1.0, 160.0 / 255.0, 1.0);
const MISS: Color = Color::new(1.0, 160.0 / 255.0, 160.0 / 255.0, 1.0);
const MISS_ICON: Color = Color::new(1.0, 0.0, 0.0, 1.0);
const HIT_FLASH_LIFE: f32 = 0.09;
const BATCH_QUADS: usize = 800;

fn fade(c: Color, a: f32) -> Color {
    Color { a: c.a * a, ..c }
}

fn scale(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k, c.a)
}

fn tint_to(to: Color, k: f32) -> Color {
    Color::new(
        1.0 + (to.r - 1.0) * k,
        1.0 + (to.g - 1.0) * k,
        1.0 + (to.b - 1.0) * k,
        1.0,
    )
}

fn hash(a: usize, b: usize) -> f32 {
    let mut x = (a as u32).wrapping_mul(0x9e37_79b9) ^ (b as u32).wrapping_mul(0x85eb_ca6b);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x & 0xffff) as f32 / 65535.0
}

struct Batch {
    texture: Texture2D,
    vertices: Vec<Vertex>,
    indices: Vec<u16>,
}

impl Batch {
    fn new(texture: &Texture2D) -> Self {
        Self {
            texture: texture.clone(),
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    fn quad(&mut self, p: [Vec3; 4], c: [Color; 4]) {
        if self.vertices.len() >= BATCH_QUADS * 4 {
            self.flush();
        }
        let base = self.vertices.len() as u16;
        let uv = [
            vec2(0.0, 1.0),
            vec2(1.0, 1.0),
            vec2(1.0, 0.0),
            vec2(0.0, 0.0),
        ];
        self.vertices
            .extend((0..4).map(|i| Vertex::new2(p[i], uv[i], c[i])));
        self.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }

    fn solid(&mut self, p: [Vec3; 4], c: Color) {
        self.quad(p, [c; 4]);
    }

    fn flush(&mut self) {
        if self.vertices.is_empty() {
            return;
        }
        draw_mesh(&Mesh {
            vertices: std::mem::take(&mut self.vertices),
            indices: std::mem::take(&mut self.indices),
            texture: Some(self.texture.clone()),
        });
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        self.flush();
    }
}

fn sprite(batch: &mut Batch, center: Vec3, size: Vec2, rot: f32, color: Color) {
    let (s, c) = rot.sin_cos();
    let ax = vec3(c, s, 0.0) * size.x / 2.0;
    let ay = vec3(-s, c, 0.0) * size.y / 2.0;
    batch.solid(
        [
            center - ax - ay,
            center + ax - ay,
            center + ax + ay,
            center - ax + ay,
        ],
        color,
    );
}

#[derive(Clone, Copy)]
struct Frame3 {
    origin: Vec3,
    ax: Vec3,
    ay: Vec3,
}

impl Frame3 {
    fn at(&self, p: Vec2) -> Vec3 {
        self.origin + self.ax * p.x + self.ay * p.y
    }

    fn rect(&self, batch: &mut Batch, min: Vec2, max: Vec2, color: Color) {
        let p = [
            vec2(min.x, min.y),
            vec2(max.x, min.y),
            vec2(max.x, max.y),
            vec2(min.x, max.y),
        ];
        batch.solid(p.map(|q| self.at(q)), color);
    }

    fn line(&self, batch: &mut Batch, a: Vec2, b: Vec2, width: f32, color: Color) {
        let n = (b - a).perp().normalize_or_zero() * width / 2.0;
        batch.solid([a - n, b - n, b + n, a + n].map(|q| self.at(q)), color);
    }
}

fn panel(side: f32) -> Frame3 {
    let a = 10f32.to_radians();
    Frame3 {
        origin: vec3(7.5 * side, 0.0, -9.0),
        ax: vec3(a.cos(), 0.0, side * a.sin()),
        ay: Vec3::Y,
    }
}

fn panel_px(x: f32, y: f32) -> Vec2 {
    vec2(
        (x / PANEL_PX.x - 0.5) * PANEL_W,
        (0.5 - y / PANEL_PX.y) * PANEL_H,
    )
}

fn tilted_bar(y: f32, toward: f32) -> Frame3 {
    let a = 15f32.to_radians();
    Frame3 {
        origin: vec3(0.0, y, -10.0),
        ax: Vec3::X,
        ay: vec3(0.0, a.cos(), toward * a.sin()),
    }
}

fn fog_amount(d: f32) -> f32 {
    ((d - FOG_BEGIN) / (FOG_END - FOG_BEGIN))
        .clamp(0.0, 1.0)
        .powf(FOG_CURVE)
}

pub fn quantize(px: f32) -> f32 {
    if px < 24.0 {
        (px / 2.0).round() * 2.0
    } else {
        (px / 4.0).round() * 4.0
    }
}

fn format_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn clock(t: f64) -> String {
    let t = t.max(0.0) as u64;
    format!("{}:{:02}", t / 60, t % 60)
}

pub enum Align {
    Left,
    Center,
    Right,
}

pub struct Style {
    pub px: f32,
    pub color: Color,
    pub align: Align,
    pub bold: bool,
    pub middle: bool,
}

impl Style {
    pub fn new(px: f32, color: Color) -> Self {
        Self {
            px,
            color,
            align: Align::Left,
            bold: false,
            middle: false,
        }
    }

    pub fn center(self) -> Self {
        Self {
            align: Align::Center,
            ..self
        }
    }

    pub fn bold(self, bold: bool) -> Self {
        Self { bold, ..self }
    }

    pub fn middle(self) -> Self {
        Self {
            middle: true,
            ..self
        }
    }
}

pub struct Text<'a> {
    pub skin: &'a Skin,
    view: Mat4,
    pub screen: Vec2,
}

impl<'a> Text<'a> {
    pub fn screen(skin: &'a Skin) -> Self {
        Self {
            skin,
            view: Mat4::IDENTITY,
            screen: vec2(screen_width(), screen_height()),
        }
    }
    pub fn fit(&self, s: &str, px: f32, max_width: f32) -> String {
        let font = &self.skin.font_bold;
        let size = px.round().max(4.0) as u16;
        if measure_text(s, Some(font), size, 1.0).width <= max_width {
            return s.to_string();
        }
        let mut chars: Vec<char> = s.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let candidate = chars.iter().collect::<String>() + "…";
            if measure_text(&candidate, Some(font), size, 1.0).width <= max_width {
                return candidate;
            }
        }
        String::new()
    }

    fn project(&self, p: Vec3) -> Vec2 {
        let c = self.view * p.extend(1.0);
        vec2(
            (c.x / c.w + 1.0) * 0.5 * self.screen.x,
            (1.0 - c.y / c.w) * 0.5 * self.screen.y,
        )
    }

    fn unit(&self, p: Vec3) -> f32 {
        (self.project(p + Vec3::Y * 0.5) - self.project(p - Vec3::Y * 0.5)).length()
    }

    fn world(&self, s: &str, at: Vec3, em: f32, color: Color, align: Align) {
        let px = quantize(em * self.unit(at));
        if px >= 4.0 {
            self.draw(
                s,
                self.project(at),
                Style {
                    px,
                    color,
                    align,
                    bold: true,
                    middle: true,
                },
            );
        }
    }

    pub fn draw(&self, s: &str, at: Vec2, style: Style) {
        let Style {
            px,
            color,
            align,
            bold,
            middle,
        } = style;
        let font = if bold {
            &self.skin.font_bold
        } else {
            &self.skin.font
        };
        let size = px.round().clamp(4.0, 300.0) as u16;
        self.skin.ensure(bold, size);
        let dim = measure_text(s, Some(font), size, 1.0);
        let x = match align {
            Align::Left => at.x,
            Align::Center => at.x - dim.width / 2.0,
            Align::Right => at.x - dim.width,
        };
        let y = if middle {
            at.y + dim.offset_y / 2.0
        } else {
            at.y
        };
        draw_text_ex(
            s,
            x,
            y,
            TextParams {
                font: Some(font),
                font_size: size,
                color,
                ..Default::default()
            },
        );
    }
}

pub fn camera(cursor: Vec2, settings: &Settings) -> Camera3D {
    let eye = (cursor * settings.parallax).extend(CAM_Z);
    Camera3D {
        position: eye,
        target: eye - Vec3::Z,
        up: Vec3::Y,
        fovy: settings.fov.to_radians(),
        ..Default::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Space {
    Grid,
    Tunnel,
    Void,
}

impl Space {
    pub const ALL: [Space; 3] = [Space::Grid, Space::Tunnel, Space::Void];

    pub fn next(self) -> Self {
        match self {
            Space::Grid => Space::Tunnel,
            Space::Tunnel => Space::Void,
            Space::Void => Space::Grid,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Space::Grid => "grid",
            Space::Tunnel => "tunnel",
            Space::Void => "void",
        }
    }
}

pub fn draw_space(skin: &Skin, cam: &Camera3D, color: Color, space: Space, approach_rate: f32) {
    set_camera(cam);
    if crate::wallpaper::backdrop(cam) {
        gl_use_material(&skin.blend);
        return;
    }
    clear_background(crate::look::fog());
    gl_use_material(&skin.blend);
    match space {
        Space::Grid => grid_space(skin, cam, color),
        Space::Tunnel => tunnel_space(skin, cam, color, approach_rate / 2.0),
        Space::Void => void_space(skin, cam, color),
    }
}

fn grid_space(skin: &Skin, cam: &Camera3D, color: Color) {
    let tile = SPACE_HALF * 2.0 / SPACE_TILES;
    let scroll = (get_time() as f32 * SPACE_SCROLL).fract() * tile;
    let eye = cam.position;
    let fog = crate::look::fog();
    let shade = |p: Vec3, k: f32| lerp_color(scale(color, k), fog, fog_amount((p - eye).length()));
    let rows = ((SPACE_NEAR_Z + SPACE_HALF) / tile).ceil() as usize;
    let z_at = |r: usize| (-SPACE_HALF + r as f32 * tile).min(SPACE_NEAR_Z);
    let mut batch = Batch::new(&skin.white);
    let w = TILE_LINE_WIDTH * tile / 2.0;
    for y in [-SPACE_HEIGHT, SPACE_HEIGHT] {
        let at = |x: f32, z: f32| vec3(x, y, z);
        for r in 0..rows {
            let (z0, z1) = (z_at(r), z_at(r + 1));
            for c in 0..SPACE_TILES as usize {
                let x0 = -SPACE_HALF + c as f32 * tile;
                let p = [at(x0, z0), at(x0 + tile, z0), at(x0 + tile, z1), at(x0, z1)];
                batch.quad(p, p.map(|q| shade(q, TILE_BASE)));
                let lx = x0 + tile / 2.0;
                let p = [
                    at(lx - w, z0),
                    at(lx + w, z0),
                    at(lx + w, z1),
                    at(lx - w, z1),
                ];
                batch.quad(p, p.map(|q| shade(q, TILE_LINE)));
            }
        }
        for r in 0..=rows {
            let z = -SPACE_HALF + (r as f32 + 0.5) * tile + scroll;
            if z > SPACE_NEAR_Z {
                continue;
            }
            for c in 0..SPACE_TILES as usize {
                let x0 = -SPACE_HALF + c as f32 * tile;
                let p = [
                    at(x0, z - w),
                    at(x0 + tile, z - w),
                    at(x0 + tile, z + w),
                    at(x0, z + w),
                ];
                batch.quad(p, p.map(|q| shade(q, TILE_LINE)));
            }
        }
    }
}

const TUNNEL_HALF: f32 = 5.0;
const TUNNEL_SPACING: f32 = 3.5;
const TUNNEL_DEPTH: f32 = 60.0;
const TUNNEL_LANES: usize = 8;
const TUNNEL_ROWS: usize = 30;
const TUNNEL_BAR: f32 = 0.12;
const TUNNEL_NEAR_FADE: f32 = 5.0;
const VOID_DUST: usize = 90;
const VOID_DEPTH: f32 = 50.0;

fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn tunnel_space(skin: &Skin, cam: &Camera3D, color: Color, speed: f32) {
    let eye = cam.position;
    let hue_t = get_time() as f32 * 0.03;
    let fog = crate::look::fog();
    let shade = |p: Vec3, k: f32| {
        let tint = lerp_color(color, prism(p.z * 0.012 + hue_t), 0.12);
        lerp_color(scale(tint, k), fog, fog_amount((p - eye).length()))
    };
    let travel = (get_time() as f32 * speed).rem_euclid(TUNNEL_SPACING);
    let h = TUNNEL_HALF;
    let lane = 2.0 * h / TUNNEL_LANES as f32;
    let row = TUNNEL_DEPTH / TUNNEL_ROWS as f32;
    let mut b = Batch::new(&skin.white);
    for l in 0..TUNNEL_LANES {
        let x0 = -h + l as f32 * lane;
        let k = if l % 2 == 0 { 0.12 } else { 0.2 };
        for r in 0..TUNNEL_ROWS {
            let z0 = -(r as f32) * row - 2.0;
            let z1 = -((r + 1) as f32) * row - 2.0;
            let p = [
                vec3(x0, -h, z0),
                vec3(x0 + lane, -h, z0),
                vec3(x0 + lane, -h, z1),
                vec3(x0, -h, z1),
            ];
            b.quad(p, p.map(|q| shade(q, k)));
        }
    }
    let line = 0.04;
    for l in 0..=TUNNEL_LANES {
        let x = -h + l as f32 * lane;
        for r in 0..TUNNEL_ROWS {
            let z0 = -(r as f32) * row - 2.0;
            let z1 = -((r + 1) as f32) * row - 2.0;
            let p = [
                vec3(x - line, -h, z0),
                vec3(x + line, -h, z0),
                vec3(x + line, -h, z1),
                vec3(x - line, -h, z1),
            ];
            b.quad(p, p.map(|q| shade(q, 0.55)));
        }
    }
    for k in 1..=(TUNNEL_DEPTH / TUNNEL_SPACING) as usize {
        let z = -(k as f32) * TUNNEL_SPACING + travel;
        let near = smooth(0.0, -TUNNEL_NEAR_FADE, z);
        if near <= 0.0 {
            continue;
        }
        let c = fade(shade(vec3(0.0, 0.0, z), 0.8), near);
        let t = TUNNEL_BAR;
        let rect = |b: &mut Batch, x0: f32, y0: f32, x1: f32, y1: f32| {
            b.solid(
                [
                    vec3(x0, y0, z),
                    vec3(x1, y0, z),
                    vec3(x1, y1, z),
                    vec3(x0, y1, z),
                ],
                c,
            );
        };
        rect(&mut b, -h - t, -h - t, h + t, -h + t);
        rect(&mut b, -h - t, h - t, h + t, h + t);
        rect(&mut b, -h - t, -h + t, -h + t, h - t);
        rect(&mut b, h - t, -h + t, h + t, h - t);
        b.solid(
            [
                vec3(-h, -h, z + 0.06),
                vec3(h, -h, z + 0.06),
                vec3(h, -h, z - 0.06),
                vec3(-h, -h, z - 0.06),
            ],
            fade(c, 0.7),
        );
    }
}

fn void_space(skin: &Skin, cam: &Camera3D, color: Color) {
    let t = get_time() as f32;
    gl_use_material(&skin.additive);
    let mut b = Batch::new(&skin.halo);
    let breathe = 0.09 + 0.02 * (t * 0.6).sin();
    sprite(
        &mut b,
        vec3(0.0, 0.0, -40.0),
        Vec2::splat(90.0),
        0.0,
        fade(color, breathe),
    );
    for i in 0..VOID_DUST {
        let (a, c) = (hash(i, 1), hash(i, 3));
        let depth = VOID_DEPTH - ((a * VOID_DEPTH + t * (2.0 + 3.0 * c)).rem_euclid(VOID_DEPTH));
        let p = vec3((hash(i, 2) - 0.5) * 30.0, (hash(i, 4) - 0.5) * 16.0, -depth);
        let dust = lerp_color(
            prism(hash(i, 5) + t * 0.02),
            lerp_color(color, WHITE, 0.55),
            0.82,
        );
        let alpha =
            smooth(0.0, 6.0, (p - cam.position).length() - 3.0) * (1.0 - depth / VOID_DEPTH);
        sprite(
            &mut b,
            p,
            Vec2::splat(0.1 + 0.14 * c),
            0.0,
            fade(dust, 0.3 * alpha),
        );
    }
    drop(b);
    gl_use_material(&skin.blend);
}

impl Game {
    pub fn draw(&self, skin: &Skin, fx: &mut PostFx) {
        let now = self.now();
        let cam = camera(self.run.cursor, &self.settings);
        let target_cam = fx.begin(&cam);
        draw_space(
            skin,
            &target_cam,
            self.run.hud.space_color,
            self.space,
            self.settings.approach_rate,
        );
        self.draw_beat(skin, now);
        self.draw_hud_3d(skin);
        self.draw_grid(skin);
        self.draw_notes(skin, now);
        self.draw_effects(skin, now);
        self.draw_cursor(skin, now);
        gl_use_default_material();
        fx.present();
        let text = Text {
            skin,
            view: cam.matrix(),
            screen: vec2(screen_width(), screen_height()),
        };
        self.draw_hud_text(&text, now);
        self.draw_overlay(&text);
    }

    fn draw_hud_3d(&self, skin: &Skin) {
        let run = &self.run;
        let mut panels = Batch::new(&skin.panel);
        for side in [-1.0, 1.0] {
            panel(side).rect(
                &mut panels,
                vec2(-PANEL_W, -PANEL_H) / 2.0,
                vec2(PANEL_W, PANEL_H) / 2.0,
                BLACK,
            );
        }
        drop(panels);
        let accent = self.palette[0];
        let mut trim = Batch::new(&skin.white);
        for side in [-1.0, 1.0] {
            let f = panel(side);
            let (x0, x1) = (-PANEL_W / 2.0 + 0.2, PANEL_W / 2.0 - 0.2);
            let (top, bottom) = (PANEL_H / 2.0 - 0.18, -PANEL_H / 2.0 + 0.18);
            for (edge, toward) in [(top, -1.0_f32), (bottom, 1.0)] {
                let soft = f.at(vec2(x0, edge + toward * 1.4));
                let hard = f.at(vec2(x0, edge));
                let (soft2, hard2) = (f.at(vec2(x1, edge + toward * 1.4)), f.at(vec2(x1, edge)));
                trim.quad(
                    [soft, soft2, hard2, hard],
                    [
                        fade(accent, 0.0),
                        fade(accent, 0.0),
                        fade(accent, 0.09),
                        fade(accent, 0.09),
                    ],
                );
                f.rect(
                    &mut trim,
                    vec2(-0.45, edge - 0.012),
                    vec2(0.45, edge + 0.012),
                    fade(accent, 0.75),
                );
            }
        }
        drop(trim);
        let left = panel(-1.0);
        let center = panel_px(125.0, 0.65 * PANEL_PX.y);
        let sides = run.multiplier_increment.clamp(3, 32) as usize;
        let pulse = run.hud.mult_pulse;
        let radius = 64.0 * (1.0 + 0.12 * pulse) / PANEL_PX.x * PANEL_W;
        let width = 6.0 / PANEL_PX.x * PANEL_W;
        let filled = run.hud.multiplier_progress * 360.0;
        let corner = |i: usize| {
            let a = i as f32 / sides as f32 * TAU;
            center + vec2(a.sin(), a.cos()) * radius
        };
        const STEPS: usize = 8;
        let ring_color = run.hud.multiplier_color;
        gl_use_material(&skin.additive);
        let mut glow = Batch::new(&skin.white);
        for i in 0..sides {
            let (a, c) = (corner(i), corner(i + 1));
            for s in 0..STEPS {
                let p0 = a.lerp(c, s as f32 / STEPS as f32);
                let p1 = a.lerp(c, (s + 1) as f32 / STEPS as f32);
                let mid = (p0 + p1) / 2.0 - center;
                let angle = mid.x.atan2(mid.y).to_degrees().rem_euclid(360.0);
                if angle <= filled {
                    left.line(
                        &mut glow,
                        p0,
                        p1,
                        width * 3.4,
                        fade(ring_color, 0.035 + 0.2 * pulse),
                    );
                    left.line(
                        &mut glow,
                        p0,
                        p1,
                        width * 1.8,
                        fade(ring_color, 0.07 + 0.3 * pulse),
                    );
                }
            }
        }
        let flash = |opacity: f32| ((opacity - 0.62) / 0.38).clamp(0.0, 1.0);
        let right = panel(1.0);
        let row = 0.625 * PANEL_PX.y;
        let icon = |x0: f32, x1: f32| (panel_px(x0, row + 7.0), panel_px(x1, row - 12.0));
        let pad = 0.1;
        let (lo, hi) = icon(125.0 - 21.0, 125.0 - 2.0);
        right.rect(
            &mut glow,
            lo - Vec2::splat(pad),
            hi + Vec2::splat(pad),
            fade(HITS_ICON, 0.32 * flash(run.hud.hits_opacity)),
        );
        let (lo, hi) = icon(125.0 + 2.0, 125.0 + 21.0);
        right.rect(
            &mut glow,
            lo - Vec2::splat(pad),
            hi + Vec2::splat(pad),
            fade(MISS_ICON, 0.32 * flash(run.hud.misses_opacity)),
        );
        drop(glow);
        gl_use_material(&skin.blend);
        let mut b = Batch::new(&skin.white);
        let fill = fade(ring_color, 0.025 + 0.035 * run.hud.multiplier_progress);
        for i in 0..sides {
            let (a, c) = (left.at(corner(i)), left.at(corner(i + 1)));
            b.solid([left.at(center), a, c, c], fill);
        }
        for i in 0..sides {
            let (a, c) = (corner(i), corner(i + 1));
            for s in 0..STEPS {
                let p0 = a.lerp(c, s as f32 / STEPS as f32);
                let p1 = a.lerp(c, (s + 1) as f32 / STEPS as f32);
                let mid = (p0 + p1) / 2.0 - center;
                let angle = mid.x.atan2(mid.y).to_degrees().rem_euclid(360.0);
                let color = if angle <= filled {
                    run.hud.multiplier_color
                } else {
                    RING_GRAY
                };
                let ext = (p1 - p0).normalize_or_zero() * width / 2.0;
                left.line(&mut b, p0 - ext * 0.15, p1 + ext * 0.15, width, color);
            }
        }
        let (lo, hi) = icon(125.0 - 21.0, 125.0 - 2.0);
        right.rect(
            &mut b,
            vec2(lo.x, lo.y),
            vec2(hi.x, hi.y),
            fade(HITS_ICON, run.hud.hits_opacity),
        );
        let (lo, hi) = icon(125.0 + 2.0, 125.0 + 21.0);
        let m = fade(MISS_ICON, run.hud.misses_opacity);
        let t = (hi.x - lo.x) * 0.2;
        right.line(&mut b, lo, hi, t, m);
        right.line(&mut b, vec2(lo.x, hi.y), vec2(hi.x, lo.y), t, m);
        let ly = 0.65 * PANEL_PX.y;
        let (l0, l1) = (panel_px(16.0, ly), panel_px(234.0, ly));
        let lh = 2.0 / PANEL_PX.y * PANEL_H;
        let mid = (l0 + l1) / 2.0;
        let corners = |a: Vec2, z: Vec2| {
            [
                vec2(a.x, a.y - lh / 2.0),
                vec2(z.x, a.y - lh / 2.0),
                vec2(z.x, a.y + lh / 2.0),
                vec2(a.x, a.y + lh / 2.0),
            ]
        };
        for (a, z, ca, cz) in [(l0, mid, 0.0, 0.5), (mid, l1, 0.5, 0.0)] {
            let c = corners(a, z).map(|q| right.at(q));
            b.quad(
                c,
                [
                    fade(WHITE, ca),
                    fade(WHITE, cz),
                    fade(WHITE, cz),
                    fade(WHITE, ca),
                ],
            );
        }
        let alpha = if run.failed { 0.5 } else { 1.0 };
        let health = tilted_bar(-6.3, -1.0);
        let (min, max) = (vec2(-BAR_HALF_W, -BAR_HALF_H), vec2(BAR_HALF_W, BAR_HALF_H));
        health.rect(&mut b, min, max, fade(accent, 0.18 * alpha));
        let level = run.hud.health.clamp(0.0, 1.0);
        let fill = -BAR_HALF_W + 2.0 * BAR_HALF_W * level;
        let danger = ((0.25 - level) / 0.25).clamp(0.0, 1.0);
        let beat = 0.925 + 0.075 * (self.now() as f32 * TAU * 2.0).sin();
        let bar = lerp_color(HEALTH, HEALTH_LOW, danger);
        health.rect(
            &mut b,
            min,
            vec2(fill, BAR_HALF_H),
            fade(bar, alpha * (1.0 - danger + danger * beat)),
        );
        let progress = tilted_bar(6.3, 1.0);
        progress.rect(&mut b, min, max, PROGRESS_BG);
        let p = (self.now() / self.length()).clamp(0.0, 1.0) as f32;
        progress.rect(
            &mut b,
            min,
            vec2(-BAR_HALF_W + 2.0 * BAR_HALF_W * p, BAR_HALF_H),
            WHITE,
        );
    }

    fn draw_grid(&self, skin: &Skin) {
        let mut b = Batch::new(&skin.white);
        let flat = Frame3 {
            origin: Vec3::ZERO,
            ax: Vec3::X,
            ay: Vec3::Y,
        };
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                let corner = vec2(sx, sy) * GRID_EDGE;
                let inward = -vec2(sx, sy);
                let h0 = corner;
                let h1 = corner + vec2(inward.x * GRID_ARM, inward.y * GRID_THICK);
                flat.rect(&mut b, h0.min(h1), h0.max(h1), WHITE);
                let v1 = corner + vec2(inward.x * GRID_THICK, inward.y * GRID_ARM);
                flat.rect(&mut b, h0.min(v1), h0.max(v1), WHITE);
            }
        }
    }

    fn draw_notes(&self, skin: &Skin, now: f64) {
        let run = &self.run;
        let notes = &self.map.notes;
        let at = self.approach_time();
        let (distance, note_size) = (self.settings.approach_distance, self.settings.note_size);
        let hit_depth = HIT_WINDOW as f32 * self.settings.approach_rate;
        let lo = notes.partition_point(|n| n.time < now - HIT_WINDOW);
        let hi = notes.partition_point(|n| n.time <= now + at);
        let visible: Vec<(usize, Vec3, Color)> = (lo..hi)
            .rev()
            .filter(|&i| run.states[i] == NoteState::Pending)
            .map(|i| {
                let n = notes[i];
                let depth = ((n.time - now) / at) as f32 * distance;
                let progress = 1.0 - ((depth + hit_depth) / (distance + hit_depth)).max(0.0);
                let opacity = (progress / FADE_IN).clamp(0.0, 1.0).powf(OPACITY_EXPONENT);
                (i, vec3(n.x, n.y, -depth), fade(self.note_color(i), opacity))
            })
            .collect();
        gl_use_material(&skin.additive);
        let mut glow = Batch::new(&skin.note_glow);
        for &(_, pos, color) in &visible {
            sprite(
                &mut glow,
                pos,
                Vec2::splat(note_size * NOTE_GLOW_SCALE),
                0.0,
                fade(color, NOTE_GLOW),
            );
        }
        drop(glow);
        gl_use_material(&skin.blend);
        let mut b = Batch::new(&skin.white);
        let s = note_size / 2.0;
        for &(_, pos, color) in &visible {
            for (quad, k) in &skin.note_mesh {
                b.solid(
                    quad.map(|v| pos + v * s),
                    Color {
                        a: color.a,
                        ..scale(color, *k)
                    },
                );
            }
        }
        drop(b);
        if self.settings.note_core {
            let mut cores = Batch::new(&skin.fill);
            for &(_, pos, color) in &visible {
                let hot = lerp_color(color, WHITE, 0.65);
                sprite(
                    &mut cores,
                    pos + Vec3::Z * (SQUIRCLE_DEPTH * s + 0.002),
                    Vec2::splat(note_size * 0.8),
                    0.0,
                    Color { a: color.a, ..hot },
                );
            }
        }
    }

    fn draw_beat(&self, skin: &Skin, now: f64) {
        if !self.settings.beat_glow || crate::look::active().calm || self.phase != Phase::Playing {
            return;
        }
        let notes = &self.map.notes;
        let due = notes.partition_point(|n| n.time <= now);
        let Some(last) = due.checked_sub(1).map(|i| notes[i]) else {
            return;
        };
        let kick = (-(now - last.time) as f32 * 7.0).exp();
        let t = now as f32;
        let hue = note_hue(due) + t * 0.02;
        gl_use_material(&skin.additive);
        let mut b = Batch::new(&skin.halo);
        sprite(
            &mut b,
            vec3(0.0, 0.0, -38.0),
            Vec2::splat(80.0 + 10.0 * kick),
            0.0,
            fade(prism(hue), 0.05 + 0.07 * kick),
        );
        sprite(
            &mut b,
            vec3((t * 0.25).sin() * 14.0, (t * 0.18).cos() * 7.0, -42.0),
            Vec2::splat(56.0),
            0.0,
            fade(prism(hue + 0.33), 0.05 + 0.03 * kick),
        );
        drop(b);
        gl_use_material(&skin.blend);
    }

    fn draw_effects(&self, skin: &Skin, now: f64) {
        if crate::look::active().calm {
            return;
        }
        gl_use_material(&skin.additive);
        let pulse = self.run.hud.mult_pulse;
        if self.settings.level_pulse && pulse > 0.0 {
            let mut ring = Batch::new(&skin.wave);
            let color = lerp_color(self.note_color(self.run.last_hit.unwrap_or(0)), WHITE, 0.4);
            sprite(
                &mut ring,
                vec3(0.0, 0.0, -0.05),
                Vec2::splat(2.5 + (1.0 - pulse) * 5.0),
                0.0,
                fade(color, 0.22 * pulse * pulse),
            );
        }
        let mut halos = Batch::new(&skin.halo);
        for e in &self.run.effects {
            let age = ((now - e.born) / BREAK_LIFE).clamp(0.0, 1.0) as f32;
            if e.score.is_some() {
                sprite(
                    &mut halos,
                    e.pos.extend(-0.02),
                    Vec2::splat(1.2 + age * 0.5),
                    0.0,
                    fade(self.note_color(e.note), 0.24 * (1.0 - age).powi(3)),
                );
            }
        }
        drop(halos);
        let mut flashes = Batch::new(&skin.fill);
        for e in &self.run.effects {
            let t = (now - e.born) as f32;
            if e.score.is_some() && t < HIT_FLASH_LIFE {
                let a = t / HIT_FLASH_LIFE;
                let color = lerp_color(self.note_color(e.note), WHITE, 0.65);
                sprite(
                    &mut flashes,
                    e.pos.extend(0.01),
                    Vec2::splat(self.settings.note_size * (1.0 + 0.3 * a)),
                    0.0,
                    fade(color, 0.95 * (1.0 - a).powi(2)),
                );
            }
        }
        drop(flashes);
        let mut waves = Batch::new(&skin.wave);
        for e in &self.run.effects {
            let age = ((now - e.born) / BREAK_LIFE).clamp(0.0, 1.0) as f32;
            if e.score.is_some() {
                sprite(
                    &mut waves,
                    e.pos.extend(-0.03),
                    Vec2::splat(0.6 + age * 1.0),
                    0.0,
                    fade(self.note_color(e.note), 0.45 * (1.0 - age).powi(2)),
                );
            }
        }
        for e in &self.run.effects {
            let age = ((now - e.born - 0.05) / BREAK_LIFE).clamp(0.0, 1.0) as f32;
            if e.score.is_some() && now - e.born > 0.05 && age < 1.0 {
                sprite(
                    &mut waves,
                    e.pos.extend(-0.03),
                    Vec2::splat(0.5 + age * 1.5),
                    0.0,
                    fade(
                        lerp_color(self.note_color(e.note), prism(note_hue(e.note) + 0.18), 0.2),
                        0.3 * (1.0 - age).powi(2),
                    ),
                );
            }
        }
        drop(waves);
        gl_use_material(&skin.blend);
        let mut b = Batch::new(&skin.white);
        for e in &self.run.effects {
            let t = (now - e.born) as f32;
            match e.score {
                Some(_) => {
                    let a = t / BREAK_LIFE as f32;
                    if a >= 1.0 {
                        continue;
                    }
                    for k in 0..8 {
                        let (r1, r2, r3) = (
                            hash(e.note, k * 3),
                            hash(e.note, k * 3 + 1),
                            hash(e.note, k * 3 + 2),
                        );
                        let angle = (k as f32 + r1 * 0.6) / 8.0 * TAU;
                        let dir = Vec2::from_angle(angle);
                        let size = (0.07 + r3 * 0.05) * (1.0 - a * 0.5);
                        let c = lerp_color(
                            self.note_color(e.note),
                            prism(note_hue(e.note) + k as f32 * 0.03),
                            0.22,
                        );
                        for (lag, w) in [(0.0, 1.0), (0.02, 0.6), (0.045, 0.3)] {
                            let tl = (t - lag).max(0.0);
                            let xy =
                                e.pos + dir * (0.3 + (1.0 + r2) * tl) - vec2(0.0, 3.5 * tl * tl);
                            sprite(
                                &mut b,
                                xy.extend(-(1.0 + r3) * tl),
                                Vec2::splat(size * (0.4 + 0.6 * w)),
                                angle + r2 * 9.0 * tl,
                                fade(c, (1.0 - a) * 0.9 * w),
                            );
                        }
                    }
                }
                None => {
                    let a = t / POPUP_LIFE as f32;
                    if a >= 1.0 {
                        continue;
                    }
                    let size = 0.128;
                    let c = fade(MISS_ICON, 100.0 / 255.0 * (1.0 - a));
                    let p = vec3(e.x, -1.4, 0.0);
                    for rot in [TAU / 8.0, -TAU / 8.0] {
                        sprite(&mut b, p, vec2(size * 1.3, size * 0.25), rot, c);
                    }
                }
            }
        }
    }

    fn draw_cursor(&self, skin: &Skin, now: f64) {
        let run = &self.run;
        if self.settings.cursor_trail && !crate::look::active().calm {
            gl_use_material(&skin.additive);
            let mut ribbon = Batch::new(skin.cursor(self.settings.cursor_style));
            for &(t, p) in run.trail.iter().skip(1).rev() {
                if p == run.cursor {
                    continue;
                }
                let alpha = 1.0 - ((now - t) / TRAIL_TIME).clamp(0.0, 1.0) as f32;
                sprite(
                    &mut ribbon,
                    p.extend(0.0),
                    Vec2::splat(CURSOR_SIZE * self.settings.cursor_size * (0.35 + 0.65 * alpha)),
                    0.0,
                    fade(
                        lerp_color(
                            self.palette[0],
                            prism(now as f32 * 0.2 + (1.0 - alpha) * 0.4),
                            0.25,
                        ),
                        alpha * 0.55,
                    ),
                );
            }
            drop(ribbon);
            gl_use_material(&skin.blend);
        }
        let mut b = Batch::new(skin.cursor(self.settings.cursor_style));
        sprite(
            &mut b,
            run.cursor.extend(0.0),
            Vec2::splat(CURSOR_SIZE * self.settings.cursor_size),
            0.0,
            crate::look::cursor_color(self.settings.cursor_color),
        );
    }

    fn draw_hud_text(&self, text: &Text, now: f64) {
        let run = &self.run;
        let left = panel(-1.0);
        let right = panel(1.0);
        let em = 50.0 / PANEL_PX.y * PANEL_H;
        let small = 30.0 / PANEL_PX.y * PANEL_H;
        let at = |f: Frame3, x: f32, y: f32| f.at(panel_px(x, y * PANEL_PX.y));
        let label = em * 0.44;
        let dim = fade(WHITE, 0.4);
        let shown = if self.phase == Phase::Playing {
            run.hud.score_shown.round() as u64
        } else {
            run.score
        };
        text.world("SCORE", at(left, 125.0, 0.28), label, dim, Align::Center);
        text.world(
            &format_count(shown),
            at(left, 125.0, 0.35),
            em,
            WHITE,
            Align::Center,
        );
        let pulse = run.hud.mult_pulse;
        text.world(
            &format!("{}x", run.multiplier),
            at(left, 125.0, 0.65),
            em * (1.0 + 0.3 * pulse),
            WHITE,
            Align::Center,
        );
        text.world(
            "MULTIPLIER",
            at(left, 125.0, 0.79),
            label,
            dim,
            Align::Center,
        );
        let grade_color = match run.grade() {
            "SS" | "S" => GRADE_S,
            "A" => GRADE_A,
            "B" => GRADE_B,
            "C" => self.note_color(0),
            _ => MISS,
        };
        let tint = |k: f32| {
            Color::new(
                WHITE.r + (grade_color.r - WHITE.r) * k,
                WHITE.g + (grade_color.g - WHITE.g) * k,
                WHITE.b + (grade_color.b - WHITE.b) * k,
                1.0,
            )
        };
        text.world(
            "ACCURACY",
            at(right, 125.0, 0.28),
            label,
            dim,
            Align::Center,
        );
        let acc = format!("{:.2}%", run.accuracy());
        text.world(&acc, at(right, 125.0, 0.35), em, tint(0.4), Align::Center);
        text.world(
            run.grade(),
            at(right, 125.0, 0.43),
            em * 0.5,
            fade(grade_color, 0.7),
            Align::Center,
        );
        text.world("NOTES", at(right, 125.0, 0.79), label, dim, Align::Center);
        let hits = fade(tint_to(HITS_ICON, 0.35), run.hud.hits_opacity);
        let misses = fade(tint_to(MISS, 0.35), run.hud.misses_opacity);
        text.world(
            &run.hits.to_string(),
            at(right, 100.0, 0.6175),
            small,
            hits,
            Align::Right,
        );
        text.world(
            &run.misses.to_string(),
            at(right, 150.0, 0.6175),
            small,
            misses,
            Align::Left,
        );
        text.world(
            &format_count(run.judged() as u64),
            at(right, 125.0, 0.7),
            em,
            WHITE,
            Align::Center,
        );
        if run.combo > 0 {
            text.world(
                &run.combo.to_string(),
                vec3(0.0, 3.5, -10.0),
                2.24,
                fade(WHITE, 0.14),
                Align::Center,
            );
        }
        let time = format!("{} / {}", clock(now), clock(self.length()));
        text.world(
            &time,
            vec3(0.0, 7.0, -10.0),
            1.12,
            fade(WHITE, 0.745),
            Align::Center,
        );
        text.world(
            &self.map.title,
            vec3(0.0, 8.25, -10.0),
            0.64,
            WHITE,
            Align::Center,
        );
        for e in &run.effects {
            let Some(score) = e.score else { continue };
            if crate::look::active().calm {
                continue;
            }
            let a = ((now - e.born) / POPUP_LIFE) as f32;
            if a < 1.0 {
                let rise = 1.0 - (1.0 - a).powi(5);
                let p = vec3(e.x, -1.4 + 0.25 * rise, 0.0);
                text.world(
                    &score.to_string(),
                    p,
                    0.16,
                    fade(WHITE, 0.47 * (1.0 - a)),
                    Align::Center,
                );
            }
        }
        if self.phase == Phase::Playing && now < 0.0 {
            let n = (-now).ceil() as i32;
            let pulse = (-now).fract() as f32;
            text.world(
                &n.to_string(),
                Vec3::ZERO,
                0.7,
                fade(WHITE, 0.2 + 0.5 * pulse),
                Align::Center,
            );
        }
        let info = format!(
            "{:.0} fps {:.1} ms (p99 {:.1})   audio {:.1} ms   pace {:.1} ms   swap {:.1} ms   offset {:+.0} ms  [ ]   sensitivity {:.2}  - =   space {}  F3",
            1000.0 / self.stats.avg_ms.max(0.001),
            self.stats.avg_ms,
            self.stats.p99_ms,
            self.music.clock_stats().period_ms,
            self.stats.delay_ms,
            self.stats.swap_ms,
            self.offset * 1000.0,
            self.sensitivity,
            self.space.name()
        );
        text.draw(
            &info,
            vec2(16.0, text.screen.y - 14.0),
            Style::new(14.0, fade(WHITE, 0.3)),
        );
    }

    fn draw_overlay(&self, text: &Text) {
        let run = &self.run;
        let best_line = match self.phase {
            Phase::Ready | Phase::Finished if self.new_best => Some("NEW BEST".to_string()),
            Phase::Ready | Phase::Finished => self
                .best
                .map(|b| format!("best {}   {:.2}%", format_count(b.score), b.accuracy)),
            _ => None,
        };
        let (title, mut lines): (String, Vec<String>) = match self.phase {
            Phase::Playing => return,
            Phase::Ready => (
                self.map.title.clone(),
                vec![
                    if self.map.mappers.is_empty() {
                        format!("{} notes", self.map.notes.len())
                    } else {
                        format!(
                            "mapped by {}   {} notes",
                            self.map.mappers,
                            self.map.notes.len()
                        )
                    },
                    "click to play".into(),
                    "Esc back   R restart   [ ] offset   - = sensitivity   F2 glow".into(),
                ],
            ),
            Phase::Paused => (
                "PAUSED".into(),
                vec![
                    "click or Esc to resume".into(),
                    "R restart   Q back to maps".into(),
                ],
            ),
            Phase::Finished => (
                run.grade().into(),
                vec![
                    if run.failed {
                        "FAILED".into()
                    } else {
                        format!("{}   {:.2}%", format_count(run.score), run.accuracy())
                    },
                    format!(
                        "{} hits   {} misses   {}x max combo",
                        run.hits, run.misses, run.max_combo
                    ),
                    match self.summary() {
                        Some(s) => format!(
                            "avg {:.0} ms late   spread {:.0} ms   95% within {:.0} ms",
                            s.mean_ms, s.spread_ms, s.p95_ms
                        ),
                        None => "no hits to measure".into(),
                    },
                    "click to retry   V watch replay   Esc back to maps".into(),
                ],
            ),
        };
        if let Some(line) = best_line {
            lines.insert(1, line);
        }
        let age = (get_time() - self.phase_since) as f32;
        let reveal = (1.0 - (-age * 10.0).exp()).clamp(0.0, 1.0);
        let rise = (1.0 - reveal) * 16.0;
        let (w, h) = (text.screen.x, text.screen.y);
        draw_rectangle(0.0, 0.0, w, h, fade(crate::look::fog(), 0.85 * reveal));
        let s = h / 720.0;
        let color = match self.phase {
            Phase::Finished => match run.grade() {
                "SS" | "S" => GRADE_S,
                "A" => GRADE_A,
                "B" => GRADE_B,
                "C" => self.note_color(0),
                _ => MISS,
            },
            _ => WHITE,
        };
        let color = fade(color, reveal);
        let size = if self.phase == Phase::Finished {
            130.0
        } else {
            46.0
        };
        text.draw(
            &title,
            vec2(w / 2.0, h * 0.42 + rise),
            Style::new(size * s, color).center().bold(true),
        );
        for (i, line) in lines.iter().enumerate() {
            let (px, alpha, bold) = if i == 0 {
                (22.0, 0.9, true)
            } else {
                (17.0, 0.55, false)
            };
            let y = h * 0.42 + (46.0 + i as f32 * 32.0) * s + rise;
            text.draw(
                line,
                vec2(w / 2.0, y),
                Style::new(px * s, fade(WHITE, alpha * reveal))
                    .center()
                    .bold(bold),
            );
        }
    }
}
