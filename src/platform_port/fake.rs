//! The headless port. A Rune script drives it with marks, on the virtual clock:
//!
//! ```text
//! key:draw-down  key:draw-up  key:keyboard  key:send
//! key:capture                a tap of the capture key, right ⌘
//! key:voice                  a tap of the voice key, right ⌥
//! key:voice-hold  key:voice-release  the voice key held past the threshold, then up
//! key:shift-down  key:shift-up
//! key:enter  key:esc         ↩ or Esc, which reach Lens only while it takes them
//!                            (`pill_keys`); otherwise they go to the app in front,
//!                            recorded in app-keys.jsonl beside the clipboard
//! key:1 … key:6              a digit, with fn when the draw key is down: it reaches
//!                            Lens only with fn down during a capture (`capture_keys`),
//!                            and otherwise goes to the app in front, as above
//! pointer:X:Y                the pointer is at X, Y in global points from now on;
//!                            until then it is at the first display's centre
//! accessibility:off          Accessibility is not allowed from now on
//! screen-recording:off       Screen Recording is not allowed from now on; each time
//!                            Lens would show its guide, a line lands in guides.jsonl
//! screen-recording:on        it is allowed again
//! paste:fail:the reason      the next paste fails with this
//! jobs:hold  jobs:next  jobs:last  jobs:drain  jobs:release
//!                            hold slow work, then run it: see `Script::held`
//! watch:click  watch:scroll  watch:switch  watch:change:0.7
//! screen:0.7                 the screen changes by 0.7 of the area, unseen until
//! watch:sample               the next sample reports it, unless a rebase came first
//! watch:late:scroll          the same events, to the watch that stopped last
//! say:0.5:1.2:the words      a segment from 0.5 s to 1.2 s, while voice runs
//! voice:downloading:3:10  voice:loading  voice:warming  voice:listening
//!                            voice's progress, while it runs
//! voice:fail:the reason      voice stops on its own
//! voice:late:fail:the reason  the last finished voice reports this after its finish returned
//! voice:last:0.5:1.2:words   the sentence in progress, which finishing voice delivers
//! voice:finish-fails:reason  finishing voice fails with this instead
//! clipboard:PATH             the system clipboard now holds the image at PATH
//! ocr:the words              text recognition finds these words from now on
//! screen-text:the words      the front window reads as these words from now on;
//!                            until then it reads as no words
//! screen-text:fail           reading the front window fails from now on; each read
//!                            adds a line to screen-reads.jsonl beside the clipboard
//! pick:#RRGGBB  pick:cancel  answers a Pick from screen that is waiting
//! still:fail                 the next still fails
//! close:editor               presses the window's close button
//! press:bar-edit  press:pick presses the bar's Edit, or picks the display, from a script
//!                            driving another window
//! front:NAME                 the app NAME has the keyboard, as when the user switches to it
//! press:copy-last-dictation  the Dock menu's Copy Last Dictation
//! ```
//!
//! Any other mark is left to the script. Work runs inline, so a tree read at a
//! mark sees every still and send that mark caused.
//!
//! When `ATTN_LENS_FAKE_CLIPBOARD` names a folder, what the app copies lands in
//! it as `image.png` and `text.txt`, so a test can read what a user would paste.
//! Each paste adds a JSON line to `pasted.jsonl` there: `{"text": …}`.
//! `ATTN_LENS_FAKE_VIDEO_ENCODER` names a stand-in for the MP4 helper.
//!
//! The fake counts any Lens window taking the keyboard as Lens becoming the active app.
//! When `ATTN_LENS_FAKE_FOCUS` names a file, each time Lens gives the keyboard back to an
//! app, a line with the app's name lands in it.
use super::{
    Chord, FAKE_TICK, KeyEvent, PillKey, PlatformPort, Press, Sink, Voice, VoiceEvent, VoiceSource,
    VoiceStatus, Watch, WatchEvent,
};
use crate::lens::session::{Area, DisplayFrame, HostTime, Segment};
use image::{Rgba, RgbaImage};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// The display a fake capture covers, in points.
/// The menu bar's height on a Mac display without a notch.
const MENU_BAR: f64 = 24.;

pub const DISPLAY: Area = Area {
    display: 1,
    rect: [0., 0., 640., 400.],
};

