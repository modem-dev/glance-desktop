//! Glance's editor on Rune. One image at a time, edited through typed actions: the
//! toolbar, menus, keys and gestures all reach [`Editor::dispatch`].
//!
//! The image shown is a raster made off the UI thread, with marks after
//! [`Preview::marks`] drawn live on top of it, so a pointer move never touches pixels.
pub mod actions;
mod canvas;
mod color_picker;
mod dispatch;
mod export;
mod gesture;
mod icons;
mod keys;
mod live;
pub mod menus;
mod panels;
mod preview;
pub mod scenes;
mod stage;
mod view;

use crate::{
    document::{Document, Mark, Tool},
    lens::Lens,
    navigation::Layout,
    platform_port::PlatformPort,
    style::Style,
};
use color_picker::ColorPicker;
use gesture::Gesture;
use image::RgbaImage;
use preview::Preview;
use rune::{AppHandle, TextEdit};
use std::sync::{Arc, Mutex};
pub use view::view;

/// Everything a Mark can be drawn with, remembered per tool for the session.
#[derive(Clone, Copy)]
pub(crate) struct ToolSettings {
    pub color: [u8; 4],
    pub width: f32,
    pub style: Style,
    pub magnification: f32,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            color: [255, 56, 100, 255],
            width: 5.,
            style: Style::default(),
            magnification: 2.,
        }
    }
}

/// The label being typed, in [`Editor::text_edit`]. It becomes a mark when the user
/// finishes.
pub(crate) struct TextDraft {
    pub mark: Mark,
}

/// A numeric field being typed into, in [`Editor::number_edit`]. It applies on Enter or
/// on leaving.
pub(crate) struct NumberDraft {
    pub label: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Panel {
    Tool,
    Image,
    Backdrop,
    Animation,
}

/// Slow work in flight. Only one runs at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Load,
    Transform,
    Save,
    Copy,
    Paste,
    Ocr,
    ColorSample,
    Export,
}

/// What the toolbar says about the last copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyFeedback {
    Copied,
    ReadingText,
    TextCopied,
    NoText,
}

/// What finished off the UI thread. `id` is the operation that asked; a result for
/// any other is stale and dropped.
pub(crate) enum Message {
    Preview(preview::Rendered),
    /// A backdrop or entrance frame for the canvas.
    Frame(Box<stage::Frame>),
    ExportProgress {
        id: u64,
        percent: u32,
    },
    /// `None` when the export was cancelled.
    Exported {
        id: u64,
        result: Result<Option<std::path::PathBuf>, String>,
    },
    /// An opened or pasted image, or a crop, resize, rotation or an undo of one: the
    /// new document with its raster.
    Replaced {
        id: u64,
        result: Result<(Box<Document>, preview::Rendered), String>,
    },
    Saved {
        id: u64,
        result: Result<std::path::PathBuf, String>,
    },
    Copied {
        id: u64,
        result: Result<(), String>,
    },
    Recognized {
        id: u64,
        revision: u64,
        text: Result<String, String>,
    },
    ColorSampled {
        id: u64,
        color: Result<Option<[u8; 3]>, String>,
    },
    /// The flattened image a magnifier being dragged enlarges.
    LensSource {
        request: u64,
        image: Arc<RgbaImage>,
    },
    /// The lens a magnifier shows, sampled from that image.
    LensTile {
        request: u64,
        key: live::LensKey,
        tile: RgbaImage,
    },
}

/// Handed to workers: they post a message and wake the window.
#[derive(Clone)]
pub(crate) struct Mailbox {
    messages: Arc<Mutex<Vec<Message>>>,
    window: AppHandle<Editor>,
}

impl Mailbox {
    pub fn post(&self, message: Message) {
        self.messages.lock().unwrap().push(message);
        self.window.wake();
    }
}

/// A request that arrived during an operation.
enum Deferred {
    Open(u64, Box<Document>),
    Leave { send: bool },
}

