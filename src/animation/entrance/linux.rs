//! Vulkan entrance sampling on the preview/export worker.
use super::{Entrance, Projection};
use image::RgbaImage;
use std::sync::{Arc, Mutex, OnceLock};

static SHADER: OnceLock<Mutex<Result<crate::linux_compute::Compute, String>>> = OnceLock::new();
pub(super) fn frame(
    source: &Arc<RgbaImage>,
    effect: Entrance,
    bounds: (f32, f32, f32, f32),
    p: f32,
) -> Result<RgbaImage, String> {
    let projection = Projection::new(effect, p, bounds);
    let mut words = vec![
        source.width(),
        source.height(),
        u32::from(effect == Entrance::Diagonal),
    ];
    words.extend(
        [
            2. * p * p * (3. - 2. * p),
            (bounds.2.recip().powi(2) + bounds.3.recip().powi(2)).sqrt(),
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
        ]
        .map(f32::to_bits),
    );
    for row in projection.inverse {
        words.extend(row.map(f32::to_bits));
    }
    words.extend(
        [
            projection.center.0,
            projection.center.1,
            (p * if effect == Entrance::Pop { 5. } else { 4. }).min(1.),
        ]
        .map(f32::to_bits),
    );
    let shader = SHADER.get_or_init(|| {
        Mutex::new(crate::linux_compute::Compute::new(
            include_str!("../../shaders/entrance.comp"),
            21,
        ))
    });
    match &mut *shader.lock().map_err(|e| e.to_string())? {
        Ok(shader) => shader.frame(source.width(), source.height(), &words, Some(source)),
        Err(error) => Err(error.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "measures hardware Vulkan versus CPU sampling on synthetic cards; no files or network"]
    fn vulkan_foreground_sampling_benchmark() {
        use std::time::Instant;
        let source = Arc::new(RgbaImage::from_pixel(
            960,
            640,
            image::Rgba([40, 140, 230, 240]),
        ));
        let bounds = (80., 60., 800., 520.);
        for effect in [Entrance::Diagonal, Entrance::Pop, Entrance::Tilt] {
            let animation = super::super::ImageAnimation {
                effect,
                delay_ms: 0,
                ..Default::default()
            };
            // Compile/upload before measuring steady-state sampling.
            frame(&source, effect, bounds, 0.3).unwrap();
            let started = Instant::now();
            for i in 0..8 {
                std::hint::black_box(
                    frame(&source, effect, bounds, 0.25 + i as f32 * 0.08).unwrap(),
                );
            }
            let gpu = started.elapsed().as_secs_f64() * 1000. / 8.;
            let started = Instant::now();
            for i in 0..8 {
                std::hint::black_box(super::super::foreground(
                    &source,
                    animation,
                    bounds,
                    0.25 + i as f32 * 0.08,
                ));
            }
            let cpu = started.elapsed().as_secs_f64() * 1000. / 8.;
            println!("{effect:?} 960×640 foreground: CPU {cpu:.2} ms, Vulkan {gpu:.2} ms");
        }
        let source = image::RgbaImage::from_pixel(640, 420, image::Rgba([40, 140, 230, 255]));
        for effect in [Entrance::Diagonal, Entrance::Pop, Entrance::Tilt] {
            let renderer = crate::animation::Renderer::with_animation(
                &source,
                crate::backdrop::Backdrop {
                    motion: crate::animation::Motion::Liquid,
                    padding: 160,
                    ..Default::default()
                },
                Some(960),
                super::super::ImageAnimation {
                    effect,
                    delay_ms: 0,
                    ..Default::default()
                },
            );
            renderer.frame(0.05);
            let started = Instant::now();
            for i in 0..8 {
                std::hint::black_box(renderer.frame(0.05 + i as f32 * 0.016));
            }
            println!(
                "{effect:?} composed {}×{}: {:.2} ms/frame",
                renderer.width,
                renderer.height,
                started.elapsed().as_secs_f64() * 1000. / 8.
            );
        }
    }
    #[test]
    #[ignore = "requires hardware Vulkan; synthetic frames only, no files/network"]
    fn vulkan_matches_cpu_sampling_and_refreshes_cached_pixels() {
        let sources = [
            Arc::new(RgbaImage::from_fn(96, 64, |x, y| {
                image::Rgba([x as u8 * 2, y as u8 * 3, 70, if x < 8 { 0 } else { 210 }])
            })),
            Arc::new(RgbaImage::from_pixel(
                96,
                64,
                image::Rgba([30, 170, 200, 255]),
            )),
            Arc::new(RgbaImage::from_fn(63, 97, |x, y| {
                image::Rgba([x as u8 * 3, y as u8 * 2, 120, if y < 8 { 0 } else { 150 }])
            })),
            Arc::new(RgbaImage::from_pixel(96, 64, image::Rgba([0, 0, 0, 0]))),
        ];
        for source in sources {
            for effect in [Entrance::Diagonal, Entrance::Pop, Entrance::Tilt] {
                for p in [0.01, 0.2, 0.5, 0.85, 0.999] {
                    let bounds = (8., 6., 80., 50.);
                    let actual = frame(&source, effect, bounds, p).unwrap();
                    let animation = super::super::ImageAnimation {
                        effect,
                        delay_ms: 0,
                        ..Default::default()
                    };
                    let expected = super::super::foreground(&source, animation, bounds, p);
                    assert!(
                        actual
                            .as_raw()
                            .iter()
                            .zip(expected.as_raw())
                            .all(|(a, b)| a.abs_diff(*b) <= 1),
                        "{effect:?}, p={p}"
                    );
                }
            }
        }
    }
}
