use super::actions::Action;
use super::{Editor, preview_base, render_image};
use super::{feedback::CopyFeedback, state::Gesture};
use crate::{document::Document, glance};
use gpui::*;
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OperationId(u64);
impl OperationId {
    pub(crate) fn value(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum OperationKind {
    Capture,
    ColorSample,
    Open,
    Paste,
    Copy,
    Save,
    Upload,
    Crop,
    Transform,
    Ocr,
    Ask,
    Video,
}
pub(super) struct ActiveOperation {
    pub(super) id: OperationId,
    pub(super) kind: OperationKind,
}
#[derive(Default)]
pub(super) struct OperationState {
    pub(super) active: Option<ActiveOperation>,
    next_id: u64,
}
pub(crate) enum Message {
    AskProgress {
        id: OperationId,
        steps: usize,
        status: String,
    },
    AskFinished {
        id: OperationId,
        result: Result<crate::chatgpt::agent::ResultDocument, String>,
    },
    ChatgptAuthorization {
        generation: u64,
        url: String,
    },
    ChatgptFinished {
        generation: u64,
        outcome: super::chatgpt::Outcome,
    },
    Accessibility(super::panels::number::Scope, Action),
    AccessibilityPopup(
        super::panels::number::Scope,
        super::panels::Popup,
        Option<usize>,
    ),
    AccessibilityNumber(super::panels::number::Scope, &'static str, f32),
    Automation(crate::automation::Request),
    Lens(crate::effects::LensKey, Arc<RenderImage>),
    #[cfg(any(target_os = "macos", test))]
    Magnify(f32, (f32, f32), bool),
    #[cfg(target_os = "macos")]
    Hotkey(bool),
    Preview(u64, usize, u32, Arc<RenderImage>),
    MotionPreviewReady,
    Operation(OperationId, OperationResult),
    VideoProgress(OperationId, u32),
}
pub(crate) enum OperationResult {
    ExtractedText {
        revision: u64,
        rectangle: [u32; 4],
        result: Result<String, String>,
        engine: crate::chatgpt::OcrEngine,
    },
    ToolColorSample {
        tool: crate::document::Tool,
        selected: Option<usize>,
        revision: u64,
        result: Result<Option<[u8; 3]>, String>,
    },
    ColorSample {
        stop: usize,
        revision: u64,
        result: Result<Option<[u8; 3]>, String>,
    },
    Cropped(Document, Arc<RenderImage>),
    Image(Result<Option<image::RgbaImage>, String>),
    VideoSaved(Result<Option<std::path::PathBuf>, String>),
    Saved(Result<Option<std::path::PathBuf>, String>),
    Copied(Result<(), String>),
    RemoteCopied(Result<glance::Share, String>),
    Transformed(Result<(Document, usize, Arc<RenderImage>), String>),
    Failed(String),
}
impl Editor {
    pub(super) fn schedule_preview(&mut self) {
        if self.preview.rendering {
            return;
        }
        self.preview.rendering = true;
        let mut document = self.document.render_snapshot();
        if let Some(index) = self.interaction.gesture.first_drag_index() {
            document.marks.truncate(index);
        }
        let revision = self.preview.revision;
        let count = document.marks.len();
        let inside_padding = document.backdrop.map_or(0, |b| b.inside_padding);
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let image = render_image(preview_base(&document));
            let _ = sender.send_blocking(Message::Preview(revision, count, inside_padding, image));
        });
    }
    pub(super) fn changed(&mut self) {
        self.preview.revision += 1;
        self.extraction = None;
        self.schedule_preview();
    }
    pub(super) fn is_busy(&self) -> bool {
        self.operations.active.is_some() || self.preview.waiting
    }
    pub(super) fn start_operation(&mut self, kind: OperationKind) -> Option<OperationId> {
        if self.is_busy() {
            return None;
        }
        self.cancel_gesture();
        self.operations.next_id += 1;
        let id = OperationId(self.operations.next_id);
        self.operations.active = Some(ActiveOperation { id, kind });
        Some(id)
    }
    pub(super) fn spawn_operation(
        &self,
        id: OperationId,
        work: impl FnOnce() -> OperationResult + Send + 'static,
    ) {
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                .unwrap_or_else(|_| {
                    OperationResult::Failed("The operation stopped unexpectedly. Try again.".into())
                });
            let _ = sender.send_blocking(Message::Operation(id, result));
        });
    }
    pub(super) fn receive(&mut self, message: Message, cx: &mut Context<Self>) {
        match message {
            Message::AskProgress { id, steps, status } => {
                if self.ask.running.as_ref().is_some_and(|run| run.id == id) {
                    self.ask.steps = steps;
                    self.ask.status = status;
                    cx.notify();
                }
            }
            Message::AskFinished { id, result } => self.finish_ask(id, result, cx),
            Message::ChatgptAuthorization { generation, url } => {
                if generation == self.chatgpt.generation
                    && self.chatgpt.signing_in
                    && !self
                        .chatgpt
                        .cancel
                        .load(std::sync::atomic::Ordering::Relaxed)
                {
                    cx.open_url(&url);
                }
            }
            Message::ChatgptFinished {
                generation,
                outcome,
            } => {
                if generation == self.chatgpt.generation {
                    self.chatgpt.busy = false;
                    self.chatgpt.signing_in = false;
                    if let Some(snapshot) = outcome.snapshot {
                        self.chatgpt.snapshot = snapshot;
                    }
                    self.chatgpt.status = outcome.status;
                    self.chatgpt.error = outcome.error;
                    cx.notify();
                }
            }

            Message::Accessibility(scope, action) => {
                if scope == self.tool_scope() {
                    self.dispatch_ui(action, cx);
                }
            }
            Message::AccessibilityPopup(scope, popup, index) => {
                if scope == self.tool_scope() {
                    if let Some(index) = index {
                        if self.panels.popup == Some(popup) {
                            self.choose_popup(popup, index, cx);
                        }
                    } else {
                        self.open_popup(popup, cx);
                    }
                }
            }
            Message::AccessibilityNumber(scope, label, value) => {
                if scope == self.tool_scope() {
                    self.dispatch_ui(self.number_action(label, value), cx);
                }
            }
            Message::Automation(request) => self.automation(request, cx),
            Message::MotionPreviewReady => cx.notify(),
            Message::Lens(key, image) => {
                self.preview.lens_rendering = false;
                if self.preview.lens_wanted == Some(key) {
                    if let Some((_, old)) = self.preview.lens.replace((key, image)) {
                        self.preview.retired.push(old);
                    }
                } else {
                    self.preview.retired.push(image);
                }
                cx.notify();
            }
            #[cfg(any(target_os = "macos", test))]
            Message::Magnify(delta, position, smart) => {
                if self.is_busy() || self.interaction.gesture.is_active() {
                    return;
                }
                let action = if smart && self.viewport.zoom.is_some_and(|z| z >= 1.) {
                    Action::Fit
                } else {
                    let factor = if smart {
                        1. / self
                            .viewport
                            .zoom
                            .unwrap_or(self.viewport.layout.get().scale)
                            .max(0.01)
                    } else {
                        1. + delta
                    };
                    Action::ZoomAt {
                        factor,
                        anchor: position,
                    }
                };
                self.dispatch_ui(action, cx);
            }
            #[cfg(target_os = "macos")]
            Message::Hotkey(area) => {
                self.dispatch_ui(Action::Capture { area }, cx);
            }
            Message::Preview(revision, count, inside_padding, image) => {
                self.preview.rendering = false;
                if revision == self.preview.revision {
                    self.preview
                        .retired
                        .push(std::mem::replace(&mut self.preview.image, image));
                    self.preview.mark_count = count;
                    self.preview.inside_padding = inside_padding;
                    if self.preview.waiting {
                        self.preview.waiting = false;

                        self.feedback.status = "Ready".into();
                    }
                } else {
                    self.schedule_preview();
                }
                cx.notify();
            }
            Message::VideoProgress(id, percent) => {
                if self
                    .operations
                    .active
                    .as_ref()
                    .is_some_and(|op| op.id == id && op.kind == OperationKind::Video)
                {
                    self.video_export.progress = Some(percent);
                    cx.notify();
                }
            }
            Message::Operation(id, result) => {
                if self.operations.active.as_ref().is_none_or(|op| op.id != id) {
                    return;
                }
                self.operations.active = None;
                let chatgpt_ocr = matches!(
                    &result,
                    OperationResult::ExtractedText {
                        engine: crate::chatgpt::OcrEngine::Chatgpt,
                        ..
                    }
                );
                self.receive_result(result, cx);
                if chatgpt_ocr && !self.chatgpt.busy {
                    let _ = self.chatgpt_job(super::chatgpt::Job::Load, cx);
                }
            }
        }
    }
    fn receive_result(&mut self, result: OperationResult, cx: &mut Context<Self>) {
        let failed = matches!(
            &result,
            OperationResult::Failed(_)
                | OperationResult::ColorSample { result: Err(_), .. }
                | OperationResult::ToolColorSample { result: Err(_), .. }
                | OperationResult::Image(Err(_))
                | OperationResult::VideoSaved(Err(_))
                | OperationResult::Saved(Err(_))
                | OperationResult::Copied(Err(_))
                | OperationResult::RemoteCopied(Err(_))
                | OperationResult::Transformed(Err(_))
                | OperationResult::ExtractedText { result: Err(_), .. }
        );
        match result {
            OperationResult::ExtractedText {
                revision,
                rectangle,
                result,
                engine,
            } => {
                if revision != self.preview.revision {
                    self.set_copy_feedback(None, cx);
                    self.feedback.status =
                        "Image changed during text extraction; extract again".into();
                } else if engine == crate::chatgpt::OcrEngine::Chatgpt
                    && self
                        .chatgpt
                        .inference_cancel
                        .load(std::sync::atomic::Ordering::Relaxed)
                {
                    self.set_copy_feedback(None, cx);
                    self.feedback.status = "ChatGPT OCR canceled".into();
                } else {
                    match result {
                        Ok(text) => {
                            if text.is_empty() {
                                self.feedback.status =
                                    "No text found. Try a clearer image or a smaller crop.".into();
                                self.set_copy_feedback(Some(CopyFeedback::NoText), cx);
                            } else {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                                self.feedback.status = "Copied recognized text to clipboard".into();
                                self.set_copy_feedback(Some(CopyFeedback::TextCopied), cx);
                            }
                            self.extraction = Some(super::Extraction {
                                revision,
                                rectangle,
                                text,
                                engine,
                            });
                        }
                        Err(error) => {
                            self.set_copy_feedback(None, cx);
                            self.feedback.status = error;
                        }
                    }
                }
            }
            OperationResult::ToolColorSample {
                tool,
                selected,
                revision,
                result,
            } => {
                if revision != self.preview.revision
                    || tool != self.options_tool()
                    || selected != self.interaction.selected
                {
                    self.feedback.status =
                        "Annotation changed while sampling; pick the color again".into();
                } else {
                    match result {
                        Ok(Some(rgb)) => {
                            let mut color = self.tool_settings().color;
                            color[..3].copy_from_slice(&rgb);
                            self.dispatch_ui(super::actions::Action::SetColor { color }, cx);
                        }
                        Ok(None) => self.feedback.status = "Color sampling canceled".into(),
                        Err(error) => self.feedback.status = error,
                    }
                }
            }
            OperationResult::ColorSample {
                stop,
                revision,
                result,
            } => {
                if revision != self.preview.revision {
                    self.feedback.status =
                        "Image changed while sampling; pick the color again".into();
                } else {
                    match result {
                        Ok(Some(rgb)) => self.dispatch_ui(
                            super::actions::Action::SetBackdropColor { stop, rgb },
                            cx,
                        ),
                        Ok(None) => self.feedback.status = "Color sampling canceled".into(),
                        Err(error) => self.feedback.status = error,
                    }
                }
            }
            OperationResult::Transformed(Ok((document, count, image))) => {
                self.set_selection(Vec::new());
                self.interaction.gesture = Gesture::Idle;
                self.preview.inside_padding = document.backdrop.map_or(0, |b| b.inside_padding);
                self.document = document;
                self.preview.revision += 1;
                self.preview
                    .retired
                    .push(std::mem::replace(&mut self.preview.image, image));
                self.preview.mark_count = count;
                self.viewport.zoom = None;
                self.viewport.pan = (0., 0.);
            }
            OperationResult::Transformed(Err(e)) => {
                self.feedback.status = e;
            }
            OperationResult::Cropped(document, image) => {
                let count = document.marks.len();
                self.set_selection(Vec::new());
                self.interaction.gesture = Gesture::Idle;
                self.preview.inside_padding = document.backdrop.map_or(0, |b| b.inside_padding);
                self.document = document;
                self.preview.revision += 1;
                self.preview
                    .retired
                    .push(std::mem::replace(&mut self.preview.image, image));
                self.preview.mark_count = count;
                self.viewport.zoom = None;
                self.viewport.pan = (0., 0.);
                self.feedback.status = "Cropped • ⌘Z to restore".into();
            }
            OperationResult::Image(Ok(Some(image))) => {
                self.set_selection(Vec::new());
                self.interaction.gesture = Gesture::Idle;
                self.document = Document::new(image);
                self.panels.backdrop_disabled = None;
                self.panels.sampling_color = None;
                self.panels.sampling_tool_color = false;
                self.panels.popup = None;
                self.viewport.zoom = None;
                self.viewport.pan = (0., 0.);
                self.preview.mark_count = usize::MAX;
                self.preview.waiting = true;
                self.changed();
                self.feedback.status = "Preparing image…".into();
            }
            OperationResult::Image(Ok(None)) => {
                self.feedback.status = "Selection canceled".into();
            }
            OperationResult::Image(Err(e)) => {
                self.feedback.status = e;
            }
            OperationResult::VideoSaved(result) => {
                self.video_export.progress = None;
                self.video_export.cancel = None;
                self.feedback.status = match result {
                    Ok(Some(path)) => {
                        self.video_export.last_video = Some(path.clone());
                        format!("Animation saved to {}", path.display())
                    }
                    Ok(None) => "Animation export canceled".into(),
                    Err(e) => e,
                };
            }
            OperationResult::Saved(result) => {
                self.feedback.status = match result {
                    Ok(Some(path)) => format!("Saved {}", path.display()),
                    Ok(None) => "Save canceled".into(),
                    Err(e) => e,
                };
            }
            OperationResult::Copied(result) => {
                self.set_copy_feedback(result.is_ok().then_some(CopyFeedback::Copied), cx);
                self.feedback.status = match result {
                    Ok(()) => "Copied image to clipboard".into(),
                    Err(e) => e,
                };
            }
            OperationResult::RemoteCopied(result) => {
                self.feedback.status = match result {
                    Ok(share) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(format!(
                            "Screenshot: {}",
                            share.url
                        )));
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        let minutes = share.expires_at.saturating_sub(now).div_ceil(60_000);
                        self.set_copy_feedback(Some(CopyFeedback::LinkCopied(minutes)), cx);
                        format!(
                            "Glance link copied • expires in {minutes} min • Paste into your agent’s chat"
                        )
                    }
                    Err(e) => {
                        self.set_copy_feedback(None, cx);
                        e
                    }
                };
            }
            OperationResult::Failed(error) => {
                self.feedback.status = error;
                self.set_copy_feedback(None, cx);
                self.video_export.progress = None;
                self.video_export.cancel = None;
            }
        }
        crate::platform::show_editor(cx);
        if failed
            && self.feedback.status != "ChatGPT OCR canceled"
            && let Some(window) = cx.windows().first().copied()
        {
            let detail = self.feedback.status.clone();
            if let Ok(answer) = window.update(cx, |_, window, cx| {
                window.prompt(
                    PromptLevel::Critical,
                    "Glance couldn’t complete the operation",
                    Some(&detail),
                    &["OK"],
                    cx,
                )
            }) {
                cx.spawn(async move |_, _| {
                    let _ = answer.await;
                })
                .detach();
            }
        }
        cx.notify();
    }
}
