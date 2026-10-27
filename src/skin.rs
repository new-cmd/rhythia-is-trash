use std::{cell::RefCell, collections::HashSet};

use macroquad::{
    miniquad::{BlendFactor, BlendState, BlendValue, Comparison, Equation, PipelineParams},
    prelude::*,
};

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying mediump vec2 uv;
varying mediump vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}"#;

const FRAGMENT: &str = r#"#version 100
varying mediump vec2 uv;
varying mediump vec4 color;
uniform sampler2D Texture;
void main() {
    gl_FragColor = color * texture2D(Texture, uv);
}"#;

pub struct Skin {
    pub note_mesh: Vec<([Vec3; 4], f32)>,
    pub note_glow: Texture2D,
    pub cursors: Vec<Texture2D>,
    pub fill: Texture2D,
    pub halo: Texture2D,
    pub wave: Texture2D,
    pub panel: Texture2D,
    pub white: Texture2D,
    pub font: Font,
    pub font_bold: Font,
    seen: RefCell<HashSet<(bool, u16)>>,
    pub blend: Material,
    pub additive: Material,
}

pub const NOTE_GLOW_SCALE: f32 = 1.7;
pub const CURSOR_STYLES: [&str; 5] = ["Square", "Ring", "Dot", "Cross", "Diamond"];
const SQUIRCLE_EXP: f32 = 4.5;
const SQUIRCLE_INNER: f32 = 0.714;
pub const SQUIRCLE_DEPTH: f32 = 0.125;
const SQUIRCLE_SEGMENTS: usize = 48;

impl Skin {
    pub fn ensure(&self, bold: bool, size: u16) {
        if !self.seen.borrow_mut().insert((bold, size)) {
            return;
        }
        let font = if bold { &self.font_bold } else { &self.font };
        let chars: Vec<char> = if size <= 64 {
            (32u8..127).map(char::from).collect()
        } else {
            "0123456789,.%:/+-x ".chars().collect()
        };
        font.populate_font_cache(&chars, size);
    }

    pub fn load() -> Self {
        let report = std::env::var_os("RHYTHM_PERF").is_some();
        let started = std::time::Instant::now();
        let font_semibold = font(include_bytes!("../assets/fonts/NotoSans-SemiBold.ttf"));
        let font_bold = font(include_bytes!("../assets/fonts/NotoSans-Bold.ttf"));
        let fonts_ms = started.elapsed().as_secs_f64() * 1e3;
        let blend = material(BlendFactor::OneMinusValue(BlendValue::SourceAlpha));
        let additive = material(BlendFactor::One);
        let materials_ms = started.elapsed().as_secs_f64() * 1e3 - fonts_ms;
        let skin = Self::bake_all(font_semibold, font_bold, blend, additive);
        if report {
            eprintln!(
                "startup: Skin::load {:.1} ms (fonts {fonts_ms:.1}, materials {materials_ms:.1}, textures {:.1})",
                started.elapsed().as_secs_f64() * 1e3,
                started.elapsed().as_secs_f64() * 1e3 - fonts_ms - materials_ms,
            );
        }
        skin
    }

    fn bake_all(font: Font, font_bold: Font, blend: Material, additive: Material) -> Self {
        Self {
            note_mesh: squircle_frame(),
            note_glow: bake(128, 128, NOTE_GLOW_SCALE, |p, _| {
                let d = sd_round_box(p, Vec2::splat(0.94), 0.45);
                let band = (d + 0.14).abs() - 0.14;
                (-band.max(0.0) * 5.0).exp() * 0.5 * edge_fade(p, NOTE_GLOW_SCALE)
            }),
            halo: bake(64, 64, 1.0, |p, _| {
                (-p.length_squared() * 5.0).exp() * edge_fade(p, 1.0)
            }),
            wave: bake(128, 128, 1.0, |p, px| {
                let d = (p.length() - 0.8).abs() - 0.018;
                cover(d, px)
            }),
            fill: bake(64, 64, 1.0, |p, px| {
                cover(sd_round_box(p, Vec2::splat(0.94), 0.45), px)
            }),
            cursors: CURSOR_STYLES
                .iter()
                .map(|style| bake(128, 128, 1.0, |p, px| cursor_alpha(style, p, px)))
                .collect(),
            panel: bake(64, 160, 1.0, |p, _| {
                let fade = ((1.0 - p.y.abs()) / 0.21).clamp(0.0, 1.0);
                0.38 * fade
            }),
            white: bake(4, 4, 1.0, |_, _| 1.0),
            font,
            font_bold,
            seen: Default::default(),
            blend,
            additive,
        }
    }
}

