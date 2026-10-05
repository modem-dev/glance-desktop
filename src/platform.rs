use image::{ImageReader, RgbaImage};
use std::{path::PathBuf, process::Command};
mod color_sampler;
pub use color_sampler::sample_screen_color;
pub fn load(path: &std::path::Path) -> Result<RgbaImage, String> {
    let reader = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16000);
    limits.max_image_height = Some(16000);
    limits.max_alloc = Some(512 * 1024 * 1024);
    let mut reader = reader;
    reader.limits(limits);
    reader
        .decode()
        .map(|i| i.to_rgba8())
        .map_err(|e| e.to_string())
}
#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}
/// Called only when the user requests capture, before hiding the editor.
#[cfg(target_os = "macos")]
pub fn screen_capture_permission() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    unsafe {
        if CGPreflightScreenCaptureAccess() {
            return Ok(());
        }
        // Let macOS present its normal permission request for a first-time grant.
        let _ = CGRequestScreenCaptureAccess();
        if CGPreflightScreenCaptureAccess() {
            return Ok(());
        }
    }
    Err("macOS is not authorizing this Glance build to record the screen. Open System Settings → Privacy & Security → Screen & System Audio Recording. If Glance is already enabled, quit Glance, remove its entry with −, add /Applications/Glance.app with +, enable it, then reopen. Rebuilding an ad-hoc signed app can invalidate an older permission.".into())
}
fn capture_failure(area: bool, stderr: &[u8], code: Option<i32>) -> Option<String> {
    let detail = String::from_utf8_lossy(stderr);
    let detail = detail.trim();
    if area && detail.is_empty() && matches!(code, Some(0 | 1)) {
        return None;
    }
    Some(if detail.is_empty() {
        format!(
            "Screen capture produced no image (exit {}). Try again with the screen unlocked.",
            code.map_or("unknown".into(), |c| c.to_string())
        )
    } else {
        format!("Screen capture failed: {detail}")
    })
}
#[cfg(target_os = "macos")]
pub fn capture(area: bool) -> Result<Option<RgbaImage>, String> {
    // A private directory prevents other local accounts from planting a
    // symlink at the capture path or reading the unedited screenshot.
    let capture_dir = tempfile::Builder::new()
        .prefix("glance-capture-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let path = capture_dir.path().join("capture.png");
    std::thread::sleep(std::time::Duration::from_millis(250));
    let mut command = Command::new("/usr/sbin/screencapture");
    command.args(["-x", "-t", "png"]);
    if area {
        command.args(["-i", "-s"]);
    } else {
        command.arg("-m");
    }
    let output = command.arg(&path).output().map_err(|e| e.to_string())?;
    let result = if path.exists() {
        load(&path).map(Some)
    } else if let Some(error) = capture_failure(area, &output.stderr, output.status.code()) {
        Err(error)
    } else {
        Ok(None)
    };
    let _ = std::fs::remove_file(path);
    result
}
#[cfg(target_os = "macos")]
fn dialog(script: &str) -> Result<Option<PathBuf>, String> {
    let output = Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return if error.contains("-128") {
            Ok(None)
        } else {
            Err(error.trim().to_string())
        };
    }
    let path = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    Ok(Some(PathBuf::from(path.trim_end())))
}
#[cfg(target_os = "macos")]
pub fn open() -> Result<Option<RgbaImage>, String> {
    match dialog(
        "POSIX path of (choose file with prompt \"Open an image in Glance\" of type {\"public.png\", \"public.jpeg\"})",
    )? {
        Some(path) => load(&path).map(Some),
        None => Ok(None),
    }
}
pub fn save(image: RgbaImage) -> Result<Option<PathBuf>, String> {
    let Some(path) = export_destination(ExportFormat::Png)? else {
        return Ok(None);
    };
    image
        .save_with_format(&path, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(Some(path))
}
#[cfg(target_os = "macos")]
pub fn copy(image: RgbaImage) -> Result<(), String> {
    arboard::Clipboard::new()
        .map_err(|e| e.to_string())?
        .set_image(arboard::ImageData {
            width: image.width() as usize,
            height: image.height() as usize,
            bytes: std::borrow::Cow::Owned(image.into_raw()),
        })
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
pub fn clipboard_image() -> Result<RgbaImage, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let data = clipboard
        .get_image()
        .map_err(|_| "The clipboard doesn’t contain an image.".to_string())?;
    if data.width > 16000
        || data.height > 16000
        || data.width.saturating_mul(data.height) > 64_000_000
    {
        return Err("Clipboard image is too large.".into());
    }
    RgbaImage::from_raw(
        data.width as u32,
        data.height as u32,
        data.bytes.into_owned(),
    )
    .ok_or_else(|| "Invalid clipboard image.".into())
}

enum ExportFormat {
    Png,
    Gif,
    Mp4,
}

#[cfg(target_os = "macos")]
fn export_destination(format: ExportFormat) -> Result<Option<PathBuf>, String> {
    let (prompt, name, extension, folder) = match format {
        ExportFormat::Png => ("Save annotated screenshot", "Screenshot", "png", "pictures"),
        ExportFormat::Gif => (
            "Export animated backdrop GIF",
            "Animated Screenshot",
            "gif",
            "pictures",
        ),
        ExportFormat::Mp4 => (
            "Export animated backdrop video",
            "Animated Screenshot",
            "mp4",
            "movies",
        ),
    };
    // Use local time and filename-safe separators, like macOS screenshots.
    // All interpolated values are constants, never user-provided filenames.
    let script = format!(
        r#"set timestamp to do shell script "/bin/date '+%Y-%m-%d at %H.%M.%S'"
set exportName to "{name} " & timestamp & ".{extension}"
POSIX path of (choose file name with prompt "{prompt}" default name exportName default location (path to {folder} folder))"#
    );
    dialog(&script)?
        .map(|path| checked_export_path(path, extension))
        .transpose()
}

// Changing a save-dialog path can bypass its overwrite confirmation.
// An explicitly chosen filename may be replaced; an appended extension must
// never turn an unconfirmed choice into a replacement (including symlinks).
fn checked_export_path(mut path: PathBuf, extension: &str) -> Result<PathBuf, String> {
    if path.extension().is_none() {
        path.set_extension(extension);
        match std::fs::symlink_metadata(&path) {
            Ok(_) => return Err("That filename already exists; choose the full filename in the save dialog to confirm replacement.".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(e) => return Err(e.to_string()),
        }
    } else if path.extension().is_none_or(|ext| ext != extension) {
        return Err(format!("Choose a filename ending in .{extension}"));
    }
    Ok(path)
}

pub fn animation_destination(gif: bool) -> Result<Option<PathBuf>, String> {
    export_destination(if gif {
        ExportFormat::Gif
    } else {
        ExportFormat::Mp4
    })
}
#[cfg(target_os = "macos")]
pub const ANNOTATION_FONT: &str = "Arial";
#[cfg(target_os = "linux")]
pub const ANNOTATION_FONT: &str = "DejaVu Sans";
#[cfg(target_os = "macos")]
pub const UI_FONT: &str = ".AppleSystemUIFont";
#[cfg(target_os = "linux")]
pub const UI_FONT: &str = "DejaVu Sans";

pub fn annotation_font() -> Option<&'static ab_glyph::FontArc> {
    static FONT: std::sync::OnceLock<Option<ab_glyph::FontArc>> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        #[cfg(target_os = "macos")]
        let paths = ["/System/Library/Fonts/Supplemental/Arial.ttf"];
        #[cfg(target_os = "linux")]
        let paths = [
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ];
        paths
            .iter()
            .find_map(|path| ab_glyph::FontArc::try_from_vec(std::fs::read(path).ok()?).ok())
    })
    .as_ref()
}

