//! Attn Lens's live capture: what a capture holds, and the state every window
//! and the automation socket read. Input arrives through the platform port and
//! changes state here; each change wakes the windows, because a Rune window does
//! not redraw from another window's change.
pub mod attn;
pub mod automation;
pub mod bar;
#[cfg(target_os = "macos")]
pub mod capture;
pub mod deliver;
pub mod dictation;
pub mod ink;
pub mod pill;
pub mod refiner;
pub(crate) mod scenes;
pub mod screen_words;
pub mod select;
pub mod session;
pub mod strip;
pub mod tile;
pub mod voice;
#[cfg(target_os = "macos")]
pub mod watch;

use crate::{
    document::Document,
    platform_port::{
        Chord, KeyEvent, PlatformPort, Press, Sink, Voice, VoiceEvent, VoiceSource, VoiceStatus,
        Watch, WatchEvent,
    },
    settings::Settings,
};
use dictation::{Dictation, DictationSummary};
use ink::InkTool;
use rune::prefs::Prefs;
use serde_json::{Value, json};
use session::{
    Area, Capture, CaptureVoice, Cause, DisplayFrame, HostTime, InkMark, RunningVoice, Segment,
    Still, VoiceOwner,
};
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::Duration,
};

/// The fraction of the area that counts as a new screen (D8 on the plan), past the
/// largest change that is not one. Measured on Safari pages (C5 on s-dqjzpt): 250 ms
/// apart, a big scroll changes up to 0.50, a tab between similar articles 0.35,
/// playing video 0.39, a blinking cursor 0.0004. Video cuts reach 0.96 and do count.
const CHANGE_FRACTION: f32 = 0.6;

/// How long ink takes to fade: a feel choice, judged in the user's session (C15).
const FADE: std::time::Duration = std::time::Duration::from_millis(250);

/// How long the tile of a sent capture stays (D13 on the plan).
const TILE_STAYS: Duration = Duration::from_secs(5);

/// What Lens asks of the editor window, which owns a still while it is open.
pub enum EditorCall {
    Open {
        still: u64,
        document: Box<Document>,
    },
    /// Back to the capture, then Send if `send`.
    Leave {
        send: bool,
    },
}

/// How a capture closed, which decides what is left of it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Closing {
    Esc,
    Sent,
}

/// A closed capture, kept in the corner until it leaves or the user acts on it.
struct Tile {
    capture: Capture,
    sent: bool,
    /// When it leaves. None while hovered, and always for an unsent capture.
    leaves: Option<HostTime>,
}

/// Whether the ink layer takes the pointer: `All` while the draw key is down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Input {
    #[default]
    None,
    All,
}

/// The last send, whichever capture it belonged to.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SendStatus {
    #[default]
    Idle,
    Sending,
    Sent(String),
    /// attn's error, verbatim. The capture stays whole.
    Failed(String),
}

const CLOSED_BEFORE_SENT: &str = "The capture was closed before it was sent.";

/// What the bar shows of voice.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum VoiceState {
    #[default]
    Off,
    /// The voice key was pressed and the port has not reported progress yet.
    Starting,
    Running(VoiceStatus),
    /// Waiting for the sentence in progress, before voice stops or the send goes.
    Finishing,
}

/// Who gets the user's typing during a capture (D6 on the plan).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyOwner {
    #[default]
    Bar,
    Page,
}

/// What Lens knows about reaching attn.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum AttnStatus {
    #[default]
    Loading,
    Ready,
    /// Lens holds no token, or attn refused it. ↩ pairs.
    Unpaired(String),
    /// Waiting for the user to allow Lens in attn; the code shows once attn gives it.
    Pairing(Option<String>),
    /// attn could not list the sessions. ↩ asks again.
    Unlisted(String),
}

/// A stroke under way: what its press decided.
#[derive(Clone, Copy, Debug)]
pub struct StrokeStart {
    pub still: u64,
    pub at: HostTime,
    pub boxed: bool,
    pub tool: InkTool,
}

/// A label placed on the ink layer, waiting for its text in the bar.
#[derive(Clone, Copy, Debug)]
struct LabelDraft {
    start: StrokeStart,
    at: (f32, f32),
    end: HostTime,
}

#[derive(Default)]
struct State {
    /// The displays the area picker covers while it is open.
    picking: Option<Vec<DisplayFrame>>,
    last_area: Option<Area>,
    capture: Option<Capture>,
    tile: Option<Tile>,
    /// The still the editor holds. The live surfaces stay hidden and the watch off.
    editing: Option<u64>,
    /// Open this still in the editor as soon as its frame lands.
    edit_when_landed: Option<u64>,
    keys: KeyOwner,
    /// Counts requests to hand the keyboard to the bar.
    bar_key_requests: u64,
    tool: InkTool,
    /// Where the tool strip stands, in points on the capture's display, while
    /// the ink layer takes the pointer.
    strip: Option<(f32, f32)>,
    label: Option<LabelDraft>,
    attn: AttnStatus,
    sessions: Vec<attn::Session>,
    /// The Tab palette's selected row, while it is open.
    palette: Option<usize>,
    input: Input,
    /// A corner handle is being dragged. The handles hold still until it ends, so
    /// the window that owns the drag never moves under the pointer.
    resizing: bool,
    send: SendStatus,
    /// The capture a running send is delivering. It takes no more stills: the
    /// send has its copies and closes the capture when it succeeds.
    sending: Option<u64>,
    /// The newest listing asked for. An older one finishing later is dropped.
    listing: u64,
    /// A failure with no capture to hold it, such as a refused still.
    error: Option<String>,
    next_capture: u64,
    /// Never reset, so an edit coming back for a closed capture's still matches nothing.
    next_still: u64,
    watch_generation: u64,
    /// Counts voice starts; see `CaptureVoice::Running`.
    voice_run: u64,
    /// Set by automation: the next voice plays this WAV instead of the microphone.
    voice_wav: Option<PathBuf>,
    /// The voice key with no capture open.
    dictation: Option<Dictation>,
    /// What the last dictation settled on, for Copy Last Dictation (D8). In memory only.
    last_dictation: Option<String>,
    /// Where the pill shows: the display with the pointer when it appeared.
    dictation_display: Option<DisplayFrame>,
    /// A settled dictation's text, pasted once the state lock is let go.
    paste: Option<(u64, String)>,
    watch: Option<Watch>,
    /// Loaded once at launch; changed only through `Lens::set_settings`.
    settings: Settings,
    /// Why the last change to the settings could not be saved. It still holds for this run.
    settings_error: Option<String>,
    /// Counts requests to open the Settings window.
    settings_requests: u64,
}

type WindowVisible = Box<dyn Fn() -> bool + Send>;

pub struct Lens {
    port: Arc<dyn PlatformPort>,
    state: Mutex<State>,
    /// Where `State.settings` is saved.
    prefs: Mutex<Prefs>,
    /// The capture of each still being taken. A send waits for its capture's
    /// stills so none is left out; another capture's stills are not its concern.
    /// Stills on their way, as (capture id, still id).
    pending_stills: (Mutex<Vec<(u64, u64)>>, Condvar),
    /// Counts changes; the condvar wakes whoever waits for the next one.
    generation: (Mutex<u64>, Condvar),
    wakers: Mutex<Vec<Box<dyn Fn() + Send>>>,
    editor: Mutex<Option<Sink<EditorCall>>>,
    /// Each window's name and how to ask whether it shows, for automation.
    windows: Mutex<Vec<(&'static str, WindowVisible)>>,
}

impl Lens {
    /// A Lens whose settings are kept in `prefs`.
    pub fn new(port: Arc<dyn PlatformPort>, prefs: Prefs) -> Arc<Self> {
        Arc::new(Self {
            port,
            state: Mutex::new(State {
                settings: Settings::load(&prefs),
                ..Default::default()
            }),
            prefs: Mutex::new(prefs),
            pending_stills: Default::default(),
            generation: Default::default(),
            wakers: Default::default(),
            editor: Default::default(),
            windows: Default::default(),
        })
    }

