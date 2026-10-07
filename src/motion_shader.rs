//! Procedural backdrops. Metal/Vulkan handle preview/export; CPU is a fallback.
use crate::{animation::Motion, backdrop::PRESETS};
use image::RgbaImage;
use std::f32::consts::TAU;

#[repr(C)]
struct Uniforms {
    width: u32,
    height: u32,
    orbit_sin: f32,
    orbit_cos: f32,
    dark: [f32; 3],
    blue: [f32; 3],
    cyan: [f32; 3],
    mint: [f32; 3],
    effect: u32,
    seed: u32,
    offset: [f32; 2],
}
impl Uniforms {
    fn new(width: u32, height: u32, preset: usize, motion: Motion, phase: f32) -> Self {
        let rgb = |hex: u32| {
            [
                ((hex >> 16) & 255) as f32 / 255.,
                ((hex >> 8) & 255) as f32 / 255.,
                (hex & 255) as f32 / 255.,
            ]
        };
        let (dark, blue, cyan, mint) = if motion == Motion::Lava && preset == 3 {
            (rgb(0x18080d), rgb(0xc32a1e), rgb(0xff7b23), rgb(0xffe4a0))
        } else if preset == 1 {
            // The reference's navy / electric blue / cyan / mint palette.
            (rgb(0x000329), rgb(0x0060f6), rgb(0x008fff), rgb(0x80ffdb))
        } else {
            let (_, a, z) = PRESETS[preset];
            let a = rgb(a);
            let z = rgb(z);
            (
                std::array::from_fn(|i| z[i] * 0.10),
                z,
                a,
                std::array::from_fn(|i| a[i] * 0.45 + 0.55),
            )
        };
        let (orbit_sin, orbit_cos) = (phase.rem_euclid(1.) * TAU).sin_cos();
        Self {
            width,
            height,
            orbit_sin,
            orbit_cos,
            dark,
            blue,
            cyan,
            mint,
            effect: match motion {
                Motion::Liquid => 0,
                Motion::Lava => 1,
                Motion::Aurora => 2,
                Motion::Contours => 3,
                Motion::Prism => 4,
                Motion::Paint => 5,
                Motion::Nebula => 6,
                _ => panic!("motion does not use a shader"),
            },
            seed: 0,
            offset: [0., 0.],
        }
    }
    fn seeded(mut self, seed: u32) -> Self {
        self.seed = seed;
        if seed != 0 {
            self.offset = [
                (random_value(seed, 17) - 0.5) * 0.36,
                (random_value(seed, 29) - 0.5) * 0.36,
            ];
            let (s, c) = (random_value(seed, 41) * TAU).sin_cos();
            (self.orbit_sin, self.orbit_cos) = (
                self.orbit_sin * c + self.orbit_cos * s,
                self.orbit_cos * c - self.orbit_sin * s,
            );
        }
        self
    }
}

