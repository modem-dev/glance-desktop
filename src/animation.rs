//! Looping backdrops. Shader effects share a Metal renderer; foreground stays cached.
use crate::backdrop::Backdrop;
use gpui::{
    Bounds, Pixels, Rgba, Window, linear_color_stop, linear_gradient, point, px, quad, rgb, size,
};
use image::{Pixel, RgbaImage};
use std::f32::consts::TAU;
mod entrance;
pub use entrance::{AnimationControl, Entrance, ImageAnimation};
mod composition_preview;
pub use composition_preview::CompositionPreview;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    #[default]
    Still,
    Flow,
    Lava,
    #[serde(alias = "stars")]
    Nebula,
    Paint,
    Liquid,
    Aurora,
    Contours,
    Prism,
}
impl Motion {
    pub const EFFECTS: [Self; 8] = [
        Self::Flow,
        Self::Nebula,
        Self::Aurora,
        Self::Contours,
        Self::Paint,
        Self::Prism,
        Self::Liquid,
        Self::Lava,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Still => "Still",
            Self::Flow => "Flow",
            Self::Lava => "Lava",
            Self::Nebula => "Nebula",
            Self::Paint => "Painterly",
            Self::Liquid => "Liquid",
            Self::Aurora => "Aurora",
            Self::Contours => "Contours",
            Self::Prism => "Prism",
        }
    }
    pub fn uses_shader(self) -> bool {
        !matches!(self, Self::Still | Self::Flow)
    }
    pub fn suggested_preset(self) -> Option<usize> {
        match self {
            Self::Liquid => Some(1),
            Self::Lava | Self::Paint => Some(3),
            Self::Aurora => Some(0),
            Self::Contours => Some(6),
            Self::Prism => Some(2),
            _ => None,
        }
    }
}
#[derive(Clone, Copy)]
struct Disc {
    x: f32,
    y: f32,
    r: f32,
    color: [u8; 3],
    alpha: f32,
    soft: bool,
}
struct Scene {
    top: [u8; 3],
    bottom: [u8; 3],
    discs: Vec<Disc>,
}
fn color(hex: u32) -> [u8; 3] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}
fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    std::array::from_fn(|i| (a[i] as f32 * (1. - t) + b[i] as f32 * t).round() as u8)
}
fn hex(c: [u8; 3]) -> u32 {
    (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
}
fn random(i: usize) -> f32 {
    let mut x = (i as u32).wrapping_mul(747796405).wrapping_add(2891336453);
    x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277803737);
    ((x >> 22) ^ x) as f32 / u32::MAX as f32
}
fn scene(b: Backdrop, phase: f32) -> Scene {
    let phase = phase.rem_euclid(1.);
    let p = phase * TAU
        + if b.seed == 0 {
            0.
        } else {
            random(b.seed as usize) * TAU
        };
    let random = |i: usize| random(i ^ b.seed as usize);
    let [a, z] = b.colors();
    let a = color(a);
    let z = color(z);
    let cream = [255, 234, 211];
    let accent = mix(a, cream, 0.55);
    let mut s = Scene {
        top: a,
        bottom: if b.gradient { z } else { a },
        discs: vec![],
    };
    match b.motion {
        Motion::Still
        | Motion::Liquid
        | Motion::Lava
        | Motion::Paint
        | Motion::Aurora
        | Motion::Contours
        | Motion::Prism => {}
        Motion::Flow => {
            let wave = p.sin() * 0.5 + 0.5;
            s.top = mix(a, z, wave * 0.6);
            s.bottom = mix(z, accent, (p + 1.8).sin() * 0.25 + 0.3);
            for i in 0..9 {
                let q = random(i * 7 + 1) * TAU;
                let speed = if i % 3 == 0 { 2. } else { 1. };
                let x = 0.5 + 0.51 * (p * speed + q).sin();
                let y = 0.5 + 0.51 * (p + q * 1.7).cos();
                let r = 0.16 + random(i * 7 + 2) * 0.24;
                let c = [a, z, accent, cream][i % 4];
                s.discs.push(Disc {
                    x,
                    y,
                    r,
                    color: c,
                    alpha: 0.55,
                    soft: true,
                });
            }
        }
        Motion::Nebula => {
            s.top = [7, 12, 29];
            s.bottom = [19, 29, 55];
            // The worker-rendered nebula supplies the colored background;
            // keep the existing crisp drifting/twinkling stars above it.
            for i in 0..110 {
                let x = random(i * 7 + 200);
                let y = (random(i * 7 + 201) + phase).rem_euclid(1.);
                let edge = (y * 20.).min((1. - y) * 20.).clamp(0., 1.);
                let alpha =
                    (0.45 + 0.4 * (p * (1. + (i % 3) as f32) + random(i + 70) * TAU).sin()) * edge;
                s.discs.push(Disc {
                    x: x + 0.012 * (p + random(i + 50) * TAU).sin(),
                    y,
                    r: 0.0012 + random(i * 7 + 202) * 0.0023,
                    color: [231, 241, 255],
                    alpha,
                    soft: false,
                });
            }
        }
    }
    s
}
// Match GPUI 0.2.2's four-sample Gaussian shadow integration (Apache-2.0).
// Cache normalized coverage so exports do no per-pixel exponentials.
fn soft_coverage(x: f32, y: f32) -> f32 {
    const N: usize = 256;
    static TABLE: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    let x = x.abs();
    let y = y.abs();
    if x >= 1. || y >= 1. {
        return 0.;
    }
    let table = TABLE.get_or_init(|| {
        let erf = |v: f32| {
            let a = v.abs();
            let r = 1. + (0.278393 + (0.230389 + (0.000972 + 0.078108 * a) * a) * a) * a;
            v.signum() * (1. - 1. / r.powi(4))
        };
        let sigma = 0.18;
        let core = 0.45;
        let mut values = vec![0.; N * N];
        for yi in 0..N {
            for xi in 0..N {
                let x = xi as f32 / (N - 1) as f32;
                let y = yi as f32 / (N - 1) as f32;
                let low = y - core;
                let high = y + core;
                let start = (-3_f32 * sigma).clamp(low, high);
                let end = (3_f32 * sigma).clamp(low, high);
                let step = (end - start) / 4.;
                let mut alpha = 0.;
                for i in 0..4 {
                    let sample = start + step * (i as f32 + 0.5);
                    let delta = -(y - sample).abs();
                    let curved = (core * core - delta * delta).max(0.).sqrt();
                    let integral = 0.5
                        * (erf((x + curved) * std::f32::consts::FRAC_1_SQRT_2 / sigma)
                            - erf((x - curved) * std::f32::consts::FRAC_1_SQRT_2 / sigma));
                    let gaussian = (-(sample * sample) / (2. * sigma * sigma)).exp()
                        / ((2. * std::f32::consts::PI).sqrt() * sigma);
                    alpha += integral * gaussian * step;
                }
                values[yi * N + xi] = alpha.clamp(0., 1.);
            }
        }
        values
    });
    let x = x * (N - 1) as f32;
    let y = y * (N - 1) as f32;
    let xi = (x as usize).min(N - 2);
    let yi = (y as usize).min(N - 2);
    let tx = x - xi as f32;
    let ty = y - yi as f32;
    let top = table[yi * N + xi] * (1. - tx) + table[yi * N + xi + 1] * tx;
    let bottom = table[(yi + 1) * N + xi] * (1. - tx) + table[(yi + 1) * N + xi + 1] * tx;
    top * (1. - ty) + bottom * ty
}
mod preview;
pub use preview::Preview;
pub fn paint(
    b: Backdrop,
    phase: f32,
    bounds: Bounds<Pixels>,
    radius: Pixels,
    preview: &mut Preview,
    playing: bool,
    window: &mut Window,
) {
    if b.motion.uses_shader() {
        if b.motion == Motion::Nebula {
            // Keep space dark while the first worker frame is being prepared.
            window.paint_quad(quad(
                bounds,
                px(0.),
                rgb(0x070c1d),
                px(0.),
                rgb(0),
                Default::default(),
            ));
        }
        preview.paint(b, phase, bounds, radius, playing, window);
        if b.motion != Motion::Nebula {
            return;
        }
    }
    let s = scene(b, phase);
    if !b.motion.uses_shader() {
        window.paint_quad(quad(
            bounds,
            px(0.),
            linear_gradient(
                180.,
                linear_color_stop(rgb(hex(s.top)), 0.),
                linear_color_stop(rgb(hex(s.bottom)), 1.),
            ),
            px(0.),
            rgb(0),
            Default::default(),
        ));
    }
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let unit = w.min(h);
    for d in s.discs {
        let r = d.r * unit;
        let c = rgb(hex(d.color));
        if d.soft {
            // GPUI's Metal shadow shader supplies continuous Gaussian blur.
            let core = r * 0.45;
            window.paint_shadows(
                Bounds::new(
                    bounds.origin + point(px(d.x * w - core), px(d.y * h - core)),
                    size(px(core * 2.), px(core * 2.)),
                ),
                px(core).into(),
                &[gpui::BoxShadow {
                    color: Rgba { a: d.alpha, ..c }.into(),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(r * 0.18),
                    spread_radius: px(0.),
                }],
            );
            continue;
        }
        window.paint_quad(quad(
            Bounds::new(
                bounds.origin + point(px(d.x * w - r), px(d.y * h - r)),
                size(px(r * 2.), px(r * 2.)),
            ),
            px(r),
            Rgba { a: d.alpha, ..c },
            px(0.),
            c,
            Default::default(),
        ));
    }
}