    pub fn port(&self) -> &Arc<dyn PlatformPort> {
        &self.port
    }

    /// Calls `wake` after every change.
    pub fn on_change(&self, wake: impl Fn() + Send + 'static) {
        self.wakers.lock().unwrap().push(Box::new(wake));
    }

    /// Where Lens reaches the editor window.
    pub fn set_editor(&self, editor: Sink<EditorCall>) {
        *self.editor.lock().unwrap() = Some(editor);
    }

    fn call_editor(&self, call: EditorCall) {
        let editor = self.editor.lock().unwrap().clone();
        if let Some(editor) = editor {
            editor(call);
        }
    }

    /// Reports `window` in the automation state.
    pub fn report_window(&self, window: &'static str, visible: impl Fn() -> bool + Send + 'static) {
        self.windows
            .lock()
            .unwrap()
            .push((window, Box::new(visible)));
    }

    /// Changes each time the state does; a window redraws when it differs from what it saw.
    pub fn generation(&self) -> u64 {
        *self.generation.0.lock().unwrap()
    }

    /// Blocks until the generation differs from `since` or `timeout` passes, and
    /// returns the generation. Read the state after this: a change that lands
    /// later moves the generation past the one returned.
    pub fn wait_change(&self, since: u64, timeout: Duration) -> u64 {
        let (generation, moved) = &self.generation;
        let (generation, _) = moved
            .wait_timeout_while(generation.lock().unwrap(), timeout, |now| *now == since)
            .unwrap();
        *generation
    }

    fn changed(&self) {
        *self.generation.0.lock().unwrap() += 1;
        self.generation.1.notify_all();
        for wake in self.wakers.lock().unwrap().iter() {
            wake();
        }
    }

    /// Saves `settings` and applies them from now on. A save that fails keeps them
    /// for this run, and the Settings window shows why.
    pub fn set_settings(&self, settings: Settings) {
        {
            let mut state = self.state.lock().unwrap();
            if state.settings == settings {
                return;
            }
            let saved = settings.save(&state.settings, &mut self.prefs.lock().unwrap());
            state.settings_error = saved.err();
            state.settings = settings;
        }
        self.changed();
    }

    /// Whether Screen Recording is allowed, asked without showing the guide: the
    /// Settings window says when "Use words on screen" cannot read the screen.
    pub fn screen_recording_allowed(&self) -> bool {
        self.port.screen_recording_allowed()
    }

    /// Opens the Settings window, or brings it to the front.
    pub fn open_settings(&self) {
        self.state.lock().unwrap().settings_requests += 1;
        self.changed();
    }

    /// Starts listening for keys. Without Screen Recording or Accessibility, it
    /// shows the user how to allow them now rather than at the first capture key.
    /// Screen Recording comes first, since its grant needs a reopen; Accessibility
    /// then shows after it.
    pub fn listen(self: &Arc<Self>) {
        let lens = self.clone();
        let sink: Sink<KeyEvent> = Arc::new(move |event| lens.key(event));
        self.port.keys(sink);
        if self.port.screen_recording() {
            self.port.accessibility();
        }
    }

    pub fn key(self: &Arc<Self>, event: KeyEvent) {
        match event {
            KeyEvent::DrawDown => self.set_input(Input::All),
            KeyEvent::DrawUp => self.set_input(Input::None),
            KeyEvent::Pressed(Chord::Capture) => {
                if self.is_editing() {
                    self.leave_editor(false);
                } else if self.is_open() {
                    // The asked-for still begins a new view, so the next stroke takes its own.
                    if let Some(capture) = self.state.lock().unwrap().capture.as_mut() {
                        capture.view_still = None;
                    }
                    self.take_still(Cause::Hotkey);
                    self.give_keys_to_bar();
                } else {
                    self.open_picker();
                }
            }
            KeyEvent::Voice(press) => {
                if self.is_open() {
                    self.capture_voice(press);
                } else {
                    self.dictation(press);
                }
            }
            KeyEvent::Pressed(Chord::Keyboard) => self.give_keys_to_bar(),
            KeyEvent::Pressed(Chord::Send) => self.send_or_recover(),
            KeyEvent::Pill(key) => self.pill_key(key),
            KeyEvent::Tool(digit) => {
                if let Some(tool) = InkTool::from_digit(digit) {
                    self.pick_tool(tool);
                }
            }
        }
    }

    fn is_editing(&self) -> bool {
        self.state.lock().unwrap().editing.is_some()
    }

    /// Asks the editor to give its still back, committing what it holds.
    pub fn leave_editor(&self, send: bool) {
        self.call_editor(EditorCall::Leave { send });
    }

    fn still_pending(&self, capture: u64) -> bool {
        let pending = self.pending_stills.0.lock().unwrap();
        pending.iter().any(|&(of, _)| of == capture)
    }

    /// The bar's Edit: the newest still, which may be still on its way. With none
    /// taken yet, takes one to edit.
    pub fn edit_latest(self: &Arc<Self>) {
        let latest = {
            let mut state = self.state.lock().unwrap();
            let Some(capture) = state.capture.as_ref() else {
                return;
            };
            let latest = capture.stills.last().map(|s| s.id);
            let next = state.next_still;
            let newest_pending = self
                .pending_stills
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|&&(of, _)| of == capture.id)
                .map(|&(_, still)| still)
                .max()
                .filter(|&pending| latest.is_none_or(|landed| pending > landed));
            if newest_pending.is_some() {
                state.edit_when_landed = newest_pending;
                return;
            }
            if latest.is_none() {
                state.edit_when_landed = Some(next);
            }
            latest
        };
        match latest {
            Some(still) => self.edit_still(still),
            None => {
                self.take_still(Cause::Hotkey);
            }
        }
    }

    /// Hands a still to the editor. The capture waits behind it: its surfaces hide
    /// and the watch stops, since the editor window itself would count as the page
    /// changing. The ink left on screen goes, as the editor may change those marks.
    pub fn edit_still(self: &Arc<Self>, id: u64) {
        let document = {
            let mut guard = self.state.lock().unwrap();
            let state = &mut *guard;
            let Some(capture) = state.capture.as_mut() else {
                return;
            };
            if state.editing.is_some() || state.sending == Some(capture.id) {
                return;
            }
            let Some(still) = capture.stills.iter().find(|s| s.id == id) else {
                return;
            };
            let document = still.doc.clone();
            capture.view_still = None;
            for mark in &mut capture.ink {
                mark.stale = true;
            }
            state.editing = Some(id);
            self.input_now(state, Input::None);
            state.palette = None;
            state.watch = None;
            state.watch_generation += 1;
            document
        };
        self.changed();
        // The editor is a window of its own: it makes Lens the active app.
        self.port.hold_front();
        self.call_editor(EditorCall::Open {
            still: id,
            document: Box::new(document),
        });
    }

    /// The editor's latest version of a still.
    pub fn still_edited(&self, id: u64, document: Document) {
        let mut guard = self.state.lock().unwrap();
        let state = &mut *guard;
        let open = state.capture.iter_mut();
        let closed = state.tile.iter_mut().map(|t| &mut t.capture);
        // The capture can close behind the editor; the edit goes with it.
        if let Some(still) = open
            .chain(closed)
            .flat_map(|c| c.stills.iter_mut())
            .find(|s| s.id == id)
        {
            still.doc = document;
        }
    }