fn smoothstep(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn grain_hash(x: u32, y: u32) -> u32 {
    let mut n = x
        .wrapping_mul(1973)
        .wrapping_add(y.wrapping_mul(9277))
        .wrapping_add(89173);
    n = (n ^ (n >> 16)).wrapping_mul(2246822519);
    n = (n ^ (n >> 13)).wrapping_mul(3266489917);
    n ^ (n >> 16)
}
fn random_value(x: u32, y: u32) -> f32 {
    (grain_hash(x, y) & 65535) as f32 / 65535.
}
fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] * (1. - t) + b[i] * t)
}
fn paper_noise(x: f32, y: f32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();
    let fx = fx * fx * (3. - 2. * fx);
    let fy = fy * fy * (3. - 2. * fy);
    let a = random_value(ix as u32, iy as u32);
    let b = random_value((ix + 1) as u32, iy as u32);
    let c = random_value(ix as u32, (iy + 1) as u32);
    let d = random_value((ix + 1) as u32, (iy + 1) as u32);
    (a * (1. - fx) + b * fx) * (1. - fy) + (c * (1. - fx) + d * fx) * fy
}
fn nebula_noise(mut x: f32, mut y: f32) -> f32 {
    let mut value = 0.;
    let mut weight = 0.55;
    for _ in 0..4 {
        value += paper_noise(x, y) * weight;
        // Rotate each octave so the wisps do not reveal the noise grid.
        (x, y) = (x * 1.6 - y * 1.2 + 9.1, x * 1.2 + y * 1.6 + 3.7);
        weight *= 0.5;
    }
    value
}
fn nebula(u: &Uniforms, px: f32, py: f32) -> [f32; 3] {
    let x = px * 3.2 + random_value(u.seed, 53) * 19.;
    let y = py * 3.2 + random_value(u.seed, 67) * 19.;
    // Crossing, periodic currents deform the gas locally. Fine turbulence
    // and dark dust lanes break up the broad glow into luminous filaments.
    let wx = nebula_noise(x + 0.38 * u.orbit_cos, y + 0.32 * u.orbit_sin);
    let wy = nebula_noise(x + 5.2 - 0.30 * u.orbit_sin, y + 1.3 + 0.35 * u.orbit_cos);
    let qx = x + (wx - 0.5) * 2.2 + 0.18 * (y * 1.6 + u.orbit_sin).sin();
    let qy = y + (wy - 0.5) * 2.2 + 0.18 * (x * 1.3 + u.orbit_cos).cos();
    let gas = nebula_noise(qx, qy);
    let dust = nebula_noise(qx * 1.7 + 4.8, qy * 1.7 - 2.6);
    let spine = py + px * 0.35 + 0.12 * (px * 3. + u.orbit_sin).sin();
    let envelope = (-spine * spine * 4.5).exp();
    let density = smoothstep(0.22, 0.72, gas) * envelope;
    let lanes = smoothstep(0.30, 0.67, dust);
    let emission = density * (0.18 + 0.82 * lanes);
    let filament =
        (1. - smoothstep(0.018, 0.15, (gas - 0.52).abs())) * density * (0.35 + 0.65 * lanes);
    let tint = smoothstep(0.25, 0.70, wy);
    let gas_color = mix3(u.blue, u.cyan, tint);
    // Keep the cloud emission subdued so the crisp stars lead the scene.
    std::array::from_fn(|i| {
        [0.018, 0.026, 0.060][i]
            + u.blue[i] * 0.025
            + gas_color[i] * emission * 0.50
            + u.mint[i] * filament * 0.12
    })
}
fn lava(u: &Uniforms, px: f32, py: f32) -> [f32; 3] {
    let t = u.orbit_sin.atan2(u.orbit_cos);
    // Advect the entire fluid through crossing currents rather than moving
    // rigid circles. The Jacobian carries surface lighting through the warp.
    let a = py * 5. - t;
    let b = px * 7. + py * 4. + 2. * t;
    let c = px * 4. + t;
    let d = py * 6. - px * 3. - 2. * t;
    let qx = px + 0.10 * a.sin() + 0.035 * b.sin();
    let qy = py + 0.08 * c.sin() + 0.035 * d.cos();
    let jxx = 1. + 0.245 * b.cos();
    let jxy = 0.50 * a.cos() + 0.14 * b.cos();
    let jyx = 0.32 * c.cos() + 0.105 * d.sin();
    let jyy = 1. - 0.21 * d.sin();
    let mut field = 0.;
    let mut gx = 0.;
    let mut gy = 0.;
    let unit = u.width.min(u.height) as f32;
    let aspect_x = u.width as f32 / unit;
    let aspect_y = u.height as f32 / unit;
    for i in 0..8 {
        let angle = i as f32 * 1.8
            + if u.seed == 0 {
                0.
            } else {
                random_value(i, u.seed) * TAU
            };
        let s = (t + angle).sin();
        let c = (t + angle).cos();
        let surge = (2. * t - angle).sin();
        let (cx, cy, r) = if i < 6 {
            (
                (i as f32 / 5. - 0.5) * aspect_x * 0.9 + 0.18 * s + 0.07 * surge,
                aspect_y * 0.34 * c + 0.09 * surge,
                0.145 + 0.025 * (2. * t + angle).sin(),
            )
        } else {
            // Small globules travel between the larger streams and rejoin them.
            (
                aspect_x * 0.40 * (2. * t + angle).sin(),
                aspect_y * 0.42 * c,
                0.065 + 0.015 * surge,
            )
        };
        let tilt = 0.55 * s;
        let (sn, cs) = tilt.sin_cos();
        let stretch = 0.72 + 0.18 * (2. * t + angle).cos();
        let dx = qx - cx;
        let dy = qy - cy;
        let lx = dx * cs + dy * sn;
        let ly = (-dx * sn + dy * cs) * stretch;
        let d = lx * lx + ly * ly + 0.004;
        field += r * r / d;
        let slope = -2. * r * r / (d * d);
        gx += slope * (lx * cs - ly * stretch * sn);
        gy += slope * (lx * sn + ly * stretch * cs);
    }
    let (gx, gy) = (gx * jxx + gy * jyx, gx * jxy + gy * jyy);
    let fill = smoothstep(0.85, 1.1, field);
    let core = smoothstep(1., 2.1, field);
    let shine = 0.4
        + 0.6
            * ((gx * -0.5 + gy * -0.8) / (gx * gx + gy * gy).sqrt().max(0.001) + 0.3).clamp(0., 1.);
    let rim = (-(field - 1.15).powi(2) * 18.).exp() * shine;
    let base = std::array::from_fn(|i| u.dark[i] + u.blue[i] * field.min(0.8) * 0.07);
    mix3(
        base,
        mix3(mix3(u.blue, u.cyan, core), u.mint, rim * 0.85),
        fill,
    )
}
fn aurora(u: &Uniforms, px: f32, py: f32) -> [f32; 3] {
    let mut color = u.dark;
    for i in 0..2 {
        let f = i as f32;
        let edge = -0.18
            + f * 0.32
            + 0.13 * (px * 3.8 + f * 1.8 + 0.6 * u.orbit_sin).sin()
            + 0.045 * (px * 9. - f + u.orbit_cos).cos();
        let d = py - edge;
        let fibers = 0.7 + 0.3 * (px * 160. + (px * 11. + u.orbit_sin).sin()).sin().powi(2);
        let curtain = (-d.max(0.) * 8.).exp() * smoothstep(-0.025, 0.03, d) * fibers;
        let glow = 0.4 * (-d * d * 65.).exp() + 0.65 * (-d * d * 1250.).exp() + curtain * 0.4;
        let t = 0.5 + 0.5 * (px * 2. + f + u.orbit_sin).sin();
        let light = mix3(mix3(u.blue, u.cyan, t), u.mint, 0.20);
        for i in 0..3 {
            color[i] += light[i] * glow;
        }
    }
    color
}
fn contours(u: &Uniforms, px: f32, py: f32) -> [f32; 3] {
    let qx = px * 4.;
    let qy = py * 4.;
    let t = u.orbit_sin.atan2(u.orbit_cos);
    // Independent traveling waves deform the terrain locally instead of
    // translating a fixed image. Integer temporal frequencies close the loop.
    let warp = qx * 0.7 - qy * 0.9 - 2. * t;
    let bend = qx * 0.65 + qy * 0.45 - t;
    let a = qx + 0.85 * (qy * 0.8 + t).sin() + 0.30 * warp.cos();
    let b = qy * 0.85 - qx * 0.25 + 0.65 * bend.cos();
    let c = qx * 1.6 + qy * 1.2 + t;
    let weight = 0.30 + 0.06 * u.orbit_sin;
    let field = 0.52 * a.sin() + weight * b.cos() + 0.12 * c.sin();
    // Advect three major levels per loop. atan2's branch jump is an integer
    // number of levels, so both major and minor isolines remain continuous.
    let level = field * 9. + 3. * t / TAU;
    let d = (level - level.round()).abs();
    let minor = (level * 3. - (level * 3.).round()).abs();
    let dx = 0.52 * a.cos() * (1. - 0.21 * warp.sin())
        - weight * b.sin() * (-0.25 - 0.4225 * bend.sin())
        + 0.192 * c.cos();
    let dy = 0.52 * a.cos() * (0.68 * (qy * 0.8 + t).cos() + 0.27 * warp.sin())
        - weight * b.sin() * (0.85 - 0.2925 * bend.sin())
        + 0.144 * c.cos();
    let width = (36. * (dx.abs() + dy.abs()) / u.width.min(u.height) as f32).clamp(0.004, 0.14);
    let line = 1. - smoothstep(width * 0.45, width * 1.35, d);
    let fine = 1. - smoothstep(width * 0.60, width * 2.40, minor);
    let base = mix3(
        mix3(u.dark, u.blue, 0.15 + 0.15 * field),
        u.cyan,
        fine * 0.16,
    );
    mix3(base, mix3(u.cyan, u.mint, 0.35), line * 0.85)
}
fn prism_vertex(x: f32, y: f32, t: f32, seed: u32) -> [f32; 3] {
    let jitter = random_value(x as i32 as u32 ^ seed, y as i32 as u32) - 0.5;
    [
        x + 0.28 * jitter + 0.14 * (t + x * 1.7 + y * 0.9).sin(),
        y + 0.24 * jitter + 0.14 * (2. * t + x * 0.8 - y * 1.4).cos(),
        0.32 * (x * 0.9 + y * 1.1 - t).sin()
            + 0.18 * (x * 1.6 - y * 0.7 + 2. * t).cos()
            + 0.24 * jitter,
    ]
}
fn prism(u: &Uniforms, px: f32, py: f32) -> [f32; 3] {
    let t = u.orbit_sin.atan2(u.orbit_cos);
    let p = [(px * 0.8 + py * 0.6) * 3., (-px * 0.6 + py * 0.8) * 3.];
    let ix = p[0].floor() as i32;
    let iy = p[1].floor() as i32;
    // Shared moving vertices preserve straight edges and a watertight mesh.
    for y in iy - 1..=iy + 1 {
        for x in ix - 1..=ix + 1 {
            let a = prism_vertex(x as f32, y as f32, t, u.seed);
            let b = prism_vertex(x as f32 + 1., y as f32, t, u.seed);
            let c = prism_vertex(x as f32, y as f32 + 1., t, u.seed);
            let d = prism_vertex(x as f32 + 1., y as f32 + 1., t, u.seed);
            let hash = grain_hash(x as u32 ^ u.seed, y as u32);
            // Alternate the diagonal to break up the regular tiled pattern.
            let triangles = if hash & 1 == 0 {
                [[a, b, c], [d, c, b]]
            } else {
                [[a, b, d], [a, d, c]]
            };
            for (side, vertices) in triangles.into_iter().enumerate() {
                let [a, b, c] = vertices;
                let e = [b[0] - a[0], b[1] - a[1]];
                let f = [c[0] - a[0], c[1] - a[1]];
                let v = [p[0] - a[0], p[1] - a[1]];
                let det = e[0] * f[1] - e[1] * f[0];
                let s = (v[0] * f[1] - v[1] * f[0]) / det;
                let r = (e[0] * v[1] - e[1] * v[0]) / det;
                let weights = [1. - s - r, s, r];
                if weights.iter().any(|w| *w < 0.) {
                    continue;
                }
                let hash = hash ^ if side == 0 { 0x5bd1e995 } else { 0 };
                let phase = (hash & 65535) as f32 / 65535. * TAU;
                // Shared heights form a crystalline surface. Each plane bends
                // the same traveling light band by its own surface normal.
                let ez = b[2] - a[2];
                let fz = c[2] - a[2];
                let nx = (e[1] * fz - ez * f[1]) / det;
                let ny = (ez * f[0] - e[0] * fz) / det;
                let length = (nx * nx + ny * ny + 1.).sqrt();
                let (nx, ny, nz) = (nx / length, ny / length, 1. / length);
                let light = (nx * -0.35 + ny * -0.45 + nz * 0.82).max(0.);
                let band = p[0] * 1.65 + p[1] * 0.85 - t;
                let refraction = band + nx * 2.8 + ny * 1.8;
                let spectrum = [
                    0.5 + 0.5 * refraction.cos(),
                    0.5 + 0.5 * (refraction - TAU / 3.).cos(),
                    0.5 + 0.5 * (refraction + TAU / 3.).cos(),
                ];
                // Neutral palettes stay neutral; colored palettes split light.
                let saturation = u.cyan.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                    - u.cyan.iter().copied().fold(f32::INFINITY, f32::min);
                let face = mix3(u.blue, u.cyan, 0.5 + 0.5 * (phase + nx - ny).sin());
                let face = mix3(face, spectrum, (saturation * 1.4).min(1.) * 0.38);
                let glint = (0.5 + 0.5 * refraction.cos()).powi(10);
                let color = mix3(
                    mix3(u.dark, face, 0.24 + 0.65 * light),
                    u.mint,
                    glint * 0.72,
                );
                // Barycentric distance divided by gradient gives pixel coverage.
                let distances = [
                    weights[0] * det.abs() / ((e[0] - f[0]).powi(2) + (e[1] - f[1]).powi(2)).sqrt(),
                    s * det.abs() / (f[0] * f[0] + f[1] * f[1]).sqrt(),
                    r * det.abs() / (e[0] * e[0] + e[1] * e[1]).sqrt(),
                ];
                let edge = distances.into_iter().fold(f32::INFINITY, f32::min);
                let seam =
                    1. - smoothstep(0.0015, 0.0015 + 2. / u.width.min(u.height) as f32, edge);
                // Both adjacent faces meet at the same edge color, avoiding
                // a flickering pixel when a moving edge crosses its sample.
                let edge_light = (0.5 + 0.5 * band.cos()).powi(8);
                let edge_color = mix3(mix3(u.dark, u.blue, 0.48), u.mint, 0.18 + 0.72 * edge_light);
                return mix3(color, edge_color, seam);
            }
        }
    }
    u.dark
}
fn painterly(u: &Uniforms, px: f32, py: f32) -> [f32; 3] {
    let random_value = |i, salt| random_value(i ^ u.seed, salt);
    let mut color = mix3(u.mint, [1., 0.97, 0.90], 0.65);
    for i in 0..14 {
        let phase = random_value(i, 29) * TAU;
        let s = u.orbit_sin * phase.cos() + u.orbit_cos * phase.sin();
        let c = u.orbit_cos * phase.cos() - u.orbit_sin * phase.sin();
        let progress = 0.5 + 0.5 * s;
        let a = random_value(i, 17) * std::f32::consts::PI + 0.22 * c;
        let cx = (random_value(i, 21) - 0.5) * 1.7 + c * 0.06;
        let cy = (random_value(i, 22) - 0.5) * 1.3 + s * 0.06;
        let dx = px - cx;
        let dy = py - cy;
        let along = dx * a.cos() + dy * a.sin();
        let y = -dx * a.sin() + dy * a.cos() - 0.022 * (along * 8. + s * 1.8).sin();
        let full_len = 0.18 + random_value(i, 23) * 0.25;
        // Grow each brush mark from its anchored tail, on a staggered cycle.
        let len = full_len * (0.18 + 0.82 * progress);
        let x = along + full_len - len;
        let width = (0.045 + random_value(i, 24) * 0.08) * (0.85 + 0.15 * c);
        let rough = paper_noise(along * 60. + i as f32, y * 60. + i as f32) * 0.18;
        let coverage = (1. - smoothstep(0.7, 1., x.abs() / len + rough))
            * (1. - smoothstep(0.7, 1., y.abs() / width + rough));
        let fibers = 0.85 + 0.15 * (y * 260. + i as f32).sin();
        color = mix3(
            color,
            [u.blue, u.cyan, u.mint][i as usize % 3],
            coverage * fibers * (0.15 + 0.70 * progress),
        );
    }
    color
}
fn sample(u: &Uniforms, x: u32, y: u32) -> image::Rgba<u8> {
    let unit = u.width.min(u.height) as f32;
    let px = (x as f32 + 0.5 - u.width as f32 * 0.5) / unit + u.offset[0];
    let py = (y as f32 + 0.5 - u.height as f32 * 0.5) / unit + u.offset[1];
    let qx = px * 3.4 + 0.34 * u.orbit_cos;
    let qy = py * 3.4 + 0.28 * u.orbit_sin;
    let wx = (qy * 1.65 + 0.65 * u.orbit_sin).sin() + 0.45 * (qx * 1.3 - qy).cos();
    let wy = (qx * 1.45 - 0.55 * u.orbit_cos).sin() + 0.40 * (qy * 1.4 + qx).cos();
    let qx = qx + wx * 0.85;
    let qy = qy + wy * 0.85;
    let field = (qx * 1.75 + (qy * 1.35).sin()).sin()
        + 0.60 * (qy * 1.90 - qx * 0.65).cos()
        + 0.25 * (qx * 0.80 + qy * 1.20).sin();
    let band = 0.5 + 0.5 * (field * 3.8 + qy * 0.55).sin();
    let grain = (grain_hash(x, y) & 65535) as f32 / 65535. - 0.5;
    let color = match u.effect {
        0 => mix3(
            mix3(
                mix3(u.dark, u.blue, smoothstep(0.08, 0.56, band)),
                u.cyan,
                smoothstep(0.48, 0.82, band),
            ),
            u.mint,
            smoothstep(0.78, 0.98, band),
        ),
        1 => lava(u, px, py),
        2 => aurora(u, px, py),
        3 => contours(u, px, py),
        4 => prism(u, px, py),
        5 => painterly(u, px, py),
        6 => nebula(u, px, py),
        _ => unreachable!(),
    };
    let amount = if u.effect == 0 {
        0.105 * (0.25 + 0.75 * band)
    } else if u.effect == 5 {
        0.075
    } else {
        0.015
    };
    let mut out = [255; 4];
    for i in 0..3 {
        out[i] = ((color[i] + grain * amount).clamp(0., 1.) * 255.).round() as u8;
    }
    image::Rgba(out)
}
fn cpu_frame(u: &Uniforms) -> RgbaImage {
    RgbaImage::from_fn(u.width, u.height, |x, y| sample(u, x, y))
}

