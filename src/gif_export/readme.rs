//! Opt-in README media generation using the app's renderer and GIF exporter.
use super::*;

#[test]
#[ignore = "renders user-supplied local album shots for the README; writes GIFs in a reserved output directory"]
fn render_readme_album_gif() {
    use crate::{
        animation::{Entrance, ImageAnimation, Motion},
        backdrop::Backdrop,
    };
    let output = std::path::PathBuf::from(
        std::env::var_os("GLANCE_README_OUTPUT")
            .expect("Set GLANCE_README_OUTPUT to a reserved directory"),
    );
    assert!(output.is_dir());
    let mut frames = Vec::new();
    let mut dimensions = None;
    for (name, variable) in [
        ("light", "GLANCE_ALBUM_LIGHT"),
        ("dark", "GLANCE_ALBUM_DARK"),
    ] {
        let input = std::env::var_os(variable)
            .unwrap_or_else(|| panic!("Set {variable} to the local album shot"));
        let source = image::open(input).unwrap().into_rgba8();
        // Keep the README small while retaining the full gallery and its labels.
        let source =
            image::imageops::resize(&source, 480, 560, image::imageops::FilterType::Lanczos3);
        let mut document = Document::new(source);
        document.backdrop = Some(Backdrop {
            motion: Motion::Lava,
            colors: Some([[86, 65, 145], [35, 23, 64]]),
            gradient: true,
            padding: 64,
            inside_padding: 18,
            inner_radius: 24,
            shadow: 28,
            seconds: 3,
            seed: 42,
            ..Default::default()
        });
        document.image_animation = ImageAnimation {
            effect: Entrance::None,
            duration_ms: 600,
            delay_ms: 0,
            seconds: 3,
            exit: false,
        };
        let path = output.join(format!("{name}.gif"));
        assert!(!path.exists());
        assert!(encode(&document, &path, 0., &AtomicBool::new(false), |_| {}).unwrap());
        let mut decoder = gif::DecodeOptions::new()
            .read_info(File::open(path).unwrap())
            .unwrap();
        let size = (decoder.width(), decoder.height());
        assert!(dimensions.is_none_or(|previous| previous == size));
        dimensions = Some(size);
        let palette = decoder.global_palette().unwrap().to_vec();
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            frames.push(gif::Frame {
                width: frame.width,
                height: frame.height,
                left: frame.left,
                top: frame.top,
                delay: frame.delay,
                dispose: frame.dispose,
                transparent: frame.transparent,
                palette: Some(frame.palette.clone().unwrap_or_else(|| palette.clone())),
                buffer: std::borrow::Cow::Owned(frame.buffer.to_vec()),
                ..Default::default()
            });
        }
    }
    assert_eq!(frames.len(), 120);
    // Begin with a developed backdrop at phase 0.3 for GitHub's initial still.
    frames.rotate_left(18);
    let (width, height) = dimensions.unwrap();
    let file = File::create_new(output.join("albums-native.gif")).unwrap();
    let mut encoder = gif::Encoder::new(file, width, height, &[]).unwrap();
    encoder.set_repeat(gif::Repeat::Infinite).unwrap();
    for frame in frames {
        encoder.write_frame(&frame).unwrap();
    }
    encoder.into_inner().unwrap().sync_all().unwrap();
    eprintln!(
        "README album GIF: {}",
        output.join("albums-native.gif").display()
    );
}