    /// The editor gave its still back: the capture shows again, with the keyboard
    /// on its message and the strip showing the edit.
    pub fn finish_editing(self: &Arc<Self>, id: u64) {
        let (area, edited) = {
            let mut state = self.state.lock().unwrap();
            if state.editing != Some(id) {
                return;
            }
            state.editing = None;
            state.keys = KeyOwner::Bar;
            state.bar_key_requests += 1;
            let Some(capture) = state.capture.as_ref() else {
                return;
            };
            let edited = capture.stills.iter().find(|s| s.id == id);
            (
                capture.area,
                edited.map(|s| (s.doc.clone(), s.thumb.clone())),
            )
        };
        self.rewatch(area);
        self.changed();
        if let Some((document, thumb)) = edited {
            let lens = self.clone();
            self.port.run(Box::new(move || {
                bar::draw_thumbnail(&thumb, &document.export());
                lens.changed();
            }));
        }
    }

    /// The tile leaves when its time is up. Returns how long until it next needs a look.
    pub fn advance_tile(&self) -> Option<Duration> {
        let now = self.port.now();
        let mut state = self.state.lock().unwrap();
        let leaves = state.tile.as_ref()?.leaves?;
        if now < leaves {
            return Some(leaves.since(now));
        }
        state.tile = None;
        drop(state);
        self.changed();
        None
    }

    /// The pointer entered or left the tile: a sent tile waits while it is there.
    pub fn hover_tile(&self, inside: bool) {
        let now = self.port.now();
        let mut state = self.state.lock().unwrap();
        if let Some(tile) = state.tile.as_mut().filter(|t| t.sent) {
            tile.leaves = (!inside).then(|| now.after(TILE_STAYS));
        }
        drop(state);
        self.changed();
    }

    pub fn discard_tile(&self) {
        self.state.lock().unwrap().tile = None;
        self.changed();
    }

    /// Opens the tile's capture again, as it was, on the live screen.
    pub fn reopen_tile(self: &Arc<Self>) {
        let area = {
            let mut guard = self.state.lock().unwrap();
            let state = &mut *guard;
            if state.capture.is_some() {
                return;
            }
            let Some(tile) = state.tile.take() else {
                return;
            };
            let area = tile.capture.area;
            state.capture = Some(tile.capture);
            state.last_area = Some(area);
            state.keys = KeyOwner::Bar;
            state.bar_key_requests += 1;
            state.error = None;
            if state.send != SendStatus::Sending {
                state.send = SendStatus::Idle;
            }
            area
        };
        self.rewatch(area);
        self.port.capture_keys(true);
        self.changed();
        self.load_sessions();
    }

    fn is_open(&self) -> bool {
        self.state.lock().unwrap().capture.is_some()
    }

    fn give_keys_to_bar(&self) {
        let mut state = self.state.lock().unwrap();
        if state.capture.is_some() && state.editing.is_none() {
            state.keys = KeyOwner::Bar;
            state.bar_key_requests += 1;
            drop(state);
            self.changed();
        }
    }

    /// Records who has the keyboard, as the bar's window reports it.
    pub fn set_key_owner(&self, keys: KeyOwner) {
        let mut state = self.state.lock().unwrap();
        if state.keys != keys {
            state.keys = keys;
            drop(state);
            self.changed();
        }
    }

    /// Gives the page back the keyboard a press on the bar took from it.
    pub fn release_keys(&self) {
        if self.port.release_keys() {
            self.set_key_owner(KeyOwner::Page);
        }
    }

    fn set_input(&self, input: Input) {
        let mut state = self.state.lock().unwrap();
        // The ink layer exists only during a capture.
        let input = if state.capture.is_some() && state.editing.is_none() {
            input
        } else {
            Input::None
        };
        if state.input != input {
            self.input_now(&mut state, input);
            drop(state);
            self.changed();
        }
    }

    /// Sets whether the ink layer takes the pointer. While it does, the tool strip
    /// stands where the pointer was when the draw key went down.
    fn input_now(&self, state: &mut State, input: Input) {
        state.input = input;
        state.strip = match (input, &state.capture) {
            (Input::All, Some(capture)) => {
                let (x, y) = self.port.pointer();
                let origin = capture.display.frame;
                Some(((x - origin[0]) as f32, (y - origin[1]) as f32))
            }
            _ => None,
        };
    }

    fn fail(&self, error: String) {
        self.state.lock().unwrap().error = Some(error);
        self.changed();
    }

    /// Opens the area picker on every display connected now. The capture key during a pick does nothing.
    fn open_picker(&self) {
        if !self.port.screen_recording() {
            return;
        }
        let displays = self.port.displays();
        {
            let mut state = self.state.lock().unwrap();
            if state.picking.is_some() {
                return;
            }
            state.picking = Some(displays);
            state.error = None;
            // The picker owns ↩ and Esc while it shows.
            self.sync_pill_keys(&state);
        }
        self.changed();
    }

    /// Closes the area picker without starting a capture.
    pub fn cancel_pick(&self) {
        {
            let mut state = self.state.lock().unwrap();
            state.picking = None;
            self.sync_pill_keys(&state);
        }
        self.changed();
        self.port.return_front();
    }

    /// Starts a capture from the picker: `rect` on `display`, or the whole display.
    /// The rect is in points from the display's top-left.
    pub fn pick(self: &Arc<Self>, display: u32, rect: Option<[f64; 4]>) -> Result<(), String> {
        let frame = self
            .port
            .displays()
            .into_iter()
            .find(|d| d.id == display)
            .ok_or_else(|| format!("Display {display} is not connected."))?;
        let area = rect.map_or(frame.whole(), |rect| Area {
            display,
            rect: select::clamp(rect, frame),
        });
        self.start_capture(area, frame);
        Ok(())
    }

    /// Starts a capture of the whole display under the pointer.
    pub fn pick_pointer_display(self: &Arc<Self>) -> Result<(), String> {
        let area = self.port.pointer_display()?;
        self.pick(area.display, None)
    }

    /// ↩ in the picker. Returns false when no last area is left to use: there is
    /// none yet, or its display is gone.
    pub fn pick_last(self: &Arc<Self>) -> bool {
        let Some(area) = self.state.lock().unwrap().last_area else {
            return false;
        };
        self.pick(area.display, Some(area.rect)).is_ok()
    }

    /// A corner handle's drag begins or ends.
    pub fn set_resizing(self: &Arc<Self>, resizing: bool) {
        let mut state = self.state.lock().unwrap();
        let resizing = resizing && state.capture.is_some();
        if state.resizing != resizing {
            state.resizing = resizing;
            let area = state.capture.as_ref().filter(|_| !resizing).map(|c| c.area);
            drop(state);
            if let Some(area) = area {
                self.rewatch(area);
            }
            self.changed();
        }
    }

    /// Starts a watch whose events count only until the next one starts: a frame
    /// or event in flight when a watch stops can still arrive.
    fn watch_area(self: &Arc<Self>, area: Area) -> Watch {
        let generation = {
            let mut state = self.state.lock().unwrap();
            state.watch_generation += 1;
            state.watch_generation
        };
        let lens = self.clone();
        let sink: Sink<WatchEvent> = Arc::new(move |event| lens.watched(generation, event));
        self.port.watch(area, sink)
    }

    /// Watches `area` from now on, in place of what was watched before.
    fn rewatch(self: &Arc<Self>, area: Area) {
        self.state.lock().unwrap().watch = None;
        let watch = self.watch_area(area);
        let mut state = self.state.lock().unwrap();
        if state.capture.is_some() {
            state.watch = Some(watch);
        }
    }

    /// Moves the open capture's area, which the next still will cover. The screen stays live.
    pub fn set_area(&self, rect: [f64; 4]) {
        let mut state = self.state.lock().unwrap();
        let Some(capture) = state.capture.as_mut() else {
            return;
        };
        capture.area.rect = select::clamp(rect, capture.display);
        // The next stroke needs a still of the new area.
        capture.view_still = None;
        state.last_area = state.capture.as_ref().map(|c| c.area);
        drop(state);
        self.changed();
    }