#[cfg(target_os = "macos")]
mod gpu {
    use super::*;
    use metal::*;
    pub(super) struct Shader {
        device: Device,
        queue: CommandQueue,
        pipeline: ComputePipelineState,
        output: Option<Buffer>,
    }
    impl Shader {
        pub(super) fn new() -> Result<Self, String> {
            objc::rc::autoreleasepool(|| {
                let device = Device::system_default().ok_or("Metal device unavailable")?;
                let options = CompileOptions::new();
                options.set_fast_math_enabled(false);
                let library = device
                    .new_library_with_source(include_str!("shaders/motion.metal"), &options)?;
                let function = library.get_function("motion_backdrop", None)?;
                let pipeline = device.new_compute_pipeline_state_with_function(&function)?;
                let queue = device.new_command_queue();
                Ok(Self {
                    device,
                    queue,
                    pipeline,
                    output: None,
                })
            })
        }
        pub(super) fn frame(&mut self, u: &Uniforms) -> Result<RgbaImage, String> {
            objc::rc::autoreleasepool(|| {
                let length = u.width as u64 * u.height as u64 * 4;
                if self.output.as_ref().is_none_or(|b| b.length() != length) {
                    self.output = Some(
                        self.device
                            .new_buffer(length, MTLResourceOptions::StorageModeShared),
                    );
                }
                let output = self.output.as_ref().unwrap();
                let command = self.queue.new_command_buffer();
                let encoder = command.new_compute_command_encoder();
                encoder.set_compute_pipeline_state(&self.pipeline);
                encoder.set_buffer(0, Some(output), 0);
                encoder.set_bytes(
                    1,
                    std::mem::size_of::<Uniforms>() as u64,
                    u as *const Uniforms as *const _,
                );
                encoder.dispatch_threads(
                    MTLSize::new(u.width as u64, u.height as u64, 1),
                    MTLSize::new(16, 16, 1),
                );
                encoder.end_encoding();
                command.commit();
                command.wait_until_completed();
                if command.status() != MTLCommandBufferStatus::Completed {
                    return Err("Motion Metal command failed".into());
                }
                // GPUI 0.2.2 has no public custom RGBA shader hook. This prototype
                // reads shared output, then GPUI uploads it to its image atlas.
                // The buffer/pipeline are reused; preview resolution is bounded.
                let pixels = unsafe {
                    std::slice::from_raw_parts(output.contents() as *const u8, length as usize)
                };
                Ok(RgbaImage::from_raw(u.width, u.height, pixels.to_vec()).unwrap())
            })
        }
    }
    thread_local! {
        static SHADER: std::cell::RefCell<Result<Shader, String>> = std::cell::RefCell::new(Shader::new());
    }
    pub(super) fn frame(u: &Uniforms) -> Result<RgbaImage, String> {
        SHADER.with(|shader| match &mut *shader.borrow_mut() {
            Ok(shader) => shader.frame(u),
            Err(error) => Err(error.clone()),
        })
    }
}

