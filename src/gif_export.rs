//! Streaming, infinite GIF loops with one deterministic palette for the entire clip.
use crate::{animation::Renderer, document::Document};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
fn matte(image: &mut image::RgbaImage) {
    for p in image.pixels_mut() {
        let a = p[3] as u16;
        for c in &mut p.0[..3] {
            *c = ((*c as u16 * a + 246 * (255 - a) + 127) / 255) as u8
        }
        p[3] = 255;
    }
}
pub fn encode(
    document: &Document,
    path: &Path,
    start_phase: f32,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u32),
) -> Result<bool, String> {
    if document.backdrop.is_none() && !document.image_animation.enabled() {
        return Err("Add a backdrop or image animation before exporting GIF".into());
    }
    if document.image_animation.enabled() {
        document.image_animation.validate()?;
    }
    let b = document.animation_backdrop();
    if !(2..=15).contains(&b.seconds) {
        return Err("GIF duration must be 2–15 seconds".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(false);
    }
    let renderer = Renderer::for_document(document, Some(960));
    let start_phase = if document.image_animation.enabled() {
        0.
    } else {
        start_phase
    };
    // Learn colors across the full cycle, then freeze the palette to avoid flickering foreground.
    let mut samples = Vec::new();
    for i in 0..8 {
        if cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let mut frame = renderer.frame(start_phase + i as f32 / 8.);
        matte(&mut frame);
        samples.extend(image::imageops::thumbnail(&frame, 128, 128).into_raw());
    }
    let quantizer = color_quant::NeuQuant::new(10, 256, &samples);
    let palette = quantizer.color_map_rgb();
    let export_dir = tempfile::Builder::new()
        .prefix(".glance-export-")
        .tempdir_in(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )
        .map_err(|e| e.to_string())?;
    let temporary_path = export_dir.path().join("animation.gif");
    let file = File::create_new(&temporary_path).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);
    {
        let mut encoder = gif::Encoder::new(
            &mut writer,
            renderer.width as u16,
            renderer.height as u16,
            &palette,
        )
        .map_err(|e| e.to_string())?;
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .map_err(|e| e.to_string())?;
        let count = b.seconds * 20;
        for i in 0..count {
            if cancel.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let mut image = renderer.frame(start_phase + i as f32 / count as f32);
            matte(&mut image);
            let indexed: Vec<u8> = image
                .pixels()
                .map(|p| quantizer.index_of(&p.0) as u8)
                .collect();
            let frame = gif::Frame {
                width: renderer.width as u16,
                height: renderer.height as u16,
                delay: 5,
                dispose: gif::DisposalMethod::Keep,
                buffer: std::borrow::Cow::Owned(indexed),
                ..Default::default()
            };
            encoder.write_frame(&frame).map_err(|e| e.to_string())?;
            progress(((i + 1) * 100 / count).min(99));
        }
        encoder.into_inner().map_err(|e| e.to_string())?;
    }
    writer.flush().map_err(|e| e.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Ok(false);
    }
    std::fs::rename(&temporary_path, path).map_err(|e| e.to_string())?;
    progress(100);
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entrance_gif_starts_at_zero_and_reveals_over_a_static_backdrop() {
        let mut d = Document::new(image::RgbaImage::from_pixel(
            32,
            24,
            image::Rgba([230, 15, 20, 255]),
        ));
        d.image_animation = crate::animation::ImageAnimation {
            effect: crate::animation::Entrance::Diagonal,
            duration_ms: 400,
            seconds: 2,
            exit: true,
            ..Default::default()
        };
        let path = std::env::temp_dir().join(format!("glance-entrance-{}.gif", std::process::id()));
        // No backdrop and a nonzero preview phase must still export from the start.
        assert!(encode(&d, &path, 0.75, &AtomicBool::new(false), |_| {}).unwrap());
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(File::open(&path).unwrap()).unwrap();
        assert_eq!(decoder.repeat(), gif::Repeat::Infinite);
        let mut count = 0;
        let mut duration = 0;
        let mut revealed = false;
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            let offset = (12 * frame.width as usize + 16) * 4;
            let p = &frame.buffer[offset..offset + 4];
            if count == 0 {
                assert!(
                    (p[0] as i32 - p[1] as i32).abs() < 5,
                    "first frame is neutral background"
                );
            }
            if p[0] > 180 && p[1] < 60 {
                revealed = true;
            }
            count += 1;
            duration += frame.delay as u32;
        }
        assert!(revealed);
        assert_eq!(count, 40);
        assert_eq!(duration, 200);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn gif_is_infinite_has_exact_duration_and_constant_foreground() {
        let mut d = Document::new(image::RgbaImage::from_pixel(
            40,
            30,
            image::Rgba([80, 170, 110, 255]),
        ));
        d.backdrop = Some(crate::backdrop::Backdrop {
            motion: crate::animation::Motion::Lava,
            seconds: 2,
            padding: 15,
            inside_padding: 9,
            inner_radius: 0,
            shadow: 0,
            ..Default::default()
        });
        let path = std::env::temp_dir().join(format!("glance-loop-{}.gif", std::process::id()));
        assert!(encode(&d, &path, 0.31, &AtomicBool::new(false), |_| {}).unwrap());
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(File::open(&path).unwrap()).unwrap();
        assert_eq!(decoder.repeat(), gif::Repeat::Infinite);
        assert_eq!((decoder.width(), decoder.height()), (88, 78));
        let mut count = 0;
        let mut duration = 0;
        let mut center = None;
        let mut first = None;
        let mut last = None;
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            count += 1;
            duration += frame.delay as u32;
            let offset = (30 * frame.width as usize + 35) * 4;
            let pixel = &frame.buffer[offset..offset + 4];
            let padding_offset = (30 * frame.width as usize + 15) * 4;
            assert_eq!(&frame.buffer[padding_offset..padding_offset + 4], pixel);
            if let Some(previous) = &center {
                assert_eq!(pixel, previous)
            } else {
                center = Some(pixel.to_vec())
            }
            if first.is_none() {
                first = Some(frame.buffer.to_vec())
            }
            last = Some(frame.buffer.to_vec());
        }
        assert_eq!(count, 40);
        assert_eq!(duration, 200);
        assert_ne!(first, last);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn cancellation_keeps_destination_and_cleans_temporary_files() {
        let mut d = Document::new(image::RgbaImage::new(20, 20));
        d.backdrop = Some(crate::backdrop::Backdrop {
            seconds: 2,
            motion: crate::animation::Motion::Flow,
            ..Default::default()
        });
        let dir = std::env::temp_dir().join(format!("glance-gif-cancel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("existing.gif");
        std::fs::write(&path, b"existing").unwrap();
        let cancel = AtomicBool::new(false);
        assert!(
            !encode(&d, &path, 0., &cancel, |p| {
                if p >= 5 {
                    cancel.store(true, Ordering::Relaxed)
                }
            })
            .unwrap()
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"existing");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod visual_qa {
    use super::*;
    #[test]
    #[ignore = "writes a real focus/magnifier GIF and MP4 demo; requires native encoder"]
    fn focus_and_loop_demo_qa() {
        let mut d = Document::new(crate::document::demo());
        d.commit(crate::document::Mark {
            style: Default::default(),
            tool: crate::document::Tool::Spotlight,
            points: vec![(62., 258.), (1137., 423.)],
            color: [255, 184, 46, 255],
            width: 2.,
            curve: None,
            text: String::new(),
        });
        d.commit(crate::document::Mark {
            style: Default::default(),
            tool: crate::document::Tool::Magnifier,
            points: vec![(240., 279.), (936., 545.)],
            color: [76, 141, 255, 255],
            width: 15.,
            curve: None,
            text: "2".into(),
        });
        d.backdrop = Some(crate::backdrop::Backdrop {
            motion: crate::animation::Motion::Lava,
            preset: 2,
            padding: 85,
            seconds: 5,
            ..Default::default()
        });
        let dir = std::path::PathBuf::from("target/focus-qa");
        std::fs::create_dir_all(&dir).unwrap();
        d.export_at(0.2).save(dir.join("focus-demo.png")).unwrap();
        let started = std::time::Instant::now();
        assert!(
            encode(
                &d,
                &dir.join("focus-demo.gif"),
                0.,
                &AtomicBool::new(false),
                |_| {}
            )
            .unwrap()
        );
        eprintln!("5s GIF export: {:.2}s", started.elapsed().as_secs_f32());
        assert!(
            crate::video::encode(
                &d,
                &dir.join("focus-demo.mp4"),
                0.,
                &AtomicBool::new(false),
                |_| {}
            )
            .unwrap()
        );
    }
}

#[cfg(test)]
mod readme;
