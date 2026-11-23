//! The development socket that lets an agent prove Mac checks: it reports input
//! mode, stills with their causes, and segments, and starts what a key would.
//! It exists only when Lens is launched with `--automation`.
//!
//! One JSON request per line, one JSON reply per line:
//!
//! ```text
//! {"cmd":"state"}
//! {"cmd":"wait","since":0,"timeout_ms":1000}          blocks until the state changes after generation
//!                                                   `since`, or the timeout; replies with `generation` and `state`
//! {"cmd":"key","key":"capture"|"voice"|"voice-hold"|"voice-release"|"keyboard"|"send"|"draw-down"|"draw-up"}
//! {"cmd":"pick"}                                    the whole display under the pointer
//! {"cmd":"pick","display":1,"rect":[x,y,w,h]}       rect is optional: the whole display
//! {"cmd":"pick-last"}
//! {"cmd":"cancel"}
//! {"cmd":"still","cause":"hotkey"}
//! {"cmd":"stroke","points":[[x,y],...]}             what a drag on the ink layer does, in points
//!                                                   from the area's corner; needs draw-down first
//! {"cmd":"sessions"}
//! {"cmd":"send","target":"<session id>","message":"..."}
//! {"cmd":"end"}
//! {"cmd":"edit"}                                    the newest still in the editor; "still":<id> names one
//! {"cmd":"back"}                                    back to the capture; "send":true also sends
//! {"cmd":"reopen"}                                  the tile's capture
//! {"cmd":"discard"}                                 the tile
//! {"cmd":"voice-wav","path":"/abs/16k-mono.wav"}    the next voice plays it, not the microphone
//! {"cmd":"voice-wav"}                               back to the microphone
//! {"cmd":"screen_text"}                             reads the front window on a worker; replies
//!                                                   with its "text" and "ms", or the read's error
//! ```
//! A reply is `{"ok":true,...}` or `{"ok":false,"error":"..."}`. The state includes
//! `settings` (each setting by its field name, such as `screen_words`), and
//! `settings_error`, why the last change could not be saved, or null.
use super::{Lens, session::Cause};
use crate::platform_port::{Chord, KeyEvent, Press};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// The socket lives in a directory only this user can enter.
pub fn socket_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is unavailable")?;
    let dir = if cfg!(target_os = "macos") {
        PathBuf::from(home).join("Library/Caches/dev.attn.lens/automation")
    } else {
        PathBuf::from(home).join(".cache/attn-lens/automation")
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    Ok(dir.join("lens.sock"))
}

/// Serves `path` on background threads until the process exits.
pub fn listen(lens: Arc<Lens>, path: &Path) -> Result<(), String> {
    if UnixStream::connect(path).is_ok() {
        return Err(format!(
            "Another Attn Lens is already serving {}.",
            path.display()
        ));
    }
    // Nothing answers, so any file here is left from a process that has gone.
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("{}: {error}", path.display())),
    }
    let listener = UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let lens = lens.clone();
            std::thread::spawn(move || serve(&lens, stream));
        }
    });
    Ok(())
}

fn serve(lens: &Arc<Lens>, stream: UnixStream) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let reply = match serde_json::from_str(&line) {
            Ok(request) => handle(lens, &request),
            Err(error) => Err(format!("Not JSON: {error}")),
        };
        let reply = match reply {
            Ok(mut value) => {
                value["ok"] = json!(true);
                value
            }
            Err(error) => json!({"ok": false, "error": error}),
        };
        if writeln!(writer, "{reply}").is_err() {
            return;
        }
    }
}