pub struct Editor {
    me: AppHandle<Editor>,
    port: Arc<dyn PlatformPort>,
    mailbox: Mailbox,
    doc: Option<Document>,
    /// Changes with every document edit, so workers' results can be checked against it.
    revision: u64,
    preview: Preview,
    gesture: Gesture,
    /// Selected mark indices. The last is the one the sidebar shows.
    selected: Vec<usize>,
    tool: Tool,
    defaults: [ToolSettings; 11],
    crop_ratio: Option<f32>,
    next_counter: Option<u32>,
    text: Option<TextDraft>,
    text_edit: TextEdit,
    number: Option<NumberDraft>,
    number_edit: TextEdit,
    picker: ColorPicker,
    panel: Panel,
    resize_scale: f32,
    resize_smart: bool,
    space_down: bool,
    zoom_down: bool,
    /// None fits the image to the window.
    zoom: Option<f32>,
    pan: (f32, f32),
    /// The canvas's size in points, from its last layout.
    canvas: (f32, f32),
    status: String,
    copy_feedback: Option<CopyFeedback>,
    /// When the toolbar's confirmation stops showing.
    feedback_until: f64,
    now: f64,
    operation: Option<(u64, Operation)>,
    next_operation: u64,
    /// What a magnifier being dragged enlarges: the marks flattened when the drag began.
    live_source: Option<Arc<RgbaImage>>,
    live_lens: rune::Pixels,
    /// The flatten the current drag asked for; any other result is from an abandoned one.
    lens_request: u64,
    lens: live::LensState,
    /// How many points the stroke being drawn may have, set when it starts.
    stroke_room: usize,
    /// The backdrop's settings while it is turned off, so turning it on brings them back.
    backdrop_off: Option<crate::backdrop::Backdrop>,
    /// During a slider drag, every move after the first shares one undo step.
    continuing: bool,
    stage: stage::Stage,
    playback: stage::Playback,
    export: Option<export::Export>,
    /// Between a Dock click opening the window and its close button.
    open: bool,
    capture: Arc<Lens>,
    /// The capture still this window is editing, if it is one.
    pub(crate) still: Option<u64>,
    /// The image that was open when a still replaced it, back when the still is done.
    parked: Option<Document>,
    /// What to do once the running operation ends. The last request wins.
    deferred: Option<Deferred>,
}

impl Editor {
    pub fn new(me: AppHandle<Editor>, port: Arc<dyn PlatformPort>, capture: Arc<Lens>) -> Self {
        let mailbox = Mailbox {
            messages: Default::default(),
            window: me.clone(),
        };
        let stage = stage::Stage::new(port.lane());
        Self {
            me,
            port,
            mailbox,
            doc: None,
            revision: 0,
            preview: Preview::default(),
            gesture: Gesture::Idle,
            selected: vec![],
            tool: Tool::Pen,
            defaults: [ToolSettings::default(); 11],
            crop_ratio: None,
            next_counter: None,
            text: None,
            text_edit: TextEdit::new(""),
            number: None,
            number_edit: TextEdit::new(""),
            picker: ColorPicker::default(),
            panel: Panel::Tool,
            resize_scale: 2.,
            resize_smart: true,
            space_down: false,
            zoom_down: false,
            zoom: None,
            pan: (0., 0.),
            canvas: (0., 0.),
            status: String::new(),
            copy_feedback: None,
            feedback_until: 0.,
            now: 0.,
            operation: None,
            next_operation: 0,
            live_source: None,
            live_lens: rune::Pixels::new(),
            lens_request: 0,
            lens: live::LensState::default(),
            stroke_room: 0,
            backdrop_off: None,
            continuing: false,
            stage,
            playback: stage::Playback::default(),
            export: None,
            open: false,
            capture,
            still: None,
            parked: None,
            deferred: None,
        }
    }

    /// Opens the window; a Dock click lands here.
    pub fn open_window(&mut self) {
        self.open = true;
        self.me.open();
    }

    /// The window closed; Lens keeps running, so the editor only hides. Preview
    /// playback stops until it opens again. Closing it on a still is Back.
    pub fn on_window_close(&mut self, tx: &mut rune::Tx) {
        self.open = false;
        if self.still.is_some() {
            self.leave_for_key(false, tx);
        }
    }

    /// Shows a capture's still, undo and all, in place of whatever image was open.
    pub fn open_still(&mut self, still: u64, document: Document, tx: &mut rune::Tx) {
        // A running operation lands on the image it started on, which is parked next.
        if self.is_busy() {
            self.deferred = Some(Deferred::Open(still, Box::new(document)));
            return;
        }
        // A still left open goes back first, so its edits are kept.
        if self.still.is_some() {
            self.commit_text(tx);
            self.cancel_gesture();
            self.leave_still(false);
            if self.is_busy() {
                self.deferred = Some(Deferred::Open(still, Box::new(document)));
                return;
            }
        }
        if self.still.is_none() {
            self.commit_text(tx);
            self.parked = self.doc.take();
        }
        self.still = Some(still);
        self.doc = None;
        self.show_document(document);
        self.open_window();
    }

    /// Hands the still back with its edits and returns to what was open before.
    pub(crate) fn leave_still(&mut self, send: bool) {
        if self.still.is_none() || self.doc.is_none() {
            return;
        }
        let (Some(still), Some(document)) = (self.still.take(), self.doc.take()) else {
            return;
        };
        self.capture.still_edited(still, document);
        if let Some(parked) = self.parked.take() {
            self.show_document(parked);
        }
        self.open = false;
        self.me.hide();
        self.capture.finish_editing(still);
        if send {
            self.capture.send_edited();
        }
    }