#[derive(Default)]
struct SinkSlots {
    watch: Option<Sink<WatchEvent>>,
    /// The watch stopped last: a `watch:late:` mark sends it an event in flight.
    stopped_watch: Option<Sink<WatchEvent>>,
    /// What the next sample reports: the change since the baseline.
    screen_change: f32,
    voice: Option<Sink<VoiceEvent>>,
    /// The voice whose finish returned last, which can still report.
    finished_voice: Option<Sink<VoiceEvent>>,
}

type Sinks = Arc<Mutex<SinkSlots>>;

type PickSink = Sink<Result<Option<[u8; 3]>, String>>;

#[derive(Default)]
struct Script {
    marks: Vec<(String, f64)>,
    fired: usize,
    now: f64,
    keys: Option<Sink<KeyEvent>>,
    clipboard: Option<RgbaImage>,
    ocr: String,
    /// What `screen_text` reads, unless a `screen-text:fail` mark made it fail.
    screen_text: String,
    screen_text_fails: bool,
    fail_still: bool,
    pick: Option<PickSink>,
    /// After a `jobs:hold` mark, jobs wait here until a `jobs:next` mark runs the oldest,
    /// a `jobs:last` mark the newest, a `jobs:drain` mark every one waiting, or a
    /// `jobs:release` mark every one waiting, after which jobs run at once again.
    held: Option<std::collections::VecDeque<Box<dyn FnOnce() + Send>>>,
    buttons: Vec<(&'static str, Sink<()>)>,
    /// The app with the keyboard, set by `front:` marks and taken by Lens.
    front: Option<String>,
    /// The app `hold_front` noted.
    held_front: Option<String>,
    /// Lens takes ↩ and Esc.
    pill_keys: bool,
    /// A capture is open, so Lens takes fn with a digit.
    capture_keys: bool,
    /// The draw key, fn, is down.
    fn_down: bool,
    pointer: Option<(f64, f64)>,
    no_accessibility: bool,
    no_screen_recording: bool,
    paste_error: Option<String>,
}

/// What the fake calls the app with the keyboard while a Lens window has it.
const LENS_FRONT: &str = "Attn Lens";

pub struct FakePort {
    script: Mutex<Script>,
    sinks: Sinks,
    token: Mutex<Option<String>>,
    displays: Mutex<Vec<DisplayFrame>>,
    shift: AtomicBool,
    /// The sentence a finishing voice delivers before it returns.
    last_words: Arc<Mutex<Option<Segment>>>,
    finish_error: Arc<Mutex<Option<String>>>,
}

impl FakePort {
    pub fn new(marks: Vec<(String, f64)>, token: Option<&str>) -> Self {
        Self {
            script: Mutex::new(Script {
                marks,
                ..Default::default()
            }),
            sinks: Default::default(),
            token: Mutex::new(token.map(str::to_string)),
            displays: Mutex::new(vec![DisplayFrame {
                id: DISPLAY.display,
                frame: DISPLAY.rect,
                menu_bar: MENU_BAR,
                dock: 0.,
            }]),
            shift: AtomicBool::new(false),
            last_words: Arc::default(),
            finish_error: Arc::default(),
        }
    }

    /// Script marks are read on the first tick: Rune publishes the run only
    /// after the window factory returns.
    pub fn from_headless_run() -> Self {
        Self::new(vec![], Some("fake-token"))
    }