    /// The user dragged the bar: `at` is its top-left in points from the capture
    /// display's top-left, or None for its place beside the area. It stays there
    /// for the rest of the capture.
    pub fn move_bar(&self, at: Option<(f64, f64)>) {
        let mut state = self.state.lock().unwrap();
        let Some(capture) = state.capture.as_mut() else {
            return;
        };
        if capture.bar_at != at {
            capture.bar_at = at;
            drop(state);
            self.changed();
        }
    }

    fn start_capture(self: &Arc<Self>, area: Area, display: DisplayFrame) {
        // Automation can pick while a capture is open.
        let state = self.state.lock().unwrap();
        if state.capture.is_some() {
            self.close_capture(state, Closing::Esc);
        } else {
            drop(state);
        }
        let watch = self.watch_area(area);
        {
            let mut state = self.state.lock().unwrap();
            // One corner, so a new capture takes the place of the old tile.
            state.tile = None;
            state.next_capture += 1;
            let id = state.next_capture;
            let target = attn::default_session(&state.sessions)
                .map(|index| state.sessions[index].id.clone());
            // Talk that turns into showing stays one piece (D9).
            let handed = self.hand_over(&mut state, id);
            let (started, segments, voice) = match handed {
                Some(handed) => (handed.started, handed.segments, handed.voice),
                None => (self.port.now(), vec![], CaptureVoice::Off),
            };
            state.capture = Some(Capture {
                id,
                area,
                display,
                started,
                stills: vec![],
                ink: vec![],
                view_still: None,
                unseen_change: None,
                send_still_error: None,
                segments,
                target,
                message: String::new(),
                voice,
                bar_at: None,
            });
            state.picking = None;
            state.last_area = Some(area);
            state.watch = Some(watch);
            state.error = None;
            state.keys = KeyOwner::Bar;
            state.bar_key_requests += 1;
            state.label = None;
            state.palette = None;
            // A send still running reports its own result, and holds off this capture's.
            if state.send != SendStatus::Sending {
                state.send = SendStatus::Idle;
            }
        }
        self.port.capture_keys(true);
        self.changed();
        self.load_sessions();
    }

    /// Asks attn for the sessions off the UI thread, and picks a target if the
    /// capture has none or its target has gone.
    pub fn load_sessions(self: &Arc<Self>) {
        let Some(token) = self.port.token() else {
            self.state.lock().unwrap().attn =
                AttnStatus::Unpaired("Attn Lens is not paired with attn yet.".into());
            return self.changed();
        };
        let listing = {
            let mut state = self.state.lock().unwrap();
            state.attn = AttnStatus::Loading;
            state.listing += 1;
            state.listing
        };
        self.changed();
        let lens = self.clone();
        self.port.run(Box::new(move || {
            let listed = attn::locate().and_then(|bin| attn::sessions(&bin, &token));
            {
                let mut guard = lens.state.lock().unwrap();
                let state = &mut *guard;
                if state.listing != listing {
                    return;
                }
                match listed {
                    Ok(sessions) => {
                        if let Some(capture) = state.capture.as_mut() {
                            let known = capture
                                .target
                                .as_ref()
                                .is_some_and(|id| sessions.iter().any(|s| &s.id == id));
                            if !known {
                                capture.target = attn::default_session(&sessions)
                                    .map(|index| sessions[index].id.clone());
                            }
                        }
                        state.sessions = sessions;
                        state.attn = AttnStatus::Ready;
                    }
                    Err(attn::Failure::NotPaired(text)) => state.attn = AttnStatus::Unpaired(text),
                    Err(attn::Failure::Error(text)) => state.attn = AttnStatus::Unlisted(text),
                }
            }
            lens.changed();
        }));
    }

    /// Asks the user to allow Lens in attn, off the UI thread.
    fn pair(self: &Arc<Self>) {
        {
            let mut state = self.state.lock().unwrap();
            if matches!(state.attn, AttnStatus::Pairing(_)) {
                return;
            }
            state.attn = AttnStatus::Pairing(None);
        }
        self.changed();
        let lens = self.clone();
        self.port.run(Box::new(move || {
            let shown = lens.clone();
            let paired = attn::locate()
                .and_then(|bin| {
                    attn::pair(&bin, |request| {
                        shown.state.lock().unwrap().attn = AttnStatus::Pairing(Some(request.code));
                        shown.changed();
                    })
                })
                .map_err(failure_text)
                .and_then(|token| lens.port.store_token(&token));
            match paired {
                Ok(()) => lens.load_sessions(),
                Err(text) => {
                    lens.state.lock().unwrap().attn = AttnStatus::Unpaired(text);
                    lens.changed();
                }
            }
        }));
    }

    /// ↩ and ⌘⌥↩: sends, or does what must happen before a send can.
    pub fn send_or_recover(self: &Arc<Self>) {
        if self.is_editing() {
            return self.leave_editor(true);
        }
        let attn = self.state.lock().unwrap().attn.clone();
        match attn {
            AttnStatus::Unpaired(_) => self.pair(),
            AttnStatus::Unlisted(_) => self.load_sessions(),
            AttnStatus::Pairing(_) | AttnStatus::Loading => {}
            AttnStatus::Ready => {
                if let Err(error) = self.send(None, None) {
                    self.fail(error);
                }
            }
        }
    }

    /// Send from the editor. The user chose what to send, so a screen change from
    /// before the edit adds no fresh still they have not seen.
    pub fn send_edited(self: &Arc<Self>) {
        if let Some(capture) = self.state.lock().unwrap().capture.as_mut() {
            capture.unseen_change = None;
        }
        self.send_or_recover();
    }

    /// Sets the message the next send carries.
    pub fn set_message(&self, message: &str) {
        if let Some(capture) = self.state.lock().unwrap().capture.as_mut() {
            capture.message = message.to_string();
        }
    }

    /// Opens or closes the Tab palette. Opening after a failed listing asks again.
    pub fn toggle_palette(self: &Arc<Self>) {
        let reload = {
            let mut state = self.state.lock().unwrap();
            state.palette = if state.palette.is_some() {
                None
            } else {
                let chosen = state.capture.as_ref().and_then(|c| c.target.as_ref());
                Some(
                    chosen
                        .and_then(|id| state.sessions.iter().position(|s| &s.id == id))
                        .unwrap_or(0),
                )
            };
            state.palette.is_some() && matches!(state.attn, AttnStatus::Unlisted(_))
        };
        self.changed();
        if reload {
            self.load_sessions();
        }
    }

    /// Moves the palette's selection, wrapping at either end.
    pub fn move_palette(&self, by: isize) {
        let mut state = self.state.lock().unwrap();
        let rows = state.sessions.len() as isize;
        if let (Some(selected), true) = (state.palette, rows > 0) {
            state.palette = Some((selected as isize + by).rem_euclid(rows) as usize);
            drop(state);
            self.changed();
        }
    }

    /// Makes the palette's selected row, or row `index`, the target and closes it.
    pub fn choose_session(&self, index: Option<usize>) {
        let mut guard = self.state.lock().unwrap();
        let state = &mut *guard;
        let chosen = index.or(state.palette).and_then(|i| state.sessions.get(i));
        if let (Some(session), Some(capture)) = (chosen, state.capture.as_mut()) {
            capture.target = Some(session.id.clone());
        }
        state.palette = None;
        drop(guard);
        self.changed();
    }

    /// Picks the tool strokes make from now on.
    pub fn pick_tool(&self, tool: InkTool) {
        let mut state = self.state.lock().unwrap();
        if state.tool == tool {
            return;
        }
        state.tool = tool;
        drop(state);
        self.changed();
    }

