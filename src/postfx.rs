use macroquad::prelude::*;

const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
varying mediump vec2 uv;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    uv = texcoord;
}"#;

const EXTRACT: &str = r#"#version 100
precision mediump float;
varying vec2 uv;
uniform sampler2D Texture;
void main() {
    vec3 c = texture2D(Texture, uv).rgb;
    float peak = max(c.r, max(c.g, c.b));
    float soft = clamp((peak - 0.48) / 0.4, 0.0, 1.0);
    gl_FragColor = vec4(c * soft * soft, 1.0);
}"#;

const BLUR: &str = r#"#version 100
precision mediump float;
varying vec2 uv;
uniform sampler2D Texture;
uniform vec2 Step;
void main() {
    vec3 c = texture2D(Texture, uv).rgb * 0.227027;
    c += texture2D(Texture, uv + Step * 1.384615).rgb * 0.316216;
    c += texture2D(Texture, uv - Step * 1.384615).rgb * 0.316216;
    c += texture2D(Texture, uv + Step * 3.230769).rgb * 0.070270;
    c += texture2D(Texture, uv - Step * 3.230769).rgb * 0.070270;
    gl_FragColor = vec4(c, 1.0);
}"#;

const COMBINE: &str = r#"#version 100
precision mediump float;
varying vec2 uv;
uniform sampler2D Texture;
uniform sampler2D Bloom;
uniform sampler2D Wide;
uniform float Strength;
uniform float Vignette;
void main() {
    vec3 base = texture2D(Texture, uv).rgb;
    vec3 glow = texture2D(Bloom, uv).rgb * 0.5 + texture2D(Wide, uv).rgb * 0.3;
    float lum = dot(glow, vec3(0.299, 0.587, 0.114));
    glow = mix(vec3(lum), glow, 1.3);
    vec3 c = base + glow * Strength;
    float edge = smoothstep(0.45, 1.15, length(uv - 0.5));
    c *= 1.0 - Vignette * edge;
    gl_FragColor = vec4(c, 1.0);
}"#;

struct Targets {
    size: (u32, u32),
    scene: RenderTarget,
    ping: RenderTarget,
    bloom: RenderTarget,
    wide_ping: RenderTarget,
    wide: RenderTarget,
}

impl Targets {
    fn new(w: u32, h: u32) -> Self {
        let texture = |w, h| {
            let t = render_target(w, h);
            t.texture.set_filter(FilterMode::Linear);
            t
        };
        let scene = render_target_ex(
            w,
            h,
            RenderTargetParams {
                sample_count: 4,
                depth: true,
            },
        );
        scene.texture.set_filter(FilterMode::Linear);
        Self {
            size: (w, h),
            scene,
            ping: texture((w / 4).max(1), (h / 4).max(1)),
            bloom: texture((w / 4).max(1), (h / 4).max(1)),
            wide_ping: texture((w / 8).max(1), (h / 8).max(1)),
            wide: texture((w / 8).max(1), (h / 8).max(1)),
        }
    }
}

pub struct PostFx {
    targets: Option<Targets>,
    extract: Material,
    blur: Material,
    combine: Material,
    pub enabled: bool,
}

impl PostFx {
    pub fn new() -> Self {
        let shader = |fragment, params| {
            load_material(
                ShaderSource::Glsl {
                    vertex: VERTEX,
                    fragment,
                },
                params,
            )
            .expect("built-in post shader compiles")
        };
        Self {
            targets: None,
            extract: shader(EXTRACT, MaterialParams::default()),
            blur: shader(
                BLUR,
                MaterialParams {
                    uniforms: vec![UniformDesc::new("Step", UniformType::Float2)],
                    ..Default::default()
                },
            ),
            combine: shader(
                COMBINE,
                MaterialParams {
                    uniforms: vec![
                        UniformDesc::new("Strength", UniformType::Float1),
                        UniformDesc::new("Vignette", UniformType::Float1),
                    ],
                    textures: vec!["Bloom".into(), "Wide".into()],
                    ..Default::default()
                },
            ),
            enabled: true,
        }
    }

    pub fn begin(&mut self, camera: &Camera3D) -> Camera3D {
        let size = (
            screen_width().max(1.0) as u32,
            screen_height().max(1.0) as u32,
        );
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            self.targets = Some(Targets::new(size.0, size.1));
        }
        let target = self
            .targets
            .as_ref()
            .expect("targets initialized")
            .scene
            .clone();
        Camera3D {
            position: camera.position,
            target: camera.target,
            up: camera.up,
            fovy: camera.fovy,
            aspect: camera.aspect,
            projection: camera.projection,
            z_near: camera.z_near,
            z_far: camera.z_far,
            viewport: camera.viewport,
            render_target: Some(target),
        }
    }

    pub fn present(&self) {
        let t = self.targets.as_ref().expect("begin precedes present");
        let look = crate::look::active();
        if self.enabled && look.bloom > 0.0 {
            pass(&t.scene.texture, &t.bloom, &self.extract);
            self.blur
                .set_uniform("Step", vec2(1.0 / t.ping.texture.width(), 0.0));
            pass(&t.bloom.texture, &t.ping, &self.blur);
            self.blur
                .set_uniform("Step", vec2(0.0, 1.0 / t.bloom.texture.height()));
            pass(&t.ping.texture, &t.bloom, &self.blur);
            self.blur
                .set_uniform("Step", vec2(1.0 / t.wide_ping.texture.width(), 0.0));
            pass(&t.bloom.texture, &t.wide_ping, &self.blur);
            self.blur
                .set_uniform("Step", vec2(0.0, 1.0 / t.wide.texture.height()));
            pass(&t.wide_ping.texture, &t.wide, &self.blur);
        }
        set_default_camera();
        clear_background(BLACK);
        self.combine.set_texture("Bloom", t.bloom.texture.clone());
        self.combine.set_texture("Wide", t.wide.texture.clone());
        self.combine
            .set_uniform("Strength", if self.enabled { look.bloom } else { 0.0f32 });
        self.combine.set_uniform("Vignette", look.vignette);
        gl_use_material(&self.combine);
        image(&t.scene.texture, vec2(screen_width(), screen_height()));
        gl_use_default_material();
    }
}

fn image(texture: &Texture2D, size: Vec2) {
    draw_texture_ex(
        texture,
        0.0,
        0.0,
        WHITE,
        DrawTextureParams {
            dest_size: Some(size),
            flip_y: true,
            ..Default::default()
        },
    );
}

fn pass(source: &Texture2D, target: &RenderTarget, material: &Material) {
    let size = vec2(target.texture.width(), target.texture.height());
    let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, size.x, size.y));
    cam.render_target = Some(target.clone());
    set_camera(&cam);
    clear_background(BLACK);
    gl_use_material(material);
    image(source, size);
}