    fn fire(&self, mark: &str) {
        if let Some(fraction) = mark.strip_prefix("screen:") {
            self.sinks.lock().unwrap().screen_change =
                fraction.parse().expect("screen: mark holds a fraction");
        } else if mark == "watch:sample" {
            let mut sinks = self.sinks.lock().unwrap();
            let fraction = std::mem::take(&mut sinks.screen_change);
            let sink = sinks.watch.clone();
            drop(sinks);
            if let Some(sink) = sink {
                sink(WatchEvent::Change(fraction));
            }
        } else if mark == "key:shift-down" || mark == "key:shift-up" {
            self.shift.store(mark == "key:shift-down", Ordering::SeqCst);
        } else if let Some(key) = pill_key(mark) {
            let script = self.script.lock().unwrap();
            let sink = script.keys.clone().filter(|_| script.pill_keys);
            drop(script);
            match sink {
                Some(sink) => sink(KeyEvent::Pill(key)),
                None => append_line("app-keys.jsonl", serde_json::json!({ "key": mark })),
            }
        } else if let Some(digit) = tool_digit(mark) {
            let script = self.script.lock().unwrap();
            let sink = script
                .keys
                .clone()
                .filter(|_| script.capture_keys && script.fn_down);
            drop(script);
            match sink {
                Some(sink) => sink(KeyEvent::Tool(digit)),
                None => append_line("app-keys.jsonl", serde_json::json!({ "key": mark })),
            }
        } else if let Some(at) = mark.strip_prefix("pointer:") {
            let (x, y) = at.split_once(':').expect("pointer: mark holds X:Y");
            self.script.lock().unwrap().pointer = Some((
                x.parse().expect("pointer: X is a number"),
                y.parse().expect("pointer: Y is a number"),
            ));
        } else if mark == "accessibility:off" {
            self.script.lock().unwrap().no_accessibility = true;
        } else if let Some(on) = mark.strip_prefix("screen-recording:") {
            self.script.lock().unwrap().no_screen_recording = on == "off";
        } else if let Some(reason) = mark.strip_prefix("paste:fail:") {
            self.script.lock().unwrap().paste_error = Some(reason.into());
        } else if let Some(event) = key_event(mark) {
            let mut script = self.script.lock().unwrap();
            match event {
                KeyEvent::DrawDown => script.fn_down = true,
                KeyEvent::DrawUp => script.fn_down = false,
                _ => {}
            }
            let sink = script.keys.clone();
            drop(script);
            if let Some(sink) = sink {
                sink(event);
            }
        } else if let Some(event) = mark
            .strip_prefix("watch:late:")
            .and_then(|event| watch_event(&format!("watch:{event}")))
        {
            let sink = self.sinks.lock().unwrap().stopped_watch.clone();
            if let Some(sink) = sink {
                sink(event);
            }
        } else if let Some(event) = watch_event(mark) {
            let sink = self.sinks.lock().unwrap().watch.clone();
            if let Some(sink) = sink {
                sink(event);
            }
        } else if let Some(reason) = mark.strip_prefix("voice:finish-fails:") {
            *self.finish_error.lock().unwrap() = Some(reason.into());
        } else if let Some(words) = mark.strip_prefix("voice:last:") {
            *self.last_words.lock().unwrap() = segment(&format!("say:{words}"));
        } else if let Some(event) = mark
            .strip_prefix("voice:late:")
            .and_then(|late| voice_event(&format!("voice:{late}")))
        {
            let sink = self.sinks.lock().unwrap().finished_voice.clone();
            if let Some(sink) = sink {
                sink(event);
            }
        } else if let Some(event) = voice_event(mark) {
            let mut sinks = self.sinks.lock().unwrap();
            // Voice that fails has stopped.
            let sink = if matches!(event, VoiceEvent::Failed(_)) {
                sinks.voice.take()
            } else {
                sinks.voice.clone()
            };
            drop(sinks);
            if let Some(sink) = sink {
                sink(event);
            }
        } else if let Some(path) = mark.strip_prefix("clipboard:") {
            let image = image::open(path).expect("clipboard: mark names a readable image");
            self.script.lock().unwrap().clipboard = Some(image.to_rgba8());
        } else if mark == "screen-text:fail" {
            self.script.lock().unwrap().screen_text_fails = true;
        } else if let Some(text) = mark.strip_prefix("screen-text:") {
            let mut script = self.script.lock().unwrap();
            (script.screen_text, script.screen_text_fails) = (text.to_string(), false);
        } else if let Some(text) = mark.strip_prefix("ocr:") {
            self.script.lock().unwrap().ocr = text.to_string();
        } else if mark == "still:fail" {
            self.script.lock().unwrap().fail_still = true;
        } else if mark == "jobs:hold" {
            self.script.lock().unwrap().held = Some(Default::default());
        } else if mark == "jobs:next" || mark == "jobs:last" {
            let job = self.script.lock().unwrap().held.as_mut().and_then(|held| {
                if mark == "jobs:next" {
                    held.pop_front()
                } else {
                    held.pop_back()
                }
            });
            if let Some(job) = job {
                job();
            }
        } else if mark == "jobs:drain" || mark == "jobs:release" {
            let mut script = self.script.lock().unwrap();
            let jobs = if mark == "jobs:release" {
                script.held.take().unwrap_or_default()
            } else {
                script.held.as_mut().map(std::mem::take).unwrap_or_default()
            };
            drop(script);
            for job in jobs {
                job();
            }
        } else if let Some(window) = mark
            .strip_prefix("close:")
            .or_else(|| mark.strip_prefix("press:"))
        {
            let press = self
                .script
                .lock()
                .unwrap()
                .buttons
                .iter()
                .find(|(w, _)| *w == window)
                .map(|(_, press)| press.clone());
            press.expect("close: or press: names a script button")(());
        } else if let Some(app) = mark.strip_prefix("front:") {
            self.script.lock().unwrap().front = Some(app.to_string());
        } else if let Some(answer) = mark.strip_prefix("pick:") {
            let sink = self.script.lock().unwrap().pick.take();
            if let Some(sink) = sink {
                sink(Ok(picked(answer)));
            }
        }
    }
}

fn key_event(mark: &str) -> Option<KeyEvent> {
    Some(match mark.strip_prefix("key:")? {
        "draw-down" => KeyEvent::DrawDown,
        "draw-up" => KeyEvent::DrawUp,
        "capture" => KeyEvent::Pressed(Chord::Capture),
        "voice" => KeyEvent::Voice(Press::Tap),
        "voice-hold" => KeyEvent::Voice(Press::HoldStart),
        "voice-release" => KeyEvent::Voice(Press::HoldEnd),
        "keyboard" => KeyEvent::Pressed(Chord::Keyboard),
        "send" => KeyEvent::Pressed(Chord::Send),
        _ => return None,
    })
}

/// What the app in front received, one JSON line per event, beside the fake clipboard.
fn append_line(name: &str, line: serde_json::Value) {
    use std::io::Write;
    if let Some(path) = clipboard_file(name) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        writeln!(file, "{line}").unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

fn pill_key(mark: &str) -> Option<PillKey> {
    match mark {
        "key:enter" => Some(PillKey::Enter),
        "key:esc" => Some(PillKey::Esc),
        _ => None,
    }
}

fn tool_digit(mark: &str) -> Option<u8> {
    let digit: u8 = mark.strip_prefix("key:")?.parse().ok()?;
    (1..=6).contains(&digit).then_some(digit)
}

fn watch_event(mark: &str) -> Option<WatchEvent> {
    let event = mark.strip_prefix("watch:")?;
    Some(match event {
        "click" => WatchEvent::Click,
        "scroll" => WatchEvent::Scroll,
        "switch" => WatchEvent::WindowSwitch,
        _ => WatchEvent::Change(event.strip_prefix("change:")?.parse().ok()?),
    })
}

fn voice_event(mark: &str) -> Option<VoiceEvent> {
    if let Some(segment) = segment(mark) {
        return Some(VoiceEvent::Segment(segment));
    }
    let event = mark.strip_prefix("voice:")?;
    if let Some(reason) = event.strip_prefix("fail:") {
        return Some(VoiceEvent::Failed(reason.into()));
    }
    Some(VoiceEvent::Status(match event {
        "loading" => VoiceStatus::Loading,
        "warming" => VoiceStatus::WarmingUp,
        "listening" => VoiceStatus::Listening,
        _ => {
            let mut parts = event.strip_prefix("downloading:")?.split(':');
            VoiceStatus::Downloading {
                bytes: parts.next()?.parse().ok()?,
                total: parts.next()?.parse().ok()?,
            }
        }
    }))
}

fn segment(mark: &str) -> Option<Segment> {
    let mut parts = mark.strip_prefix("say:")?.splitn(3, ':');
    let start = parts.next()?.parse().ok()?;
    let end = parts.next()?.parse().ok()?;
    Some(Segment {
        start: HostTime::from_secs(start),
        end: HostTime::from_secs(end),
        text: parts.next()?.to_string(),
    })
}

fn picked(answer: &str) -> Option<[u8; 3]> {
    let hex = u32::from_str_radix(answer.strip_prefix('#')?, 16).ok()?;
    Some([(hex >> 16) as u8, (hex >> 8) as u8, hex as u8])
}

/// Writes `line` to the log a test named in `ATTN_LENS_FAKE_FOCUS`: who Lens gave
/// the keyboard back to.
fn focus_log(line: &str) {
    if let Some(path) = std::env::var_os("ATTN_LENS_FAKE_FOCUS") {
        use std::io::Write;
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("ATTN_LENS_FAKE_FOCUS names a file that can be written");
        writeln!(log, "{line}").expect("write the focus log");
    }
}

/// Where the fake's clipboard writes land, when a test asked for them.
fn clipboard_file(name: &str) -> Option<std::path::PathBuf> {
    Some(std::path::Path::new(&std::env::var_os("ATTN_LENS_FAKE_CLIPBOARD")?).join(name))
}

impl PlatformPort for FakePort {
    fn now(&self) -> HostTime {
        HostTime::from_secs(self.script.lock().unwrap().now)
    }
    fn keys(&self, sink: Sink<KeyEvent>) {
        self.script.lock().unwrap().keys = Some(sink);
    }
    fn capture_keys(&self, open: bool) {
        self.script.lock().unwrap().capture_keys = open;
    }
    fn hold_front(&self) {
        let mut script = self.script.lock().unwrap();
        if let Some(app) = script.front.take().filter(|app| app != LENS_FRONT) {
            script.held_front = Some(app);
        }
        script.front = Some(LENS_FRONT.into());
    }
    fn return_front(&self) {
        let mut script = self.script.lock().unwrap();
        let Some(app) = script.held_front.take() else {
            return;
        };
        if script.front.as_deref() != Some(LENS_FRONT) {
            return;
        }
        script.front = Some(app.clone());
        drop(script);
        focus_log(&app);
    }
    fn release_keys(&self) -> bool {
        focus_log("released");
        true
    }
    fn pill_keys(&self, on: bool) {
        self.script.lock().unwrap().pill_keys = on;
    }
    fn pointer(&self) -> (f64, f64) {
        let set = self.script.lock().unwrap().pointer;
        set.unwrap_or_else(|| {
            let display = self.displays.lock().unwrap()[0].frame;
            (display[0] + display[2] / 2., display[1] + display[3] / 2.)
        })
    }
    fn pointer_display(&self) -> Result<Area, String> {
        Ok(DISPLAY)
    }
    fn displays(&self) -> Vec<DisplayFrame> {
        self.displays.lock().unwrap().clone()
    }
    /// The first display is the main one and has the menu bar, as on a Mac.
    fn launch_displays(&self, mut displays: Vec<DisplayFrame>) {
        if let Some(main) = displays.first_mut() {
            main.menu_bar = MENU_BAR;
        }
        *self.displays.lock().unwrap() = displays;
    }
    fn shift_held(&self) -> bool {
        self.shift.load(Ordering::SeqCst)
    }
    fn screen_recording(&self) -> bool {
        let allowed = self.screen_recording_allowed();
        if !allowed {
            append_line(
                "guides.jsonl",
                serde_json::json!({ "guide": "screen-recording" }),
            );
        }
        allowed
    }
    fn screen_recording_allowed(&self) -> bool {
        !self.script.lock().unwrap().no_screen_recording
    }
    /// Every word: only an identifier's shape (a path, camelCase, snake_case, …)
    /// makes it worth spelling here, the same on every machine.
    fn ordinary_word(&self, _word: &str) -> bool {
        true
    }
    fn accessibility(&self) -> bool {
        !self.script.lock().unwrap().no_accessibility
    }
    fn still(&self, area: Area) -> Result<RgbaImage, String> {
        if std::mem::take(&mut self.script.lock().unwrap().fail_still) {
            return Err("The fake still failed.".into());
        }
        // Two pixels per point, as on a Retina display.
        let (width, height) = ((area.rect[2] * 2.) as u32, (area.rect[3] * 2.) as u32);
        Ok(RgbaImage::from_fn(width, height, |x, y| {
            Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        }))
    }
    fn screen_text(&self) -> Result<String, String> {
        let script = self.script.lock().unwrap();
        let read = if script.no_screen_recording {
            Err("Screen Recording is not allowed for Attn Lens.".to_string())
        } else if script.screen_text_fails {
            Err("The fake screen read failed.".into())
        } else {
            Ok(script.screen_text.clone())
        };
        drop(script);
        append_line(
            "screen-reads.jsonl",
            match &read {
                Ok(text) => serde_json::json!({ "text": text }),
                Err(error) => serde_json::json!({ "error": error }),
            },
        );
        read
    }
    fn watch(&self, _area: Area, sink: Sink<WatchEvent>) -> Watch {
        self.sinks.lock().unwrap().watch = Some(sink);
        let sinks = self.sinks.clone();
        let rebase = sinks.clone();
        Watch::new(
            move || {
                let mut sinks = sinks.lock().unwrap();
                sinks.stopped_watch = sinks.watch.take();
            },
            move || rebase.lock().unwrap().screen_change = 0.,
        )
    }
    fn voice(&self, _source: VoiceSource, sink: Sink<VoiceEvent>) -> Result<Voice, String> {
        let mut sinks = self.sinks.lock().unwrap();
        // The helper holds one recording at a time.
        if sinks.voice.is_some() {
            return Err("Voice is already running.".into());
        }
        sinks.voice = Some(sink.clone());
        drop(sinks);
        let (sinks, last_words, finish_error) = (
            self.sinks.clone(),
            self.last_words.clone(),
            self.finish_error.clone(),
        );
        Ok(Voice(Box::new(move || {
            // After `end_voice` the slot may hold the next voice, on its own helper;
            // this one still delivers its last sentence, as the real helper does.
            {
                let mut sinks = sinks.lock().unwrap();
                sinks.voice.take_if(|current| Arc::ptr_eq(current, &sink));
                sinks.finished_voice = Some(sink.clone());
            }
            if let Some(error) = finish_error.lock().unwrap().take() {
                return Err(error);
            }
            if let Some(words) = last_words.lock().unwrap().take() {
                sink(VoiceEvent::Segment(words));
            }
            Ok(())
        })))
    }
    fn end_voice(&self) {
        self.sinks.lock().unwrap().voice = None;
    }
    fn token(&self) -> Option<String> {
        self.token.lock().unwrap().clone()
    }
    fn store_token(&self, token: &str) -> Result<(), String> {
        *self.token.lock().unwrap() = Some(token.to_string());
        Ok(())
    }
    fn forget_token(&self) {
        *self.token.lock().unwrap() = None;
    }
    fn copy_image(&self, image: RgbaImage) -> Result<(), String> {
        if let Some(path) = clipboard_file("image.png") {
            image.save(path).map_err(|e| e.to_string())?;
        }
        self.script.lock().unwrap().clipboard = Some(image);
        Ok(())
    }
    fn clipboard_image(&self) -> Result<RgbaImage, String> {
        self.script
            .lock()
            .unwrap()
            .clipboard
            .clone()
            .ok_or_else(|| "The clipboard doesn’t contain an image.".into())
    }
    fn copy_text(&self, text: &str) -> Result<(), String> {
        if let Some(path) = clipboard_file("text.txt") {
            std::fs::write(path, text).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn paste(&self, text: &str) -> Result<(), String> {
        if let Some(error) = self.script.lock().unwrap().paste_error.take() {
            return Err(error);
        }
        append_line("pasted.jsonl", serde_json::json!({ "text": text }));
        Ok(())
    }
    fn ocr(&self, _image: &RgbaImage, _rectangle: Option<[u32; 4]>) -> Result<String, String> {
        Ok(self.script.lock().unwrap().ocr.clone())
    }
    fn exposes_state(&self) -> bool {
        true
    }
    fn lane(&self) -> super::Lane {
        super::Lane::inline()
    }
    /// `ATTN_LENS_FAKE_VIDEO_ENCODER` names a stand-in for the helper.
    fn video_encoder(&self) -> Result<std::path::PathBuf, String> {
        match std::env::var_os("ATTN_LENS_FAKE_VIDEO_ENCODER") {
            Some(path) => Ok(path.into()),
            None => crate::video::encoder_path(),
        }
    }
    fn scene_mark(&self, mark: &str) -> Result<(), String> {
        self.fire(mark);
        Ok(())
    }
    fn script_button(&self, name: &'static str, press: Sink<()>) {
        self.script.lock().unwrap().buttons.push((name, press));
    }
    fn pick_color(&self, sink: PickSink) {
        self.script.lock().unwrap().pick = Some(sink);
    }
    fn run(&self, job: Box<dyn FnOnce() + Send>) {
        let mut script = self.script.lock().unwrap();
        match &mut script.held {
            Some(held) => held.push_back(job),
            None => {
                drop(script);
                job();
            }
        }
    }
    fn tick(&self, now: f64) -> Option<Duration> {
        let run = rune::tooling::headless_run();
        loop {
            let mark = {
                let mut script = self.script.lock().unwrap();
                script.now = script.now.max(now);
                if script.marks.is_empty()
                    && let Some(run) = run
                {
                    script.marks = run.marks().map(|(n, t)| (n.to_string(), t)).collect();
                }
                let fired = script.fired;
                match script.marks.get(fired).filter(|(_, t)| *t <= now).cloned() {
                    Some(mark) => {
                        script.fired += 1;
                        mark.0
                    }
                    None => return Some(FAKE_TICK),
                }
            };
            self.fire(&mark);
        }
    }
}