    /// A press on the ink layer while the draw key is held. Takes the still on the
    /// first press of a view, before any ink (D7 on the plan). None if nothing may
    /// be drawn now.
    pub fn begin_stroke(self: &Arc<Self>) -> Option<StrokeStart> {
        let (open, tool) = {
            let state = self.state.lock().unwrap();
            let capture = state.capture.as_ref()?;
            if state.input != Input::All || state.sending == Some(capture.id) {
                return None;
            }
            (capture.view_still, state.tool)
        };
        let still = match open {
            Some(still) => still,
            None => {
                let still = self.take_still(Cause::Draw)?;
                let mut state = self.state.lock().unwrap();
                if let Some(capture) = state.capture.as_mut() {
                    capture.view_still = Some(still);
                }
                // A change from before this still is on it, so only a later one counts.
                if let Some(watch) = &state.watch {
                    watch.rebase();
                }
                still
            }
        };
        // Read after the still's own time, so a frame is never later than its marks.
        let at = self.port.now();
        Some(StrokeStart {
            still,
            at,
            boxed: self.port.shift_held(),
            tool,
        })
    }

    /// Ends a stroke with its points on the display, turning it into a mark.
    pub fn finish_stroke(&self, start: StrokeStart, points: Vec<(f32, f32)>) {
        let end = self.port.now();
        let mut guard = self.state.lock().unwrap();
        let state = &mut *guard;
        let Some(capture) = state.capture.as_mut() else {
            return;
        };
        if start.tool == InkTool::Label {
            if let Some(at) = points.first().copied() {
                state.label = Some(LabelDraft { start, at, end });
                state.keys = KeyOwner::Bar;
                state.bar_key_requests += 1;
            }
        } else {
            let counter = 1 + capture
                .ink
                .iter()
                .filter(|m| m.still == start.still && m.mark.tool == crate::document::Tool::Counter)
                .count();
            if let Some(mark) = ink::mark_from_stroke(&start, &points, counter) {
                let stale = capture.view_still != Some(start.still);
                capture.ink.push(InkMark {
                    still: start.still,
                    mark,
                    times: (start.at, end),
                    committed: false,
                    stale,
                    fade_from: None,
                });
                join_finished_ink(capture);
            }
        }
        drop(guard);
        self.changed();
    }

    /// Finishes the label being placed. Empty text drops it.
    pub fn finish_label(&self, text: &str) {
        let mut guard = self.state.lock().unwrap();
        let state = &mut *guard;
        let draft = state.label.take();
        if let (Some(draft), Some(capture)) = (draft, state.capture.as_mut())
            && !text.trim().is_empty()
        {
            let stale = capture.view_still != Some(draft.start.still);
            capture.ink.push(InkMark {
                still: draft.start.still,
                mark: ink::label_mark(draft.at, text.trim()),
                times: (draft.start.at, draft.end),
                committed: false,
                stale,
                fade_from: None,
            });
            join_finished_ink(capture);
        }
        drop(guard);
        self.changed();
    }

    /// Drops a still and the ink drawn on it: a secret can repeat across stills.
    pub fn drop_still(&self, id: u64) {
        let mut state = self.state.lock().unwrap();
        if state.editing == Some(id) {
            return;
        }
        if let Some(capture) = state.capture.as_mut() {
            capture.stills.retain(|s| s.id != id);
            capture.ink.retain(|m| m.still != id);
            if capture.view_still == Some(id) {
                capture.view_still = None;
            }
        }
        drop(state);
        self.changed();
    }

    /// The marks on the ink layer, in points on the capture's display, each as
    /// transparent as its fade has made it.
    pub fn ink_marks(&self) -> Vec<crate::document::Mark> {
        let now = self.port.now();
        let state = self.state.lock().unwrap();
        let ink = state.capture.as_ref().map(|c| c.ink.as_slice());
        ink.unwrap_or_default()
            .iter()
            .map(|m| {
                let mut mark = m.mark.clone();
                if let Some(from) = m.fade_from {
                    let left = 1. - now.since(from).as_secs_f32() / FADE.as_secs_f32();
                    mark.color[3] = (f32::from(mark.color[3]) * left.max(0.)) as u8;
                }
                mark
            })
            .collect()
    }

    /// Starts the fades that are due and removes the ink that has finished. Each
    /// mark begins only once its still exists. Returns true while any is fading.
    pub fn advance_fades(&self) -> bool {
        let now = self.port.now();
        let mut state = self.state.lock().unwrap();
        let Some(capture) = state.capture.as_mut() else {
            return false;
        };
        let due = capture
            .ink
            .iter_mut()
            .filter(|m| m.stale && m.committed && m.fade_from.is_none());
        for mark in due {
            mark.fade_from = Some(now);
        }
        let before = capture.ink.len();
        capture
            .ink
            .retain(|m| m.fade_from.is_none_or(|from| now.since(from) < FADE));
        let removed = capture.ink.len() != before;
        let fading = capture.ink.iter().any(|m| m.fade_from.is_some());
        drop(state);
        if fading || removed {
            self.changed();
        }
        fading
    }

    /// Closes the open capture and lets go of everything it held.
    pub fn end_capture(self: &Arc<Self>) {
        self.close_capture(self.state.lock().unwrap(), Closing::Esc);
    }

    /// Esc and a successful send both close the capture here, voice included.
    fn close_capture(self: &Arc<Self>, mut state: MutexGuard<'_, State>, closing: Closing) {
        let mut capture = state.capture.take();
        let closing_id = capture.as_ref().map(|c| c.id);
        let voice = capture.as_mut().map(|c| std::mem::take(&mut c.voice));
        let was_editing = state.editing.take().is_some();
        state.edit_when_landed = None;
        if let Some(mut capture) = capture {
            capture.view_still = None;
            capture.ink.retain(|m| !m.committed);
            capture.unseen_change = None;
            let in_flight = self.still_pending(capture.id);
            state.tile = match closing {
                Closing::Sent => Some(Tile {
                    capture,
                    sent: true,
                    leaves: Some(self.port.now().after(TILE_STAYS)),
                }),
                // Esc on a capture with nothing in it is a plain cancel.
                Closing::Esc if capture.stills.is_empty() && !in_flight => None,
                Closing::Esc => Some(Tile {
                    capture,
                    sent: false,
                    leaves: None,
                }),
            };
        }
        self.input_now(&mut state, Input::None);
        state.resizing = false;
        state.watch = None;
        state.label = None;
        state.palette = None;
        // A tile keeps the voice as finishing, so its last sentence lands in the
        // capture and a reopened capture's Send waits for it.
        let run = match &voice {
            Some(
                CaptureVoice::Running(RunningVoice { run, .. })
                | CaptureVoice::Finishing { run, .. },
            ) => Some(*run),
            _ => None,
        };
        if let Some(run) = run
            && let Some(tile) = state
                .tile
                .as_mut()
                .filter(|t| Some(t.capture.id) == closing_id)
        {
            tile.capture.voice = CaptureVoice::Finishing {
                run,
                send_after: false,
            };
        }
        let handle = match voice {
            Some(CaptureVoice::Running(voice)) => Some(voice.handle),
            Some(CaptureVoice::Finishing {
                send_after: true, ..
            }) => {
                state.sending = None;
                state.send = SendStatus::Failed(CLOSED_BEFORE_SENT.into());
                None
            }
            _ => None,
        };
        drop(state);
        self.port.capture_keys(false);
        self.changed();
        // The capture has no window left to take the keyboard, so it goes back to the app
        // the user was in.
        self.port.return_front();
        if was_editing {
            self.leave_editor(false);
        }
        // A stopping voice keeps its helper to itself, and the next capture gets
        // its own: a helper records one voice at a time.
        self.port.end_voice();
        let (Some(handle), Some(id), Some(run)) = (handle, closing_id, run) else {
            return;
        };
        let lens = self.clone();
        self.port.run(Box::new(move || {
            lens.voice_stopped(id, run, handle.finish())
        }));
    }