/// A constant foreground/shadow layer, reused by every exported frame.
pub struct Renderer {
    pub width: u32,
    pub height: u32,
    b: Backdrop,
    foreground: std::sync::Arc<RgbaImage>,
    image_animation: ImageAnimation,
    image_bounds: (f32, f32, f32, f32),
    transparent_background: bool,
}
impl Renderer {
    pub fn new(source: &RgbaImage, mut b: Backdrop, max_edge: Option<u32>) -> Self {
        let dimensions = b.dimensions(source.dimensions());
        let source = b.extend_edges(source);
        let (w, h) = max_edge.map_or(dimensions, |cap| b.format.video_dimensions(dimensions, cap));
        let scale = (w as f32 / dimensions.0 as f32)
            .min(h as f32 / dimensions.1 as f32)
            .min(1.);
        let source = if scale < 1. {
            std::borrow::Cow::Owned(crate::enhance::resize(
                &source,
                (
                    (source.width() as f32 * scale).round().max(1.) as u32,
                    (source.height() as f32 * scale).round().max(1.) as u32,
                ),
                false,
            ))
        } else {
            source
        };
        b.padding = (b.padding as f32 * scale).round() as u32;
        b.inner_radius = (b.inner_radius as f32 * scale).round() as u32;
        b.shadow = (b.shadow as f32 * scale).round() as u32;
        let mut foreground = RgbaImage::new(w, h);
        let frame = crate::backdrop::Frame::centered((w, h), source.dimensions());
        let (left, top) = frame.origin;
        let (left_f, top_f) = (left as f32, top as f32);
        let sw = source.width() as f32;
        let sh = source.height() as f32;
        for (x, y, p) in foreground.enumerate_pixels_mut() {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            if b.shadow > 0 {
                let blur = b.shadow as f32;
                let d = crate::backdrop::distance(
                    px - left_f,
                    py - top_f - blur * 0.25,
                    sw,
                    sh,
                    b.inner_radius as f32,
                )
                .max(0.);
                *p = image::Rgba([
                    0,
                    0,
                    0,
                    (0.22 * (-2. * (d / (blur * 0.6)).powi(2)).exp() * 255.).round() as u8,
                ]);
            }
            if x >= left && y >= top && x < left + source.width() && y < top + source.height() {
                let mut src = *source.get_pixel(x - left, y - top);
                src[3] = (src[3] as f32
                    * crate::backdrop::coverage(crate::backdrop::distance(
                        px - left_f,
                        py - top_f,
                        sw,
                        sh,
                        b.inner_radius as f32,
                    )))
                .round() as u8;
                p.blend(&src);
            }
        }
        Self {
            width: w,
            height: h,
            b,
            foreground: std::sync::Arc::new(foreground),
            image_animation: ImageAnimation::default(),
            image_bounds: (left_f, top_f, sw, sh),
            transparent_background: false,
        }
    }
    pub fn with_animation(
        source: &RgbaImage,
        b: Backdrop,
        max_edge: Option<u32>,
        animation: ImageAnimation,
    ) -> Self {
        let mut renderer = Self::new(source, b, max_edge);
        renderer.image_animation = animation;
        renderer
    }
    pub fn for_document(document: &crate::document::Document, max_edge: Option<u32>) -> Self {
        let mut renderer = Self::with_animation(
            &document.render(None),
            document.animation_backdrop(),
            max_edge,
            document.image_animation,
        );
        renderer.transparent_background = document.backdrop.is_none();
        renderer
    }
    fn prepare_preview(&self) {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if self.image_animation.enabled() {
            // Warm the GPU's first dispatch behind the loading overlay.
            let _ = entrance::gpu_preview_warmup(
                &self.foreground,
                self.image_animation.effect,
                self.image_bounds,
            );
        }
    }
    pub fn frame(&self, phase: f32) -> RgbaImage {
        let foreground = entrance::render_foreground(
            &self.foreground,
            self.image_animation,
            self.image_bounds,
            phase * self.image_animation.seconds as f32,
        );
        let s = scene(self.b, phase);
        let mut out = if self.b.motion.uses_shader() {
            crate::motion_shader::frame_with_colors(
                self.width,
                self.height,
                self.b.preset,
                self.b.motion,
                phase,
                self.b.colors,
                self.b.seed,
            )
        } else {
            RgbaImage::new(self.width, self.height)
        };
        if !self.b.motion.uses_shader() && !self.transparent_background {
            for y in 0..self.height {
                let c = mix(s.top, s.bottom, (y as f32 + 0.5) / self.height as f32);
                for x in 0..self.width {
                    let fg = foreground.get_pixel(x, y);
                    out.put_pixel(
                        x,
                        y,
                        if fg[3] == 255 {
                            *fg
                        } else {
                            image::Rgba([c[0], c[1], c[2], 255])
                        },
                    );
                }
            }
        }
        let unit = self.width.min(self.height) as f32;
        for d in s.discs {
            let r = d.r * unit;
            let cx = d.x * self.width as f32;
            let cy = d.y * self.height as f32;
            let x0 = (cx - r - 1.).max(0.) as u32;
            let y0 = (cy - r - 1.).max(0.) as u32;
            let x1 = (cx + r + 1.).max(0.).min(self.width as f32) as u32;
            let y1 = (cy + r + 1.).max(0.).min(self.height as f32) as u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    if foreground.get_pixel(x, y)[3] == 255 {
                        continue;
                    }
                    let distance = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy);
                    let coverage = if d.soft {
                        soft_coverage((x as f32 + 0.5 - cx) / r, (y as f32 + 0.5 - cy) / r)
                    } else {
                        (r + 0.5 - distance).clamp(0., 1.)
                    };
                    let alpha = (d.alpha * coverage * 255.).round() as u8;
                    if alpha > 0 {
                        out.get_pixel_mut(x, y)
                            .blend(&image::Rgba([d.color[0], d.color[1], d.color[2], alpha]));
                    }
                }
            }
        }
        for (p, fg) in out.pixels_mut().zip(foreground.pixels()) {
            match fg[3] {
                0 => {}
                255 => *p = *fg,
                _ => p.blend(fg),
            }
            if !self.transparent_background {
                p[3] = 255;
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "renders synthetic Nebula samples and a looping GIF for local review"]
    fn nebula_visual_qa() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/nebula-qa");
        std::fs::create_dir_all(&dir).unwrap();
        let backdrop = Backdrop {
            motion: Motion::Nebula,
            preset: 2,
            padding: 0,
            inner_radius: 0,
            shadow: 0,
            ..Default::default()
        };
        for (name, w, h) in [("landscape", 960, 540), ("portrait", 405, 720)] {
            Renderer::new(&RgbaImage::new(w, h), backdrop, None)
                .frame(0.22)
                .save(dir.join(format!("{name}.png")))
                .unwrap();
        }
        let mut samples = RgbaImage::new(960, 540);
        for preset in 0..8 {
            let image = Renderer::new(
                &RgbaImage::new(240, 270),
                Backdrop { preset, ..backdrop },
                None,
            )
            .frame(0.22);
            image::imageops::replace(
                &mut samples,
                &image,
                (preset % 4 * 240) as i64,
                (preset / 4 * 270) as i64,
            );
        }
        samples.save(dir.join("palettes.png")).unwrap();
        let renderer = Renderer::new(&RgbaImage::new(480, 270), backdrop, None);
        let file = std::fs::File::create(dir.join("nebula.gif")).unwrap();
        let mut encoder = gif::Encoder::new(file, 480, 270, &[]).unwrap();
        encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        for i in 0..100 {
            let mut pixels = renderer.frame(i as f32 / 100.).into_raw();
            let mut frame = gif::Frame::from_rgba_speed(480, 270, &mut pixels, 10);
            frame.delay = 5;
            encoder.write_frame(&frame).unwrap();
        }
        let renderer = Renderer::new(&RgbaImage::new(960, 540), backdrop, None);
        let start = std::time::Instant::now();
        for i in 0..60 {
            renderer.frame(i as f32 / 60.);
        }
        println!(
            "Nebula: {:.2} ms/frame (nebula compute/readback plus star composition)",
            start.elapsed().as_secs_f64() * 1000. / 60.
        );
    }

    #[test]
    fn every_image_entrance_composes_with_all_backdrops_and_holds_exactly() {
        let source = RgbaImage::from_pixel(64, 40, image::Rgba([230, 40, 70, 210]));
        let empty = RgbaImage::new(64, 40);
        for motion in std::iter::once(Motion::Still).chain(Motion::EFFECTS) {
            let b = Backdrop {
                motion,
                padding: 14,
                inner_radius: 7,
                inside_padding: 9,
                shadow: 8,
                ..Default::default()
            };
            let static_renderer = Renderer::new(&source, b, None);
            let background = Renderer::new(&empty, Backdrop { shadow: 0, ..b }, None);
            for effect in [Entrance::Diagonal, Entrance::Pop, Entrance::Tilt] {
                let animation = ImageAnimation {
                    effect,
                    exit: true,
                    ..Default::default()
                };
                let renderer = Renderer::with_animation(&source, b, None, animation);
                assert_eq!(
                    renderer.frame(0.),
                    background.frame(0.),
                    "{effect:?} {motion:?} starts on background alone"
                );
                assert_ne!(
                    renderer.frame(0.14),
                    background.frame(0.14),
                    "{effect:?} {motion:?} reveals image"
                );
                assert_eq!(
                    renderer.frame(0.5),
                    static_renderer.frame(0.5),
                    "{effect:?} {motion:?} preserves settled pixels"
                );
                assert_eq!(
                    renderer.frame(1.),
                    background.frame(1.),
                    "{effect:?} {motion:?} exits without ghost shadow"
                );
            }
        }
    }
    #[test]
    fn liquid_export_blends_transparency_and_extends_edges() {
        let source = RgbaImage::from_pixel(20, 12, image::Rgba([250, 80, 30, 128]));
        let b = Backdrop {
            motion: Motion::Liquid,
            preset: 1,
            padding: 8,
            inner_radius: 0,
            inside_padding: 6,
            shadow: 0,
            ..Default::default()
        };
        let renderer = Renderer::new(&source, b, None);
        let output = renderer.frame(0.37);
        let background =
            crate::motion_shader::frame(renderer.width, renderer.height, 1, Motion::Liquid, 0.37);
        let mut expected = *background.get_pixel(24, 20);
        expected.blend(source.get_pixel(10, 6));
        // The backdrop stays opaque when compositing translucent image pixels.
        expected[3] = 255;
        assert_eq!(*output.get_pixel(24, 20), expected);
        assert_eq!(output.get_pixel(0, 0)[3], 255);
        assert_eq!(output.get_pixel(24, 20)[3], 255);
        assert_eq!(output.get_pixel(18, 1), background.get_pixel(18, 1));
    }
    #[test]
    fn all_effects_loop_and_foreground_is_unchanged() {
        let source = RgbaImage::from_pixel(80, 50, image::Rgba([20, 40, 60, 255]));
        for motion in Motion::EFFECTS {
            let r = Renderer::new(
                &source,
                Backdrop {
                    motion,
                    padding: 20,
                    inner_radius: 0,
                    shadow: 0,
                    ..Default::default()
                },
                None,
            );
            let a = r.frame(0.);
            let b = r.frame(0.37);
            let end = r.frame(1.);
            assert_eq!(a, end, "{} must loop", motion.label());
            assert_ne!(a, b);
            for y in 20..70 {
                for x in 20..100 {
                    assert_eq!(a.get_pixel(x, y), b.get_pixel(x, y));
                    assert_eq!(b.get_pixel(x, y), source.get_pixel(x - 20, y - 20));
                }
            }
        }
    }
    #[test]
    fn seeded_effects_are_repeatable_loop_and_preserve_the_foreground() {
        let source = RgbaImage::from_pixel(40, 30, image::Rgba([20, 40, 60, 255]));
        for motion in Motion::EFFECTS {
            for (padding, format) in [
                (20, crate::backdrop::Format::Widescreen),
                (20, crate::backdrop::Format::Portrait),
            ] {
                let original = Renderer::new(
                    &source,
                    Backdrop {
                        motion,
                        padding,
                        format,
                        inner_radius: 0,
                        shadow: 0,
                        ..Default::default()
                    },
                    None,
                )
                .frame(0.37);
                for seed in [1, 42, u32::MAX] {
                    let renderer = Renderer::new(
                        &source,
                        Backdrop {
                            motion,
                            padding,
                            format,
                            seed,
                            inner_radius: 0,
                            shadow: 0,
                            ..Default::default()
                        },
                        None,
                    );
                    let frame = renderer.frame(0.37);
                    assert_eq!(frame, renderer.frame(0.37), "{motion:?}, seed {seed}");
                    assert_eq!(
                        renderer.frame(0.),
                        renderer.frame(1.),
                        "{motion:?}, seed {seed}"
                    );
                    assert_ne!(
                        original, frame,
                        "{motion:?} seed must change the composition"
                    );
                    let origin =
                        crate::backdrop::Frame::centered(frame.dimensions(), source.dimensions())
                            .origin;
                    for y in 0..source.height() {
                        for x in 0..source.width() {
                            assert_eq!(
                                frame.get_pixel(x + origin.0, y + origin.1),
                                source.get_pixel(x, y)
                            );
                        }
                    }
                }
            }
        }
        let legacy: Backdrop =
            serde_json::from_value(serde_json::json!({"motion":"lava"})).unwrap();
        assert_eq!(legacy.seed, 0);
    }

    #[test]
    fn video_dimensions_are_even_and_bounded() {
        let source = RgbaImage::new(3001, 1733);
        let r = Renderer::new(&source, Backdrop::default(), Some(1920));
        assert!(r.width <= 1920 && r.height <= 1920);
        assert_eq!(r.width % 2, 0);
        assert_eq!(r.height % 2, 0);
    }
}