#[cfg(target_os = "linux")]
mod gpu {
    use super::*;
    static SHADER: std::sync::OnceLock<
        std::sync::Mutex<Result<crate::linux_compute::Compute, String>>,
    > = std::sync::OnceLock::new();
    pub(super) fn frame(u: &Uniforms) -> Result<RgbaImage, String> {
        // Explicit scalar packing matches GLSL std430; no struct padding/unsafe casts.
        let mut words = vec![
            u.width,
            u.height,
            u.orbit_sin.to_bits(),
            u.orbit_cos.to_bits(),
        ];
        for color in [u.dark, u.blue, u.cyan, u.mint] {
            words.extend(color.map(f32::to_bits));
        }
        words.extend([u.effect, u.seed]);
        words.extend(u.offset.map(f32::to_bits));
        let shader = SHADER.get_or_init(|| {
            std::sync::Mutex::new(crate::linux_compute::Compute::new(
                include_str!("shaders/motion.comp"),
                20,
            ))
        });
        match &mut *shader.lock().map_err(|e| e.to_string())? {
            Ok(shader) => shader.frame(u.width, u.height, &words, None),
            Err(error) => Err(error.clone()),
        }
    }
}

#[cfg(test)]
pub fn frame(width: u32, height: u32, preset: usize, motion: Motion, phase: f32) -> RgbaImage {
    frame_with_colors(width, height, preset, motion, phase, None, 0)
}