fn handle(lens: &Arc<Lens>, request: &Value) -> Result<Value, String> {
    let text = |name: &str| request[name].as_str();
    match text("cmd").ok_or("The request needs a \"cmd\".")? {
        "state" => Ok(json!({"state": lens.state_json()})),
        "wait" => {
            let number = |key: &str| {
                request[key]
                    .as_u64()
                    .ok_or_else(|| format!("wait needs a whole number \"{key}\""))
            };
            let timeout = Duration::from_millis(number("timeout_ms")?);
            let generation = lens.wait_change(number("since")?, timeout);
            Ok(json!({"generation": generation, "state": lens.state_json()}))
        }
        "key" => {
            lens.key(parse_key(text("key").ok_or("key needs a \"key\".")?)?);
            Ok(json!({}))
        }
        "pick" => {
            match request["display"].as_u64() {
                Some(display) => {
                    let rect = match &request["rect"] {
                        Value::Null => None,
                        rect => Some(
                            serde_json::from_value(rect.clone())
                                .map_err(|_| "rect needs [x, y, width, height].")?,
                        ),
                    };
                    lens.pick(display as u32, rect)?;
                }
                None => lens.pick_pointer_display()?,
            }
            Ok(json!({}))
        }
        "pick-last" => lens
            .pick_last()
            .then(|| json!({}))
            .ok_or_else(|| "No last area to use.".to_string()),
        "edit" => {
            match request["still"].as_u64() {
                Some(still) => lens.edit_still(still),
                None => lens.edit_latest(),
            }
            Ok(json!({}))
        }
        "back" => {
            lens.leave_editor(request["send"].as_bool().unwrap_or(false));
            Ok(json!({}))
        }
        "reopen" => {
            lens.reopen_tile();
            Ok(json!({}))
        }
        "discard" => {
            lens.discard_tile();
            Ok(json!({}))
        }
        "cancel" => {
            lens.cancel_pick();
            Ok(json!({}))
        }
        "still" => {
            lens.take_still(parse_cause(text("cause").unwrap_or("hotkey"))?);
            Ok(json!({}))
        }
        "stroke" => {
            let points: Vec<(f32, f32)> = serde_json::from_value(request["points"].clone())
                .map_err(|_| "stroke needs \"points\": [[x, y], ...].")?;
            let area = lens
                .read(|summary| summary.capture.as_ref().map(|c| c.area))
                .ok_or("stroke needs an open capture.")?;
            let start = lens
                .begin_stroke()
                .ok_or("stroke needs the draw key held: send key draw-down first.")?;
            let on_display = points
                .iter()
                .map(|(x, y)| (x + area.rect[0] as f32, y + area.rect[1] as f32))
                .collect();
            lens.finish_stroke(start, on_display);
            Ok(json!({}))
        }
        "sessions" => Ok(json!({"sessions": lens.sessions()?.iter().map(|s| json!({
            "id": s.id,
            "label": s.label,
            "agent": s.agent,
            "state": s.state,
            "host": s.host,
        })).collect::<Vec<_>>()})),
        "send" => {
            lens.send(
                text("target").map(str::to_string),
                text("message").map(str::to_string),
            )?;
            Ok(json!({}))
        }
        "end" => {
            lens.end_capture();
            Ok(json!({}))
        }
        "voice-wav" => {
            lens.set_voice_wav(text("path").map(voice_wav).transpose()?);
            Ok(json!({}))
        }
        "screen_text" => {
            let port = lens.port().clone();
            let (reply, read) = std::sync::mpsc::channel();
            lens.port().run(Box::new(move || {
                let started = std::time::Instant::now();
                let text = port
                    .front_window()
                    .ok_or_else(|| "No window is in front.".to_string())
                    .and_then(|window| port.screen_text(window));
                let _ = reply.send((text, started.elapsed()));
            }));
            let (text, took) = read.recv().map_err(|_| "The screen read stopped.")?;
            Ok(json!({"text": text?, "ms": took.as_millis() as u64}))
        }
        other => Err(format!(
            "Unknown cmd \"{other}\". Expected state, key, pick, pick-last, cancel, still, stroke, sessions, send, end, voice-wav or screen_text."
        )),
    }
}

/// A WAV for voice must be a regular local file, named by its absolute path.
fn voice_wav(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(format!(
            "voice-wav needs an absolute path, not {}.",
            path.display()
        ));
    }
    let meta = std::fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.file_type().is_file() {
        return Err(format!("{} is not a regular file.", path.display()));
    }
    Ok(path)
}

fn parse_key(name: &str) -> Result<KeyEvent, String> {
    Ok(match name {
        "capture" => KeyEvent::Pressed(Chord::Capture),
        "voice" => KeyEvent::Voice(Press::Tap),
        "voice-hold" => KeyEvent::Voice(Press::HoldStart),
        "voice-release" => KeyEvent::Voice(Press::HoldEnd),
        "keyboard" => KeyEvent::Pressed(Chord::Keyboard),
        "send" => KeyEvent::Pressed(Chord::Send),
        "draw-down" => KeyEvent::DrawDown,
        "draw-up" => KeyEvent::DrawUp,
        other => {
            return Err(format!(
                "Unknown key \"{other}\". Expected capture, voice, voice-hold, voice-release, keyboard, send, draw-down or draw-up."
            ));
        }
    })
}

fn parse_cause(name: &str) -> Result<Cause, String> {
    Ok(match name {
        "draw" => Cause::Draw,
        "hotkey" => Cause::Hotkey,
        "click" => Cause::Click,
        "window_switch" => Cause::WindowSwitch,
        "send" => Cause::Send,
        other => {
            return Err(format!(
                "Unknown cause \"{other}\". Expected draw, hotkey, click, window_switch or send."
            ));
        }
    })
}

/// Sends one request to a running Lens and returns the reply line.
pub fn request(path: &Path, line: &str) -> Result<String, String> {
    let mut stream = UnixStream::connect(path).map_err(|e| {
        format!(
            "Cannot reach Attn Lens at {}: {e}. Launch it with --automation.",
            path.display()
        )
    })?;
    writeln!(stream, "{line}").map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|e| e.to_string())?;
    Ok(reply.trim_end().to_string())
}