    /// The screen moved under the capture: ends the view, stales its ink, and
    /// takes the still D7 asks for.
    fn watched(self: &Arc<Self>, generation: u64, event: WatchEvent) {
        let cause = match event {
            WatchEvent::Click => Some(Cause::Click),
            WatchEvent::WindowSwitch => Some(Cause::WindowSwitch),
            WatchEvent::Scroll => None,
            WatchEvent::Change(fraction) if fraction >= CHANGE_FRACTION => None,
            WatchEvent::Change(_) => return,
        };
        let cause = {
            let mut guard = self.state.lock().unwrap();
            let state = &mut *guard;
            let Some(capture) = state
                .capture
                .as_mut()
                .filter(|_| state.watch_generation == generation)
            else {
                return;
            };
            // A running send has its stills; the page may still be clicked.
            let cause = cause.filter(|_| state.sending != Some(capture.id));
            let news = cause.is_some()
                || capture.view_still.is_some()
                || capture.unseen_change.is_none()
                || capture.ink.iter().any(|m| !m.stale);
            capture.unseen_change = Some(self.port.now());
            if !news {
                return;
            }
            capture.view_still = None;
            for mark in &mut capture.ink {
                mark.stale = true;
            }
            cause
        };
        if let Some(cause) = cause {
            self.take_still(cause);
        }
        self.changed();
    }

    /// Takes a still of the open capture's area off the UI thread. Its time is now,
    /// whenever the frame arrives, so stills stay in the order they were asked for.
    /// Returns the id the still will have.
    pub fn take_still(self: &Arc<Self>, cause: Cause) -> Option<u64> {
        let at = self.port.now();
        let (id, area, still_id) = {
            let mut state = self.state.lock().unwrap();
            let next_still = state.next_still;
            let capture = state.capture.as_mut()?;
            let (id, area, still_id) = (capture.id, capture.area, next_still);
            if cause != Cause::Send && state.sending == Some(id) {
                drop(state);
                self.fail("Attn Lens is sending this capture, so no still was taken.".into());
                return None;
            }
            state.next_still += 1;
            // Counted before the state is released, so a send never sees the
            // still neither kept nor on the way.
            self.pending_stills.0.lock().unwrap().push((id, still_id));
            (id, area, still_id)
        };
        let lens = self.clone();
        self.port.run(Box::new(move || {
            let mut open_landed = false;
            let frame = lens.port.still(area).map(|frame| {
                let thumb = bar::thumbnail(&frame);
                (frame, thumb)
            });
            {
                let mut guard = lens.state.lock().unwrap();
                let state = &mut *guard;
                match frame {
                    Ok((frame, thumb)) => {
                        // A capture closed while its frame was on the way drops the
                        // frame, unless its tile can still reopen it.
                        let open = state.capture.iter_mut();
                        let closed = state.tile.iter_mut().map(|t| &mut t.capture);
                        if let Some(capture) = open.chain(closed).find(|c| c.id == id) {
                            let still = Still {
                                id: still_id,
                                at,
                                cause,
                                area,
                                thumb,
                                doc: Document::new(frame),
                            };
                            let index = capture.stills.partition_point(|s| s.at <= at);
                            capture.stills.insert(index, still);
                            if capture.unseen_change.is_some_and(|change| change <= at) {
                                capture.unseen_change = None;
                            }
                            join_finished_ink(capture);
                        }
                        if state.edit_when_landed == Some(still_id) {
                            state.edit_when_landed = None;
                            open_landed = true;
                        }
                    }
                    Err(error) => {
                        if state.edit_when_landed == Some(still_id) {
                            state.edit_when_landed = None;
                        }
                        // Ink with no frame under it cannot be sent; the next press retries.
                        if let Some(capture) = state.capture.as_mut().filter(|c| c.id == id) {
                            if cause == Cause::Send {
                                capture.send_still_error = Some(error.clone());
                            }
                            capture.ink.retain(|m| m.still != still_id);
                            if capture.view_still == Some(still_id) {
                                capture.view_still = None;
                            }
                        }
                        state.error = Some(error);
                    }
                }
            }
            {
                let mut pending = lens.pending_stills.0.lock().unwrap();
                let index = pending.iter().position(|&p| p == (id, still_id)).unwrap();
                pending.swap_remove(index);
            }
            lens.pending_stills.1.notify_all();
            lens.changed();
            if open_landed {
                lens.edit_still(still_id);
            }
        }));
        Some(still_id)
    }

    /// Plays `wav` instead of the microphone from the next voice; `None` goes
    /// back to the microphone. For driven checks, through automation.
    pub fn set_voice_wav(&self, wav: Option<PathBuf>) {
        self.state.lock().unwrap().voice_wav = wav;
    }

    /// The voice key in a capture: a tap toggles voice, a hold talks until release.
    fn capture_voice(self: &Arc<Self>, press: Press) {
        let running = self
            .state
            .lock()
            .unwrap()
            .capture
            .as_ref()
            .is_some_and(|capture| !matches!(capture.voice, CaptureVoice::Off));
        match press {
            Press::Tap => self.toggle_voice(),
            Press::HoldStart if !running => self.toggle_voice(),
            Press::HoldEnd if running => self.toggle_voice(),
            Press::HoldStart | Press::HoldEnd => {}
        }
    }

    fn toggle_voice(self: &Arc<Self>) {
        let mut state = self.state.lock().unwrap();
        let Some(capture) = state.capture.as_mut() else {
            return;
        };
        let id = capture.id;
        match std::mem::take(&mut capture.voice) {
            CaptureVoice::Running(RunningVoice { run, handle, .. }) => {
                capture.voice = CaptureVoice::Finishing {
                    run,
                    send_after: false,
                };
                drop(state);
                self.changed();
                let lens = self.clone();
                self.port.run(Box::new(move || {
                    lens.voice_stopped(id, run, handle.finish())
                }));
                return;
            }
            // A voice still finishing holds the speech helper's one recording.
            finishing @ CaptureVoice::Finishing { .. } => {
                capture.voice = finishing;
                return;
            }
            CaptureVoice::Off => {}
        }
        state.voice_run += 1;
        let run = state.voice_run;
        let source = state
            .voice_wav
            .clone()
            .map_or(VoiceSource::Microphone, VoiceSource::Wav);
        let owner = Arc::new(Mutex::new(VoiceOwner::Capture(id)));
        // `voice` returns at once and reports through the sink from elsewhere, so
        // the first report finds the handle stored.
        match self.port.voice(source, self.voice_sink(run, owner.clone())) {
            Ok(handle) => {
                if let Some(capture) = state.capture.as_mut() {
                    capture.voice = CaptureVoice::Running(RunningVoice {
                        run,
                        handle,
                        status: None,
                        owner,
                    });
                }
            }
            Err(error) => state.error = Some(error),
        }
        drop(state);
        self.changed();
    }

    /// A report from voice `run` of capture `id`. Once the capture closes, nothing counts.
    fn capture_voice_event(
        &self,
        mut guard: MutexGuard<'_, State>,
        id: u64,
        run: u64,
        event: VoiceEvent,
    ) {
        let state = &mut *guard;
        let Some(capture) = capture_by_id(&mut state.capture, &mut state.tile, id) else {
            return;
        };
        let current = matches!(&capture.voice, CaptureVoice::Running(voice) if voice.run == run);
        match event {
            // A segment counts even after voice stops: stopping waits for it.
            VoiceEvent::Segment(segment) => capture.segments.push(segment),
            VoiceEvent::Status(new) => {
                if current && let CaptureVoice::Running(voice) = &mut capture.voice {
                    voice.status = Some(new);
                }
            }
            VoiceEvent::Failed(error) => {
                if current {
                    capture.voice = CaptureVoice::Off;
                }
                state.error = Some(error);
            }
        }
        drop(guard);
        self.changed();
    }

