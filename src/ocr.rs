//! Offline source-image OCR. Called only by editor workers.
use image::RgbaImage;
use std::{
    io::{Read, Seek},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(crate) const MAX_TEXT_BYTES: usize = 65_536;
#[cfg(test)]
pub(crate) type Recognizer = fn(&RgbaImage, Option<[u32; 4]>) -> Result<String, String>;

pub(crate) fn validate_rectangle(
    (width, height): (u32, u32),
    rectangle: Option<[u32; 4]>,
) -> Result<[u32; 4], String> {
    let [x, y, w, h] = rectangle.unwrap_or([0, 0, width, height]);
    if w == 0
        || h == 0
        || x.checked_add(w).is_none_or(|right| right > width)
        || y.checked_add(h).is_none_or(|bottom| bottom > height)
    {
        return Err("Text extraction rectangle must lie inside the source image and have positive dimensions".into());
    }
    Ok([x, y, w, h])
}

pub(crate) fn extract(image: &RgbaImage, rectangle: Option<[u32; 4]>) -> Result<String, String> {
    let [x, y, w, h] = validate_rectangle(image.dimensions(), rectangle)?;
    let scratch = tempfile::Builder::new()
        .prefix("glance-ocr-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let input = scratch.path().join("source.png");
    // Composite transparent pixels onto white for recognition, without mutating the document.
    let mut sample = image::imageops::crop_imm(image, x, y, w, h).to_image();
    for pixel in sample.pixels_mut() {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u32::from(*channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        }
        pixel[3] = 255;
    }
    sample.save(&input).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let mut command = {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let dir = exe.parent().ok_or("App directory unavailable")?;
        let helper = [
            dir.join("glance-ocr"),
            dir.join("../glance-ocr"),
            dir.join("../../glance-ocr"),
        ]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or("Text recognition helper unavailable. Build with scripts/bundle.sh.")?;
        let mut command = Command::new(helper);
        command.arg(&input);
        command
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("tesseract");
        command.arg(&input).arg("stdout");
        command
    };
    run(&mut command, Duration::from_secs(45))
}

fn read_bounded(file: &mut std::fs::File, limit: usize) -> Result<Vec<u8>, String> {
    file.rewind().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err(
            "Recognized text is too large (maximum 64 KiB). Extract a smaller area.".into(),
        );
    }
    Ok(bytes)
}

fn run(command: &mut Command, timeout: Duration) -> Result<String, String> {
    // Redirect to reserved files rather than allowing pipes to fill or reading unlimited output.
    let mut output = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut error = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(|e| e.to_string())?)
        .stderr(error.try_clone().map_err(|e| e.to_string())?)
        .spawn()
        .map_err(|e| {
            #[cfg(target_os = "linux")]
            if e.kind() == std::io::ErrorKind::NotFound {
                return "Local OCR requires Tesseract. Install tesseract and tesseract-data-eng."
                    .into();
            }
            format!("Could not start text recognition: {e}")
        })?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match result {
                    Err(e) => e.to_string(),
                    _ => "Text recognition timed out. Extract a smaller area and retry.".into(),
                });
            }
        }
    };
    if !status.success() {
        error.rewind().map_err(|e| e.to_string())?;
        let mut detail = String::new();
        error
            .take(4096)
            .read_to_string(&mut detail)
            .map_err(|e| e.to_string())?;
        return Err(format!("Text recognition failed: {}", detail.trim()));
    }
    let bytes = read_bounded(&mut output, MAX_TEXT_BYTES)?;
    let text = String::from_utf8(bytes).map_err(|_| "Text recognition returned invalid UTF-8")?;
    Ok(text.trim_end_matches(['\r', '\n', '\u{c}']).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rectangles_reject_empty_outside_and_overflowing_regions() {
        assert_eq!(
            validate_rectangle((100, 80), None).unwrap(),
            [0, 0, 100, 80]
        );
        assert_eq!(
            validate_rectangle((100, 80), Some([20, 10, 80, 70])).unwrap(),
            [20, 10, 80, 70]
        );
        for rect in [
            [0, 0, 0, 10],
            [0, 0, 10, 0],
            [90, 0, 20, 10],
            [0, 70, 10, 20],
            [u32::MAX, 0, 2, 10],
        ] {
            assert!(validate_rectangle((100, 80), Some(rect)).is_err());
        }
    }
    #[test]
    fn helper_output_is_bounded_and_failures_are_reported() {
        let mut success = Command::new("/usr/bin/printf");
        success.arg("Hello\nWorld\n");
        assert_eq!(
            run(&mut success, Duration::from_secs(1)).unwrap(),
            "Hello\nWorld"
        );
        let mut failure = Command::new("/usr/bin/false");
        assert!(
            run(&mut failure, Duration::from_secs(1))
                .unwrap_err()
                .contains("failed")
        );
        let mut file = tempfile::tempfile().unwrap();
        use std::io::Write;
        file.write_all(b"12345").unwrap();
        assert!(read_bounded(&mut file, 4).is_err());
    }
    #[test]
    fn helper_timeout_stops_the_process() {
        let mut command = Command::new("/bin/sleep");
        command.arg("5");
        assert!(
            run(&mut command, Duration::from_millis(20))
                .unwrap_err()
                .contains("timed out")
        );
    }
    /// Reads only synthetic pixels and invokes the installed local OCR engine.
    #[test]
    #[ignore = "requires the native OCR helper (macOS) or Tesseract (Linux)"]
    fn native_ocr_recognizes_synthetic_text_and_regions() {
        use image::Rgba;
        let font = crate::platform::annotation_font();
        let mut image = RgbaImage::from_pixel(600, 200, Rgba([255, 255, 255, 255]));
        imageproc::drawing::draw_text_mut(
            &mut image,
            Rgba([0, 0, 0, 255]),
            20,
            20,
            40.,
            font,
            "Glance OCR",
        );
        imageproc::drawing::draw_text_mut(
            &mut image,
            Rgba([0, 0, 0, 255]),
            20,
            110,
            40.,
            font,
            "Copy this text",
        );
        let text = extract(&image, None).unwrap();
        assert!(text.contains("Glance OCR"), "{text:?}");
        assert!(text.contains("Copy this text"), "{text:?}");
        assert!(text.contains('\n'), "line breaks must survive: {text:?}");
        let first = extract(&image, Some([0, 0, 600, 85])).unwrap();
        assert!(first.contains("Glance OCR"), "{first:?}");
        assert!(!first.contains("Copy this text"));
        let blank = RgbaImage::from_pixel(200, 100, Rgba([255, 255, 255, 255]));
        assert_eq!(extract(&blank, None).unwrap(), "");
    }
}