#[cfg(test)]
mod continuity_tests {
    use super::*;
    #[test]
    fn loop_seam_is_a_normal_animation_step() {
        let source = RgbaImage::from_pixel(40, 30, image::Rgba([80, 120, 160, 255]));
        for effect in Motion::EFFECTS {
            let b = Backdrop {
                motion: effect,
                padding: 20,
                inner_radius: 0,
                ..Default::default()
            };
            let renderer = Renderer::new(&source, b, None);
            let frames: Vec<_> = (0..20).map(|i| renderer.frame(i as f32 / 20.)).collect();
            let distance = |a: &RgbaImage, b: &RgbaImage| {
                a.as_raw()
                    .iter()
                    .zip(b.as_raw())
                    .map(|(a, b)| (*a as f32 - *b as f32).abs())
                    .sum::<f32>()
                    / a.as_raw().len() as f32
            };
            let max_step = frames
                .windows(2)
                .map(|f| distance(&f[0], &f[1]))
                .fold(0., f32::max);
            let seam = distance(frames.last().unwrap(), &frames[0]);
            assert!(
                seam <= max_step * 1.5 + 0.1,
                "{} seam={seam} max_step={max_step}",
                effect.label()
            );
            assert_eq!(renderer.frame(0.), renderer.frame(1.));
        }
    }
}