    /// The finish the voice key asked for returned. It starts the send that waited for it, or
    /// fails it, as the send would lose the last sentence.
    fn voice_stopped(self: &Arc<Self>, id: u64, run: u64, finished: Result<(), String>) {
        let mut guard = self.state.lock().unwrap();
        let state = &mut *guard;
        let Some(capture) = capture_by_id(&mut state.capture, &mut state.tile, id) else {
            return;
        };
        // A reopened capture keeps its id, and may be on a newer voice by now.
        let CaptureVoice::Finishing {
            run: finishing,
            send_after,
        } = capture.voice
        else {
            return;
        };
        if finishing != run {
            return;
        }
        capture.voice = CaptureVoice::Off;
        match finished {
            Ok(()) => {}
            Err(error) if send_after => {
                state.sending = None;
                state.send = SendStatus::Failed(error);
            }
            Err(error) => state.error = Some(error),
        }
        let send = send_after && matches!(state.send, SendStatus::Sending);
        drop(guard);
        self.changed();
        if send {
            let lens = self.clone();
            self.port.run(Box::new(move || lens.finish_send(id, None)));
        }
    }

    /// Sends the open capture: every still as a PNG, with the message. `target`
    /// and `message` replace what the capture holds. The result lands in `SendStatus`.
    pub fn send(
        self: &Arc<Self>,
        target: Option<String>,
        message: Option<String>,
    ) -> Result<(), String> {
        let (id, needs_still, handle, after_voice) = {
            let mut state = self.state.lock().unwrap();
            if state.send == SendStatus::Sending {
                return Err("A send is already running.".into());
            }
            if state.editing.is_some() {
                return Err("The editor has a still of this capture open. Go Back first.".into());
            }
            let capture = state
                .capture
                .as_mut()
                .ok_or("No capture is open. Start one with right ⌘.")?;
            if let Some(target) = target {
                capture.target = Some(target);
            }
            if let Some(message) = message {
                capture.message = message;
            }
            if capture.target.is_none() {
                return Err("The capture has no target session.".into());
            }
            // None kept and none on the way, as when × dropped every one.
            let id = capture.id;
            let in_flight = self.still_pending(id);
            let needs_still =
                (capture.stills.is_empty() && !in_flight) || capture.unseen_change.is_some();
            let (voice, handle, after_voice) = match std::mem::take(&mut capture.voice) {
                CaptureVoice::Off => (CaptureVoice::Off, None, false),
                CaptureVoice::Running(RunningVoice { run, handle, .. }) => (
                    CaptureVoice::Finishing {
                        run,
                        send_after: false,
                    },
                    Some(handle),
                    false,
                ),
                CaptureVoice::Finishing { run, .. } => (
                    CaptureVoice::Finishing {
                        run,
                        send_after: true,
                    },
                    None,
                    true,
                ),
            };
            capture.voice = voice;
            state.send = SendStatus::Sending;
            state.sending = Some(id);
            (id, needs_still, handle, after_voice)
        };
        self.changed();
        if needs_still {
            self.take_still(Cause::Send);
        }
        if !after_voice {
            let lens = self.clone();
            self.port
                .run(Box::new(move || lens.finish_send(id, handle)));
        }
        Ok(())
    }

    /// Finishes the send of capture `id`. The user may have closed it and opened
    /// another by now, so only that capture is read or closed.
    fn finish_send(self: &Arc<Self>, id: u64, voice: Option<Voice>) {
        let voice = voice.map_or(Ok(()), |voice| {
            let finished = voice.finish();
            let mut guard = self.state.lock().unwrap();
            let state = &mut *guard;
            if let Some(capture) = capture_by_id(&mut state.capture, &mut state.tile, id)
                .filter(|c| matches!(c.voice, CaptureVoice::Finishing { .. }))
            {
                capture.voice = CaptureVoice::Off;
            }
            finished
        });
        let mut pending = self.pending_stills.0.lock().unwrap();
        while pending.iter().any(|&(capture, _)| capture == id) {
            pending = self.pending_stills.1.wait(pending).unwrap();
        }
        drop(pending);
        let still_error = self
            .state
            .lock()
            .unwrap()
            .capture
            .as_mut()
            .filter(|c| c.id == id)
            .and_then(|c| c.send_still_error.take());
        let delivered = voice
            .and_then(|()| still_error.map_or(Ok(()), Err))
            .map_err(attn::Failure::Error)
            .and_then(|()| self.deliver(id));
        let mut state = self.state.lock().unwrap();
        state.sending = None;
        let sent = delivered.is_ok();
        state.send = match delivered {
            Ok(target) => SendStatus::Sent(target),
            Err(attn::Failure::NotPaired(text)) => {
                self.port.forget_token();
                state.attn = AttnStatus::Unpaired(text.clone());
                SendStatus::Failed(text)
            }
            Err(attn::Failure::Error(text)) => SendStatus::Failed(text),
        };
        // The user may have closed it, or opened another, while it was sent.
        if sent && state.capture.as_ref().is_some_and(|c| c.id == id) {
            return self.close_capture(state, Closing::Sent);
        }
        // Esc closed it during the send: its tile must not say it was never sent.
        if sent {
            let leaves = self.port.now().after(TILE_STAYS);
            if let Some(tile) = state.tile.as_mut().filter(|t| t.capture.id == id) {
                tile.sent = true;
                tile.leaves = Some(leaves);
            }
        }
        drop(state);
        self.changed();
    }

    fn deliver(&self, id: u64) -> Result<String, attn::Failure> {
        let plain = attn::Failure::Error;
        let (source, target) = {
            let state = self.state.lock().unwrap();
            let capture = state
                .capture
                .as_ref()
                .filter(|c| c.id == id)
                .ok_or_else(|| plain(CLOSED_BEFORE_SENT.into()))?;
            let target = capture
                .target
                .clone()
                .ok_or_else(|| plain("The capture has no target session.".into()))?;
            let frames = capture
                .stills
                .iter()
                .map(|s| deliver::Frame {
                    at: s.at,
                    cause: s.cause,
                    doc: s.doc.clone(),
                })
                .collect();
            let source = deliver::Source {
                started: capture.started,
                frames,
                segments: capture.segments.clone(),
                typed: capture.message.clone(),
            };
            (source, target)
        };
        let token = self
            .port
            .token()
            .ok_or_else(|| attn::Failure::NotPaired("Attn Lens is not paired with attn.".into()))?;
        let dir = tempfile::Builder::new()
            .prefix("attn-lens-send-")
            .tempdir()
            .map_err(|e| plain(e.to_string()))?;
        let delivery = deliver::build(&source, dir.path()).map_err(plain)?;
        let bin = attn::locate()?;
        attn::send(&bin, &token, &target, &delivery.files, &delivery.message)?;
        Ok(target)
    }

    /// Lists the sessions the pairing can send to, for automation.
    pub fn sessions(&self) -> Result<Vec<attn::Session>, String> {
        let token = self
            .port
            .token()
            .ok_or("Attn Lens is not paired with attn.")?;
        let bin = attn::locate().map_err(failure_text)?;
        attn::sessions(&bin, &token).map_err(failure_text)
    }

    /// Runs `view` over what a window shows.
    pub fn read<R>(&self, view: impl FnOnce(&Summary) -> R) -> R {
        view(&self.summary())
    }