pub fn frame_with_colors(
    width: u32,
    height: u32,
    preset: usize,
    motion: Motion,
    phase: f32,
    colors: Option<[[u8; 3]; 2]>,
    seed: u32,
) -> RgbaImage {
    let mut uniforms = Uniforms::new(width, height, preset, motion, phase).seeded(seed);
    if let Some([from, to]) = colors {
        let a = from.map(|v| v as f32 / 255.);
        let z = to.map(|v| v as f32 / 255.);
        uniforms.dark = z.map(|v| v * 0.10);
        uniforms.blue = z;
        uniforms.cyan = a;
        uniforms.mint = a.map(|v| v * 0.45 + 0.55);
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    match gpu::frame(&uniforms) {
        Ok(image) => return image,
        Err(error) => {
            static WARNING: std::sync::Once = std::sync::Once::new();
            WARNING.call_once(|| eprintln!("Motion shader: {error}; using CPU fallback"));
        }
    }
    cpu_frame(&uniforms)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires hardware Vulkan; synthetic frames only, no files/network"]
    fn vulkan_matches_cpu_and_reports_throughput() {
        for motion in Motion::EFFECTS.into_iter().filter(|m| m.uses_shader()) {
            for (w, h) in [(97, 63), (63, 97)] {
                for phase in [0., 0.37, 0.99] {
                    for seed in [0, 12345] {
                        let mut u = Uniforms::new(w, h, 1, motion, phase).seeded(seed);
                        u.cyan = [0.2, 0.4, 0.7];
                        let actual = gpu::frame(&u).expect("hardware Vulkan required");
                        let expected = cpu_frame(&u);
                        let error = actual
                            .as_raw()
                            .iter()
                            .zip(expected.as_raw())
                            .map(|(a, b)| a.abs_diff(*b))
                            .max()
                            .unwrap();
                        assert!(
                            error <= 2,
                            "{motion:?} {w}x{h} phase={phase} seed={seed}: error={error}"
                        );
                    }
                }
            }
            let u = Uniforms::new(960, 540, 1, motion, 0.37);
            gpu::frame(&u).unwrap();
            let start = std::time::Instant::now();
            for _ in 0..12 {
                std::hint::black_box(gpu::frame(&u).unwrap());
            }
            let gpu_ms = start.elapsed().as_secs_f64() * 1000. / 12.;
            let start = std::time::Instant::now();
            std::hint::black_box(cpu_frame(&u));
            let cpu_ms = start.elapsed().as_secs_f64() * 1000.;
            println!(
                "{motion:?} 960x540: CPU {cpu_ms:.2} ms, Vulkan compute+readback {gpu_ms:.2} ms ({:.1}x)",
                cpu_ms / gpu_ms
            );
        }
    }
    #[test]
    fn prism_cells_remain_convex_throughout_motion() {
        // Either diagonal must remain valid: an inverted cell would overlap
        // its neighbor and leave holes as the vertices flex.
        for step in 0..60 {
            let t = step as f32 / 60. * TAU;
            for y in -8..=8 {
                for x in -8..=8 {
                    let x = x as f32;
                    let y = y as f32;
                    let corners = [
                        prism_vertex(x, y, t, step),
                        prism_vertex(x + 1., y, t, step),
                        prism_vertex(x + 1., y + 1., t, step),
                        prism_vertex(x, y + 1., t, step),
                    ];
                    for i in 0..4 {
                        let a = corners[i];
                        let b = corners[(i + 1) % 4];
                        let c = corners[(i + 2) % 4];
                        let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
                        assert!(cross > 0.1, "folded cell ({x}, {y}), step {step}");
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "renders local Prism palette samples and a looping GIF for visual review"]
    fn prism_crystal_visual_qa() {
        motion_visual_qa(Motion::Prism, "prism");
    }

    #[test]
    #[ignore = "renders synthetic seeded Lava/Prism variations for local visual review"]
    fn seeded_motion_visual_qa() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/seeded-motion-qa");
        std::fs::create_dir_all(&dir).unwrap();
        let mut variations = RgbaImage::new(960, 360);
        for (row, motion) in [Motion::Lava, Motion::Prism].into_iter().enumerate() {
            for (column, seed) in [0, 42, 314159].into_iter().enumerate() {
                let image = frame_with_colors(
                    320,
                    180,
                    motion.suggested_preset().unwrap_or(0),
                    motion,
                    0.22,
                    None,
                    seed,
                );
                image::imageops::replace(
                    &mut variations,
                    &image,
                    column as i64 * 320,
                    row as i64 * 180,
                );
            }
        }
        variations.save(dir.join("variations.png")).unwrap();
    }

    #[test]
    #[ignore = "renders local Lava palette samples and a looping GIF for visual review"]
    fn lava_fluid_visual_qa() {
        motion_visual_qa(Motion::Lava, "lava");
    }

    fn motion_visual_qa(motion: Motion, name: &str) {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("target/{name}-qa"));
        std::fs::create_dir_all(&dir).unwrap();
        let mut palettes = RgbaImage::new(960, 540);
        for preset in 0..PRESETS.len() {
            let image = frame(240, 270, preset, motion, 0.22);
            image::imageops::replace(
                &mut palettes,
                &image,
                (preset % 4 * 240) as i64,
                (preset / 4 * 270) as i64,
            );
        }
        palettes.save(dir.join("palettes.png")).unwrap();
        let preset = motion.suggested_preset().unwrap_or(0);
        frame(960, 540, preset, motion, 0.22)
            .save(dir.join("landscape.png"))
            .unwrap();
        frame(405, 720, preset, motion, 0.22)
            .save(dir.join("portrait.png"))
            .unwrap();
        let file = std::fs::File::create(dir.join(format!("{name}.gif"))).unwrap();
        let mut encoder = gif::Encoder::new(file, 480, 270, &[]).unwrap();
        encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        for i in 0..100 {
            let mut pixels = frame(480, 270, preset, motion, i as f32 / 100.).into_raw();
            let mut frame = gif::Frame::from_rgba_speed(480, 270, &mut pixels, 10);
            frame.delay = 5;
            encoder.write_frame(&frame).unwrap();
        }
        #[cfg(target_os = "macos")]
        {
            let start = std::time::Instant::now();
            for i in 0..60 {
                gpu::frame(&Uniforms::new(960, 540, preset, motion, i as f32 / 60.)).unwrap();
            }
            println!(
                "{}: {:.2} ms/frame (compute + readback)",
                motion.label(),
                start.elapsed().as_secs_f64() * 1000. / 60.
            );
        }
    }

    #[test]
    fn evolving_effects_are_continuous_across_phase_wraps() {
        for motion in [Motion::Contours, Motion::Prism, Motion::Paint, Motion::Lava] {
            for (before, after) in [(0.49999, 0.50001), (0.99999, 0.00001)] {
                let preset = motion.suggested_preset().unwrap_or(0);
                let a = frame(256, 144, preset, motion, before);
                let b = frame(256, 144, preset, motion, after);
                let differences: Vec<_> = a
                    .as_raw()
                    .iter()
                    .zip(b.as_raw())
                    .map(|(a, b)| a.abs_diff(*b))
                    .collect();
                let peak = *differences.iter().max().unwrap();
                let mean =
                    differences.iter().map(|v| *v as f32).sum::<f32>() / differences.len() as f32;
                assert!(
                    peak <= 12 && mean < 0.25,
                    "{motion:?}, phase {before}: peak {peak}, mean {mean}"
                );
            }
        }
    }
    #[test]
    #[ignore = "renders evolving brushwork and facet motion for visual review"]
    fn painterly_prism_motion_qa() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/dynamic-motion-qa");
        for motion in [Motion::Paint, Motion::Prism] {
            let dir = root.join(motion.label());
            std::fs::create_dir_all(&dir).unwrap();
            let preset = motion.suggested_preset().unwrap_or(0);
            let start = std::time::Instant::now();
            for i in 0..180 {
                let image = frame(960, 540, preset, motion, i as f32 / 180.);
                image.save(dir.join(format!("frame-{i:03}.png"))).unwrap();
            }
            frame(405, 720, preset, motion, 0.25)
                .save(dir.join("portrait.png"))
                .unwrap();
            println!(
                "{} review frames rendered in {:.2}s",
                motion.label(),
                start.elapsed().as_secs_f32()
            );
            #[cfg(target_os = "macos")]
            {
                let start = std::time::Instant::now();
                for i in 0..60 {
                    gpu::frame(&Uniforms::new(960, 540, preset, motion, i as f32 / 60.)).unwrap();
                }
                println!(
                    "{}: {:.2} ms/frame (compute + readback)",
                    motion.label(),
                    start.elapsed().as_secs_f64() * 1000. / 60.
                );
            }
        }
    }
    #[test]
    #[ignore = "renders a contour animation for visual motion review"]
    fn contours_motion_qa() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/contours-qa");
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..180 {
            frame(960, 540, 6, Motion::Contours, i as f32 / 180.)
                .save(dir.join(format!("frame-{i:03}.png")))
                .unwrap();
        }
        frame(405, 720, 6, Motion::Contours, 0.25)
            .save(dir.join("portrait.png"))
            .unwrap();
    }
    #[test]
    fn seeded_shader_cpu_and_metal_agree_and_phase_wraps_are_continuous() {
        for motion in Motion::EFFECTS.into_iter().filter(|m| m.uses_shader()) {
            let preset = motion.suggested_preset().unwrap_or(0);
            for seed in [17, u32::MAX] {
                for (w, h) in [(96, 54), (54, 96)] {
                    for phase in [0., 0.37, 0.99] {
                        let uniforms = Uniforms::new(w, h, preset, motion, phase).seeded(seed);
                        let cpu = cpu_frame(&uniforms);
                        assert_eq!(cpu, cpu_frame(&uniforms));
                        #[cfg(target_os = "macos")]
                        {
                            let gpu = gpu::frame(&uniforms).unwrap();
                            let peak = cpu
                                .as_raw()
                                .iter()
                                .zip(gpu.as_raw())
                                .map(|(a, b)| a.abs_diff(*b))
                                .max()
                                .unwrap();
                            assert!(
                                peak <= 2,
                                "{motion:?}, seed {seed}, phase {phase}: error {peak}"
                            );
                        }
                    }
                    let branch = (0.5 - random_value(seed, 41)).rem_euclid(1.);
                    for phase in [branch, 0.] {
                        let a = cpu_frame(
                            &Uniforms::new(w, h, preset, motion, phase - 0.00001).seeded(seed),
                        );
                        let b = cpu_frame(
                            &Uniforms::new(w, h, preset, motion, phase + 0.00001).seeded(seed),
                        );
                        let peak = a
                            .as_raw()
                            .iter()
                            .zip(b.as_raw())
                            .map(|(a, b)| a.abs_diff(*b))
                            .max()
                            .unwrap();
                        assert!(
                            peak <= 12,
                            "{motion:?}, seed {seed}, phase {phase}: discontinuity {peak}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn cpu_and_metal_agree_in_portrait_and_landscape() {
        #[cfg(target_os = "macos")]
        for motion in Motion::EFFECTS.into_iter().filter(|m| m.uses_shader()) {
            for (w, h) in [(128, 72), (72, 128)] {
                for preset in 0..PRESETS.len() {
                    for phase in [0., 0.37, 0.99] {
                        let uniforms = Uniforms::new(w, h, preset, motion, phase);
                        let gpu =
                            gpu::frame(&uniforms).expect("test requires working Metal shader");
                        let cpu = cpu_frame(&uniforms);
                        let max_error = gpu
                            .as_raw()
                            .iter()
                            .zip(cpu.as_raw())
                            .map(|(a, b)| a.abs_diff(*b))
                            .max()
                            .unwrap();
                        assert!(
                            max_error <= 2,
                            "{motion:?}, palette {preset}, phase {phase}: error {max_error}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    #[ignore = "renders visual samples and measures real Metal throughput"]
    fn liquid_visual_qa() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/liquid-qa");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, w, h) in [("landscape", 1280, 720), ("portrait", 405, 720)] {
            frame(w, h, 1, Motion::Liquid, 0.)
                .save(dir.join(format!("{name}.png")))
                .unwrap();
        }
        #[cfg(target_os = "macos")]
        for (w, h) in [(960, 540), (1920, 1080)] {
            let start = std::time::Instant::now();
            for i in 0..60 {
                gpu::frame(&Uniforms::new(w, h, 1, Motion::Liquid, i as f32 / 60.)).unwrap();
            }
            println!(
                "Liquid {w}×{h}: {:.2} ms/frame (compute + readback)",
                start.elapsed().as_secs_f64() * 1000. / 60.
            );
        }
    }
    #[test]
    #[ignore = "renders each shader style for visual review and measures GPU throughput"]
    fn motion_gallery_qa() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/motion-qa");
        std::fs::create_dir_all(&dir).unwrap();
        for motion in Motion::EFFECTS.into_iter().filter(|m| m.uses_shader()) {
            let preset = motion.suggested_preset().unwrap_or(0);
            for (name, w, h) in [("landscape", 960, 540), ("portrait", 405, 720)] {
                frame(w, h, preset, motion, 0.22)
                    .save(dir.join(format!("{}-{name}.png", motion.label())))
                    .unwrap();
            }
            #[cfg(target_os = "macos")]
            {
                let start = std::time::Instant::now();
                for i in 0..60 {
                    gpu::frame(&Uniforms::new(960, 540, preset, motion, i as f32 / 60.)).unwrap();
                }
                println!(
                    "{} 960×540: {:.2} ms/frame (compute + readback)",
                    motion.label(),
                    start.elapsed().as_secs_f64() * 1000. / 60.
                );
            }
        }
    }
}
