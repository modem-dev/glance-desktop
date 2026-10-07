//! Image entrances are a separate track from the looping backdrop.
use image::{Rgba, RgbaImage};
use std::borrow::Cow;
#[cfg(target_os = "macos")]
mod gpu;
#[cfg(target_os = "linux")]
#[path = "entrance/linux.rs"]
mod gpu;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entrance {
    #[default]
    None,
    Diagonal,
    Pop,
    Tilt,
}
impl Entrance {
    pub const ALL: [Self; 4] = [Self::None, Self::Diagonal, Self::Pop, Self::Tilt];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Diagonal => "Diagonal reveal",
            Self::Pop => "Spring pop",
            Self::Tilt => "3D settle",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImageAnimation {
    pub effect: Entrance,
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub seconds: u32,
    pub exit: bool,
}
impl Default for ImageAnimation {
    fn default() -> Self {
        Self {
            effect: Entrance::None,
            duration_ms: 1000,
            delay_ms: 200,
            seconds: 5,
            exit: false,
        }
    }
}
impl ImageAnimation {
    pub fn enabled(self) -> bool {
        self.effect != Entrance::None
    }
    pub fn validate(self) -> Result<(), String> {
        if !(200..=2000).contains(&self.duration_ms)
            || self.delay_ms > 1000
            || !(2..=15).contains(&self.seconds)
        {
            return Err(
                "Animation duration must be 0.2–2 s, delay 0–1 s, and clip length 2–15 s".into(),
            );
        }
        if self.enabled()
            && self.delay_ms + self.duration_ms + if self.exit { self.exit_ms() } else { 0 }
                > self.seconds * 1000
        {
            return Err("Lengthen the clip to fit the entrance, delay, and exit".into());
        }
        Ok(())
    }
    fn exit_ms(self) -> u32 {
        self.duration_ms.min(800)
    }
    pub fn progress(self, seconds: f32) -> f32 {
        if !self.enabled() {
            return 1.;
        }
        let enter =
            ((seconds * 1000. - self.delay_ms as f32) / self.duration_ms as f32).clamp(0., 1.);
        if self.exit {
            let remaining =
                ((self.seconds as f32 - seconds) * 1000. / self.exit_ms() as f32).clamp(0., 1.);
            enter.min(remaining)
        } else {
            enter
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationControl {
    Duration,
    Delay,
    Length,
    Time,
}
impl AnimationControl {
    pub fn label(self) -> &'static str {
        match self {
            Self::Duration => "Entrance duration",
            Self::Delay => "Delay",
            Self::Length => "Clip length",
            Self::Time => "Preview time",
        }
    }
    pub fn range(self, a: ImageAnimation) -> (u32, u32, u32) {
        match self {
            Self::Duration => (200, 2000, 100),
            Self::Delay => (0, 1000, 100),
            Self::Length => (2, 15, 1),
            Self::Time => (0, a.seconds * 1000, 1),
        }
    }
    pub fn value(self, a: ImageAnimation, time: f32) -> u32 {
        match self {
            Self::Duration => a.duration_ms,
            Self::Delay => a.delay_ms,
            Self::Length => a.seconds,
            Self::Time => (time * 1000.).round() as u32,
        }
    }
    pub fn set(self, a: &mut ImageAnimation, value: u32) {
        match self {
            Self::Duration => a.duration_ms = value,
            Self::Delay => a.delay_ms = value,
            Self::Length => a.seconds = value,
            Self::Time => {}
        }
    }
    pub fn display(self, value: u32) -> String {
        if self == Self::Length {
            format!("{value} s")
        } else {
            format!("{:.1} s", value as f32 / 1000.)
        }
    }
}

/// Sample in premultiplied alpha so transparent texels cannot tint transformed edges.
fn bilinear(image: &RgbaImage, x: f32, y: f32) -> Rgba<u8> {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let tx = x - ix as f32;
    let ty = y - iy as f32;
    let mut alpha = 0.;
    let mut rgb = [0.; 3];
    for (dx, dy, weight) in [
        (0, 0, (1. - tx) * (1. - ty)),
        (1, 0, tx * (1. - ty)),
        (0, 1, (1. - tx) * ty),
        (1, 1, tx * ty),
    ] {
        let (sx, sy) = (ix + dx, iy + dy);
        if sx < 0 || sy < 0 || sx >= image.width() as i32 || sy >= image.height() as i32 {
            continue;
        }
        let p = image.get_pixel(sx as u32, sy as u32);
        let a = p[3] as f32 * weight;
        alpha += a;
        for c in 0..3 {
            rgb[c] += p[c] as f32 * a;
        }
    }
    if alpha < 0.5 {
        return Rgba([0; 4]);
    }
    Rgba([
        (rgb[0] / alpha).round() as u8,
        (rgb[1] / alpha).round() as u8,
        (rgb[2] / alpha).round() as u8,
        alpha.round() as u8,
    ])
}

/// Inverse planar projection, centered on the screenshot. Shadow shares the plane.
struct Projection {
    inverse: [[f32; 3]; 3],
    center: (f32, f32),
}
impl Projection {
    fn new(effect: Entrance, p: f32, bounds: (f32, f32, f32, f32)) -> Self {
        let ease = 1. - (1. - p).powi(3);
        let scale = if effect == Entrance::Pop {
            let spring = 1. + 2.70158 * (p - 1.).powi(3) + 1.70158 * (p - 1.).powi(2);
            0.68 + 0.32 * spring
        } else {
            0.72 + 0.28 * ease
        };
        let angle = |degrees: f32| {
            if effect == Entrance::Tilt {
                (degrees * (1. - ease)).to_radians()
            } else {
                0.
            }
        };
        let (sy, cy) = angle(68.).sin_cos();
        let (sx, cx) = angle(-14.).sin_cos();
        let (sr, cr) = angle(-7.).sin_cos();
        let distance = bounds.2.max(bounds.3) * 2.5;
        let m = [
            [scale * (cr * cy - sr * sx * sy), -scale * sr * cx, 0.],
            [scale * (sr * cy + cr * sx * sy), scale * cr * cx, 0.],
            [scale * cx * sy / distance, -scale * sx / distance, 1.],
        ];
        // The last column is (0,0,1), so only the upper 2x2 needs inversion.
        let determinant = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        let a = m[1][1] / determinant;
        let b = -m[0][1] / determinant;
        let c = -m[1][0] / determinant;
        let d = m[0][0] / determinant;
        Self {
            inverse: [
                [a, b, 0.],
                [c, d, 0.],
                [-m[2][0] * a - m[2][1] * c, -m[2][0] * b - m[2][1] * d, 1.],
            ],
            center: (bounds.0 + bounds.2 / 2., bounds.1 + bounds.3 / 2.),
        }
    }
    fn source(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let (x, y) = (x - self.center.0, y - self.center.1);
        let w = self.inverse[2][0] * x + self.inverse[2][1] * y + 1.;
        if w <= 0.01 {
            return None;
        }
        Some((
            (self.inverse[0][0] * x + self.inverse[0][1] * y) / w + self.center.0 - 0.5,
            (self.inverse[1][0] * x + self.inverse[1][1] * y) / w + self.center.1 - 0.5,
        ))
    }
}

pub(super) fn foreground<'a>(
    cached: &'a RgbaImage,
    animation: ImageAnimation,
    bounds: (f32, f32, f32, f32),
    seconds: f32,
) -> Cow<'a, RgbaImage> {
    let p = animation.progress(seconds);
    if p >= 1. {
        return Cow::Borrowed(cached);
    }
    if p <= 0. {
        return Cow::Owned(RgbaImage::new(cached.width(), cached.height()));
    }
    let out = if animation.effect == Entrance::Diagonal {
        let threshold = 2. * p * p * (3. - 2. * p);
        // A one-pixel signed-distance ramp gives the reveal edge antialiasing.
        let edge = (bounds.2.recip().powi(2) + bounds.3.recip().powi(2)).sqrt();
        RgbaImage::from_fn(cached.width(), cached.height(), |x, y| {
            let mut pixel = *cached.get_pixel(x, y);
            if pixel[3] > 0 {
                let local =
                    (x as f32 + 0.5 - bounds.0) / bounds.2 + (y as f32 + 0.5 - bounds.1) / bounds.3;
                pixel[3] = (pixel[3] as f32 * ((threshold - local) / edge + 0.5).clamp(0., 1.))
                    .round() as u8;
            }
            pixel
        })
    } else {
        let projection = Projection::new(animation.effect, p, bounds);
        let opacity = (p * if animation.effect == Entrance::Pop {
            5.
        } else {
            4.
        })
        .min(1.);
        RgbaImage::from_fn(cached.width(), cached.height(), |x, y| {
            let mut pixel = projection
                .source(x as f32 + 0.5, y as f32 + 0.5)
                .map_or(Rgba([0; 4]), |(sx, sy)| bilinear(cached, sx, sy));
            pixel[3] = (pixel[3] as f32 * opacity).round() as u8;
            pixel
        })
    };
    Cow::Owned(out)
}

pub(super) fn render_foreground(
    cached: &std::sync::Arc<RgbaImage>,
    animation: ImageAnimation,
    bounds: (f32, f32, f32, f32),
    seconds: f32,
) -> Cow<'_, RgbaImage> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let p = animation.progress(seconds)
        && p > 0.
        && p < 1.
    {
        match gpu::frame(cached, animation.effect, bounds, p) {
            Ok(image) => return Cow::Owned(image),
            Err(error) => {
                static WARNING: std::sync::Once = std::sync::Once::new();
                WARNING
                    .call_once(|| eprintln!("Image animation shader: {error}; using CPU fallback"));
            }
        }
    }
    foreground(cached, animation, bounds, seconds)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) fn gpu_preview_warmup(
    source: &std::sync::Arc<image::RgbaImage>,
    effect: Entrance,
    bounds: (f32, f32, f32, f32),
) -> Result<(), String> {
    gpu::frame(source, effect, bounds, 0.5).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timing_holds_and_optional_exit_returns_to_hidden() {
        let a = ImageAnimation {
            effect: Entrance::Diagonal,
            ..Default::default()
        };
        assert_eq!(a.progress(0.), 0.);
        assert!((a.progress(0.7) - 0.5).abs() < 0.001);
        assert_eq!(a.progress(1.2), 1.);
        assert_eq!(a.progress(5.), 1.);
        let a = ImageAnimation { exit: true, ..a };
        assert_eq!(a.progress(4.), 1.);
        assert!((a.progress(4.6) - 0.5).abs() < 0.001);
        assert_eq!(a.progress(5.), 0.);
        assert!(
            ImageAnimation {
                seconds: 2,
                duration_ms: 2000,
                delay_ms: 1000,
                ..a
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn transformed_edges_do_not_pick_up_transparent_rgb() {
        let mut source = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 0]));
        source.put_pixel(0, 0, Rgba([0, 180, 90, 255]));
        assert_eq!(bilinear(&source, 0.5, 0.), Rgba([0, 180, 90, 128]));
    }
    #[test]
    fn diagonal_sweeps_across_image_and_settles_exactly() {
        let source = RgbaImage::from_pixel(20, 20, Rgba([80, 100, 120, 255]));
        let a = ImageAnimation {
            effect: Entrance::Diagonal,
            ..Default::default()
        };
        let half = foreground(&source, a, (0., 0., 20., 20.), 0.7);
        assert_eq!(half.get_pixel(2, 2)[3], 255);
        assert_eq!(half.get_pixel(17, 17)[3], 0);
        assert_eq!(&*foreground(&source, a, (0., 0., 20., 20.), 2.), &source);
    }
}