    fn summary(&self) -> Summary {
        let state = self.state.lock().unwrap();
        Summary {
            input: state.input,
            capture: state.capture.as_ref().map(|c| CaptureSummary {
                id: c.id,
                area: c.area,
                display: c.display,
                started: c.started,
                stills: c
                    .stills
                    .iter()
                    .map(|s| StillSummary {
                        id: s.id,
                        at: s.at,
                        cause: s.cause,
                        marks: s.doc.marks.iter().map(ink::kind).collect(),
                        times: s
                            .doc
                            .marks
                            .iter()
                            .filter_map(|m| m.drawn)
                            .map(|(from, to)| (HostTime(from), HostTime(to)))
                            .collect(),
                        thumb: s.thumb.clone(),
                        width: s.doc.base.width(),
                        height: s.doc.base.height(),
                    })
                    .collect(),
                ink: c
                    .ink
                    .iter()
                    .map(|m| InkSummary {
                        still: m.still,
                        fading: m.fade_from.is_some(),
                    })
                    .collect(),
                segments: c.segments.clone(),
                target: c.target.clone(),
                message: c.message.clone(),
                bar_at: c.bar_at,
            }),
            picking: state.picking.clone(),
            last_area: state.last_area,
            keys: state.keys,
            bar_key_requests: state.bar_key_requests,
            tool: state.tool,
            strip: state.strip,
            placing_label: state.label.is_some(),
            attn: state.attn.clone(),
            sessions: state.sessions.clone(),
            palette: state.palette,
            voice: state
                .capture
                .as_ref()
                .map_or(VoiceState::Off, |c| match &c.voice {
                    CaptureVoice::Off => VoiceState::Off,
                    CaptureVoice::Running(RunningVoice { status: None, .. }) => {
                        VoiceState::Starting
                    }
                    CaptureVoice::Running(RunningVoice {
                        status: Some(status),
                        ..
                    }) => VoiceState::Running(*status),
                    CaptureVoice::Finishing { .. } => VoiceState::Finishing,
                }),
            resizing: state.resizing,
            editing: state.editing,
            tile: state.tile.as_ref().map(|t| TileSummary {
                sent: t.sent,
                leaving: t.leaves.is_some(),
                thumb: t.capture.stills.last().map(|s| s.thumb.clone()),
                stills: t.capture.stills.len(),
                display: t.capture.display,
            }),
            send: state.send.clone(),
            error: state.error.clone(),
            dictation: state.dictation.as_ref().map(Dictation::summary),
            dictation_display: state.dictation_display,
            settings: state.settings.clone(),
            settings_error: state.settings_error.clone(),
            settings_requests: state.settings_requests,
        }
    }

    /// What automation reads back.
    pub fn state_json(&self) -> Value {
        let summary = self.summary();
        let send = match &summary.send {
            SendStatus::Idle => json!({"state": "idle"}),
            SendStatus::Sending => json!({"state": "sending"}),
            SendStatus::Sent(target) => json!({"state": "sent", "target": target}),
            SendStatus::Failed(error) => json!({"state": "failed", "error": error}),
        };
        let windows: serde_json::Map<String, Value> = self
            .windows
            .lock()
            .unwrap()
            .iter()
            .map(|(name, visible)| (name.to_string(), json!(visible())))
            .collect();
        json!({
            "windows": windows,
            "picking": summary.picking,
            "last_area": summary.last_area,
            "keys": format!("{:?}", summary.keys),
            "tool": format!("{:?}", summary.tool),
            "attn": format!("{:?}", summary.attn),
            "palette": summary.palette,
            "input": format!("{:?}", summary.input),
            "editing": summary.editing,
            "tile": summary.tile.as_ref().map(|t| json!({
                "sent": t.sent,
                "leaving": t.leaving,
                "stills": t.stills,
            })),
            "voice": match summary.voice {
                VoiceState::Off => json!({"state": "off"}),
                VoiceState::Starting => json!({"state": "starting"}),
                VoiceState::Running(status) => json!(status),
                VoiceState::Finishing => json!({"state": "finishing"}),
            },
            "send": send,
            "error": summary.error,
            "dictation": summary.dictation.as_ref().map(dictation_json),
            "last_dictation": self.state.lock().unwrap().last_dictation,
            "settings": summary.settings,
            "settings_error": summary.settings_error,
            "capture": summary.capture.map(|c| json!({
                "id": c.id,
                "area": c.area,
                "started": c.started,
                "stills": c.stills.iter().map(|s| json!({
                    "id": s.id,
                    "at": s.at,
                    "cause": s.cause,
                    "marks": s.marks.len(),
                    "kinds": s.marks,
                    "times": s.times,
                    "width": s.width,
                    "height": s.height,
                })).collect::<Vec<_>>(),
                "ink": c.ink.iter().map(|m| json!({
                    "still": m.still,
                    "fading": m.fading,
                })).collect::<Vec<_>>(),
                "segments": c.segments,
                "target": c.target,
                "message": c.message,
                "bar_at": c.bar_at,
            })),
        })
    }
}

fn dictation_json(dictation: &DictationSummary) -> Value {
    match dictation {
        DictationSummary::Recording { status, words } => {
            json!({"state": "recording", "voice": status, "words": words})
        }
        DictationSummary::Finishing { words } => json!({"state": "finishing", "words": words}),
        DictationSummary::Refining { words } => json!({"state": "refining", "words": words}),
        DictationSummary::Settled { text, how, pasting } => json!({
            "state": if *pasting { "pasting" } else { "settled" },
            "text": text,
            "how": format!("{how:?}"),
            "note": how.note(),
        }),
    }
}

fn failure_text(failure: attn::Failure) -> String {
    match failure {
        attn::Failure::NotPaired(text) | attn::Failure::Error(text) => text,
    }
}

/// The capture with this id, open or kept in the tile.
fn capture_by_id<'a>(
    open: &'a mut Option<Capture>,
    tile: &'a mut Option<Tile>,
    id: u64,
) -> Option<&'a mut Capture> {
    let closed = tile.iter_mut().map(|t| &mut t.capture);
    open.iter_mut().chain(closed).find(|c| c.id == id)
}

/// `ink`'s mark as it is on `still`'s document.
fn on_still(ink: &InkMark, still: &Still) -> crate::document::Mark {
    let scale = still.doc.base.width() as f32 / still.area.rect[2] as f32;
    let mut mark = ink::on_still(&ink.mark, still.area, scale);
    mark.drawn = Some((ink.times.0.0, ink.times.1.0));
    mark
}

/// Puts finished ink on the stills whose frames have arrived, so a stroke and its
/// frame may finish in either order and the frame always exists first.
fn join_finished_ink(capture: &mut Capture) {
    for ink in capture.ink.iter_mut().filter(|m| !m.committed) {
        if let Some(still) = capture.stills.iter_mut().find(|s| s.id == ink.still) {
            still.doc.commit(on_still(ink, still));
            ink.committed = true;
        }
    }
}

pub struct StillSummary {
    pub id: u64,
    pub at: session::HostTime,
    pub cause: Cause,
    /// What each mark on the still is, as the ink layer names it.
    pub marks: Vec<&'static str>,
    pub times: Vec<(HostTime, HostTime)>,
    pub thumb: rune::Pixels,
    pub width: u32,
    pub height: u32,
}

pub struct InkSummary {
    pub still: u64,
    pub fading: bool,
}

pub struct CaptureSummary {
    pub id: u64,
    pub area: session::Area,
    pub display: DisplayFrame,
    pub started: session::HostTime,
    pub stills: Vec<StillSummary>,
    pub ink: Vec<InkSummary>,
    pub segments: Vec<Segment>,
    pub target: Option<String>,
    pub message: String,
    pub bar_at: Option<(f64, f64)>,
}

pub struct TileSummary {
    pub sent: bool,
    pub leaving: bool,
    pub thumb: Option<rune::Pixels>,
    pub stills: usize,
    pub display: DisplayFrame,
}

pub struct Summary {
    pub picking: Option<Vec<DisplayFrame>>,
    pub last_area: Option<Area>,
    pub keys: KeyOwner,
    pub bar_key_requests: u64,
    pub tool: InkTool,
    pub strip: Option<(f32, f32)>,
    pub placing_label: bool,
    pub attn: AttnStatus,
    pub sessions: Vec<attn::Session>,
    pub palette: Option<usize>,
    pub input: Input,
    pub resizing: bool,
    pub editing: Option<u64>,
    pub tile: Option<TileSummary>,
    pub capture: Option<CaptureSummary>,
    pub voice: VoiceState,
    pub send: SendStatus,
    pub error: Option<String>,
    pub dictation: Option<DictationSummary>,
    /// The display the pill shows on: the pointer's when the dictation started.
    pub dictation_display: Option<DisplayFrame>,
    pub settings: Settings,
    pub settings_error: Option<String>,
    pub settings_requests: u64,
}