fn squircle(angle: f32, radius: f32) -> Vec2 {
    let (s, c) = angle.sin_cos();
    let e = 2.0 / SQUIRCLE_EXP;
    vec2(c.signum() * c.abs().powf(e), s.signum() * s.abs().powf(e)) * radius
}

fn squircle_frame() -> Vec<([Vec3; 4], f32)> {
    let d = SQUIRCLE_DEPTH;
    let mut quads = Vec::with_capacity(SQUIRCLE_SEGMENTS * 3);
    for i in 0..SQUIRCLE_SEGMENTS {
        let a0 = i as f32 / SQUIRCLE_SEGMENTS as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / SQUIRCLE_SEGMENTS as f32 * std::f32::consts::TAU;
        let (o0, o1) = (squircle(a0, 1.0), squircle(a1, 1.0));
        let (i0, i1) = (squircle(a0, SQUIRCLE_INNER), squircle(a1, SQUIRCLE_INNER));
        quads.push((
            [o0.extend(-d), o1.extend(-d), o1.extend(d), o0.extend(d)],
            0.7,
        ));
        quads.push((
            [i1.extend(-d), i0.extend(-d), i0.extend(d), i1.extend(d)],
            0.55,
        ));
        quads.push((
            [i0.extend(d), o0.extend(d), o1.extend(d), i1.extend(d)],
            1.0,
        ));
    }
    quads
}

fn font(bytes: &[u8]) -> Font {
    let mut f = load_ttf_font_from_bytes(bytes).expect("bundled font is valid");
    f.set_filter(FilterMode::Linear);
    f
}

fn material(dst: BlendFactor) -> Material {
    load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment: FRAGMENT,
        },
        MaterialParams {
            pipeline_params: PipelineParams {
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    dst,
                )),
                alpha_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Zero,
                    BlendFactor::One,
                )),
                depth_write: false,
                depth_test: Comparison::Always,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("built-in shader compiles")
}

impl Skin {
    pub fn cursor(&self, style: usize) -> &Texture2D {
        &self.cursors[style.min(self.cursors.len() - 1)]
    }
}

/// Outlines keep the original square cursor's weight: a 0.23 wide edge over a faint fill.
fn cursor_alpha(style: &str, p: Vec2, px: f32) -> f32 {
    let ring = |d: f32| {
        let outer = cover(d, px);
        let inner = cover(d + 0.23, px);
        (outer - inner) + inner * 0.05
    };
    match style {
        "Ring" => ring(p.length() - 0.97),
        "Dot" => cover(p.length() - 0.55, px),
        "Cross" => {
            let bar = |half: Vec2| sd_round_box(p, half, 0.06);
            cover(bar(vec2(0.97, 0.13)).min(bar(vec2(0.13, 0.97))), px)
        }
        "Diamond" => ring((p.x.abs() + p.y.abs() - 0.99) * std::f32::consts::FRAC_1_SQRT_2),
        _ => ring(p.abs().max_element() - 0.99),
    }
}

fn sd_round_box(p: Vec2, half: Vec2, r: f32) -> f32 {
    let q = p.abs() - half + r;
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - r
}

fn cover(d: f32, px: f32) -> f32 {
    (0.5 - d / px).clamp(0.0, 1.0)
}

fn edge_fade(p: Vec2, extent: f32) -> f32 {
    let e = p.abs().max_element() / extent;
    (1.0 - ((e - 0.8) / 0.2).clamp(0.0, 1.0)).powi(2)
}

fn bake(w: u16, h: u16, extent: f32, alpha: impl Fn(Vec2, f32) -> f32) -> Texture2D {
    let px = 2.0 * extent / h as f32;
    let mut bytes = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            let p = vec2(
                ((x as f32 + 0.5) / w as f32 * 2.0 - 1.0) * extent,
                (1.0 - (y as f32 + 0.5) / h as f32 * 2.0) * extent,
            );
            let a = (alpha(p, px).clamp(0.0, 1.0) * 255.0).round() as u8;
            bytes.extend_from_slice(&[255, 255, 255, a]);
        }
    }
    let t = Texture2D::from_rgba8(w, h, &bytes);
    t.set_filter(FilterMode::Linear);
    t
}