pub fn key_binding(key: &str) -> String {
    if cfg!(target_os = "linux") {
        key.replace("cmd-", "ctrl-")
    } else {
        key.into()
    }
}
pub fn command_pressed(modifiers: gpui::Modifiers) -> bool {
    modifiers.platform || (cfg!(target_os = "linux") && modifiers.control)
}
pub fn shortcut_label(label: &str) -> String {
    if cfg!(target_os = "linux") {
        label
            .replace('⌘', "Ctrl+")
            .replace('⌥', "Alt+")
            .replace('⇧', "Shift+")
    } else {
        label.into()
    }
}
pub fn show_editor(cx: &mut gpui::App) {
    #[cfg(target_os = "macos")]
    cx.activate(true);
    #[cfg(target_os = "linux")]
    for handle in cx.windows() {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
    }
}
pub fn hide_editor(cx: &mut gpui::App) {
    #[cfg(target_os = "macos")]
    cx.hide();
    #[cfg(target_os = "linux")]
    // A toolbar listener already holds the current window. Defer so the
    // window is available again before issuing the compositor request.
    cx.defer(|cx| {
        for handle in cx.windows() {
            let _ = handle.update(cx, |_, window, _| window.minimize_window());
        }
    });
}

pub enum Startup {
    Demo,
    Image(RgbaImage),
    Exit,
}
pub fn startup_image(args: impl Iterator<Item = String>) -> Result<Startup, String> {
    let mut args = args;
    let mut request = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--automation" => {}
            "--help" | "-h" => {
                println!(
                    "Glance {}\nUsage: glance desktop [--open PATH | --capture-area | --capture-screen] [--automation]\n       glance --mcp",
                    env!("CARGO_PKG_VERSION")
                );
                return Ok(Startup::Exit);
            }
            "--open" | "--capture-area" | "--capture-screen" => {
                if request.is_some() {
                    return Err("Choose only one startup image or capture action".into());
                }
                request = Some((
                    arg.clone(),
                    if arg == "--open" {
                        Some(args.next().ok_or("--open requires a path")?)
                    } else {
                        None
                    },
                ));
            }
            _ if arg.starts_with("-psn_") => {} // Older Finder launch argument.
            _ => return Err(format!("Unknown argument: {arg}. Use --help.")),
        }
    }
    match request {
        None => Ok(Startup::Demo),
        Some((action, Some(path))) if action == "--open" => {
            load(std::path::Path::new(&path)).map(Startup::Image)
        }
        Some((action, _)) => {
            screen_capture_permission()?;
            Ok(capture(action == "--capture-area")?.map_or(Startup::Exit, Startup::Image))
        }
    }
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{capture, clipboard_image, copy, open, permission as screen_capture_permission};
#[cfg(target_os = "linux")]
fn export_destination(format: ExportFormat) -> Result<Option<PathBuf>, String> {
    linux::destination(format)
}
#[cfg(test)]
mod tests {
    use super::capture_failure;
    use super::checked_export_path;
    #[test]
    fn startup_rejects_missing_or_conflicting_image_arguments() {
        for args in [
            vec!["--open"],
            vec!["--capture-area", "--capture-screen"],
            vec!["--unknown"],
        ] {
            assert!(super::startup_image(args.into_iter().map(str::to_string)).is_err());
        }
        assert!(matches!(
            super::startup_image(vec!["--automation".to_string()].into_iter()).unwrap(),
            super::Startup::Demo
        ));
    }
    #[test]
    fn exported_text_uses_an_available_platform_font() {
        assert!(
            super::annotation_font().is_some(),
            "Install the platform annotation font"
        );
        let mut document = crate::document::Document::new(image::RgbaImage::from_pixel(
            200,
            60,
            image::Rgba([255, 255, 255, 255]),
        ));
        document.commit(crate::document::Mark {
            style: Default::default(),
            tool: crate::document::Tool::Text,
            curve: None,
            points: vec![(10., 10.)],
            color: [0, 0, 0, 255],
            width: 4.,
            text: "Glance".into(),
        });
        assert!(
            document.render(None).pixels().any(|pixel| pixel[0] < 128),
            "Text must survive export"
        );
    }
    #[test]
    fn appended_export_extensions_cannot_bypass_overwrite_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        for extension in ["png", "gif", "mp4"] {
            let chosen = dir.path().join(format!("screenshot.{extension}"));
            std::fs::write(&chosen, b"original").unwrap();
            assert!(checked_export_path(dir.path().join("screenshot"), extension).is_err());
            assert_eq!(
                checked_export_path(chosen.clone(), extension).unwrap(),
                chosen
            );
            assert_eq!(std::fs::read(&chosen).unwrap(), b"original");
            assert!(checked_export_path(dir.path().join("screenshot.other"), extension).is_err());
            let new = dir.path().join(format!("new-{extension}"));
            assert_eq!(
                checked_export_path(new.clone(), extension).unwrap(),
                new.with_extension(extension)
            );
        }
        let link = dir.path().join("dangling.png");
        std::os::unix::fs::symlink(dir.path().join("missing"), &link).unwrap();
        assert!(checked_export_path(dir.path().join("dangling"), "png").is_err());
        assert!(link.is_symlink());
    }
    #[test]
    fn capture_errors_preserve_real_cause_and_cancellation_is_quiet() {
        assert!(capture_failure(true, b"", Some(1)).is_none());
        assert!(capture_failure(true, b" \n", Some(0)).is_none());
        assert!(
            capture_failure(false, b"", Some(1))
                .unwrap()
                .contains("produced no image")
        );
        assert!(
            capture_failure(true, b"could not create image from display", Some(1))
                .unwrap()
                .contains("could not create image from display")
        );
        assert!(capture_failure(true, b"", None).is_some());
    }
}