    /// Renders `document` off the UI thread and installs it with its raster.
    fn show_document(&mut self, document: Document) {
        let id = self.start(Operation::Load);
        let mailbox = self.mailbox.clone();
        self.port.run(Box::new(move || {
            let rendered = preview::render(&document);
            let result = Ok((Box::new(document), rendered));
            mailbox.post(Message::Replaced { id, result });
        }));
    }

    /// Back or Send from outside the toolbar: a key, Lens, or the window closing.
    /// A running operation is waited for, so its result lands on the still.
    pub fn leave_for_key(&mut self, send: bool, tx: &mut rune::Tx) {
        if self.is_busy() {
            // A queued open never happened: only the capture has to come back.
            if let Some(Deferred::Open(queued, _)) = self.deferred.take() {
                self.capture.finish_editing(queued);
                if self.still.is_none() {
                    if send {
                        self.capture.send_edited();
                    }
                    return;
                }
            }
            self.deferred = Some(Deferred::Leave { send });
            return;
        }
        let action = if send {
            actions::Action::SendCapture
        } else {
            actions::Action::BackToCapture
        };
        self.dispatch_ui(action, tx);
    }

    pub(crate) fn doc(&self) -> &Document {
        self.doc
            .as_ref()
            .expect("actions on a document are gated in dispatch")
    }

    pub(crate) fn doc_mut(&mut self) -> &mut Document {
        self.doc
            .as_mut()
            .expect("actions on a document are gated in dispatch")
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.operation.is_some()
    }

    /// The tool the sidebar shows: the selected mark's, otherwise the active one.
    pub(crate) fn options_tool(&self) -> Tool {
        self.primary_mark().map_or(self.tool, |mark| mark.tool)
    }

    pub(crate) fn primary_mark(&self) -> Option<&Mark> {
        let doc = self.doc.as_ref()?;
        doc.marks.get(*self.selected.last()?)
    }

    pub(crate) fn tool_settings(&self) -> ToolSettings {
        match self.primary_mark() {
            Some(mark) => ToolSettings {
                color: mark.color,
                width: mark.width,
                style: mark.style,
                magnification: crate::effects::zoom(mark),
            },
            None => self.defaults[self.tool.index()],
        }
    }

    pub(crate) fn counter_number(&self) -> u32 {
        if let Some(mark) = self.primary_mark().filter(|m| m.tool == Tool::Counter) {
            return mark.text.parse().unwrap_or(1);
        }
        self.next_counter.unwrap_or_else(|| {
            self.doc
                .iter()
                .flat_map(|doc| &doc.marks)
                .filter(|m| m.tool == Tool::Counter)
                .filter_map(|m| m.text.parse::<u32>().ok())
                .max()
                .unwrap_or(0)
                .saturating_add(1)
                .min(999)
        })
    }

    /// Where the image sits in the canvas right now.
    pub(crate) fn layout(&self) -> Layout {
        let Some(doc) = &self.doc else {
            return Layout::default();
        };
        let (width, height) = doc.base.dimensions();
        let frame = self.frame_backdrop().map(|b| b.layout((width, height)));
        let (output_width, output_height) =
            frame.as_ref().map_or((width, height), |f| f.dimensions);
        let origin = frame.map_or((0, 0), |f| f.origin);
        // Fit leaves room around the picture, and never enlarges it.
        let fit = ((self.canvas.0 - 80.) / output_width as f32)
            .min((self.canvas.1 - 70.) / output_height as f32)
            .clamp(0.01, 1.);
        let scale = self.zoom.unwrap_or(fit);
        Layout {
            x: (self.canvas.0 - output_width as f32 * scale) / 2.
                + origin.0 as f32 * scale
                + self.pan.0,
            y: (self.canvas.1 - output_height as f32 * scale) / 2.
                + origin.1 as f32 * scale
                + self.pan.1,
            scale,
            width: width as f32,
            height: height as f32,
        }
    }

    /// Runs what arrived from workers. True when the window should redraw.
    pub(crate) fn poll(&mut self, now: f64, tx: &mut rune::Tx) -> bool {
        self.now = now;
        if let Some(next) = self.port.tick(now) {
            tx.wake_after(next);
        }
        let expired = self
            .copy_feedback
            .is_some_and(|f| f != CopyFeedback::ReadingText)
            && now >= self.feedback_until;
        if expired {
            self.copy_feedback = None;
        }
        let messages = std::mem::take(&mut *self.mailbox.messages.lock().unwrap());
        let changed = expired || !messages.is_empty();
        for message in messages {
            self.receive(message, tx);
        }
        if let Some(next) = self.advance(now) {
            tx.wake_after(next);
            return true;
        }
        changed
    }
}
