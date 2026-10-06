//! GPUI editor entity: state ownership and application integration.
mod accessibility;
#[cfg(test)]
mod action_tests;
pub(crate) mod actions;
mod automation;
mod canvas;
mod chatgpt;
mod commands;
mod dispatch;
mod feedback;
mod input;
mod jobs;
mod lens;
mod panels;
mod state;
#[cfg(test)]
mod tests;
mod text_input;
mod view;

use crate::document::{self, Document, Tool};
#[cfg(target_os = "macos")]
use crate::gestures;
#[cfg(target_os = "macos")]
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use gpui::*;
pub(crate) use jobs::Message;
use jobs::OperationState;
pub(crate) use state::Layout;
use state::{
    FeedbackState, Gesture, InteractionState, PanelState, PlaybackState, PreviewState,
    VideoExportState, ViewportState,
};
use std::{cell::Cell, rc::Rc, sync::Arc};
pub(crate) struct Editor {
    accessibility: crate::accessibility::Tree,
    color_pickers: [Entity<crate::color_picker::ColorPicker>; 2],
    tool_color_picker: Entity<crate::color_picker::ColorPicker>,
    tool_picker_target: Option<(Tool, Option<usize>, u64)>,
    number_inputs: std::collections::BTreeMap<&'static str, Entity<panels::number::NumberInput>>,
    extraction: Option<Extraction>,
    chatgpt: chatgpt::State,
    #[cfg(test)]
    recognize_text: crate::ocr::Recognizer,
    _color_subscriptions: Vec<Subscription>,
    document: Document,
    interaction: InteractionState,
    viewport: ViewportState,
    preview: PreviewState,
    playback: PlaybackState,
    video_export: VideoExportState,
    panels: PanelState,
    feedback: FeedbackState,
    #[cfg(target_os = "macos")]
    _gestures: Option<gestures::Monitor>,
    pub(crate) focus: FocusHandle,
    operations: OperationState,
    sender: async_channel::Sender<Message>,
    #[cfg(target_os = "macos")]
    _hotkeys: Option<GlobalHotKeyManager>,
}
#[derive(serde::Serialize)]
struct Extraction {
    revision: u64,
    rectangle: [u32; 4],
    text: String,
    engine: crate::chatgpt::OcrEngine,
}
pub(crate) fn render_image(mut image: image::RgbaImage) -> Arc<RenderImage> {
    for p in image.pixels_mut() {
        p.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(
        image
    )]))
}
fn preview_base(doc: &Document) -> image::RgbaImage {
    let source = doc.render(None);
    let image = if let Some(b) = doc.backdrop.filter(|b| b.inside_padding > 0) {
        b.extend_edges(&source).into_owned()
    } else {
        source
    };
    image::DynamicImage::ImageRgba8(image)
        .thumbnail(1600, 1200)
        .to_rgba8()
}

impl Editor {
    pub(crate) fn new(cx: &mut Context<Self>, image: Option<image::RgbaImage>) -> Self {
        Self::with_document(
            cx,
            true,
            Document::new(image.unwrap_or_else(document::demo)),
        )
    }
    #[cfg(test)]
    pub(super) fn with_native(cx: &mut Context<Self>, native: bool) -> Self {
        Self::with_document(cx, native, Document::new(document::demo()))
    }
    fn with_document(cx: &mut Context<Self>, native: bool, document: Document) -> Self {
        let base = preview_base(&document);
        let preview = render_image(base.clone());
        let (sender, receiver) = async_channel::unbounded();
        let motion_sender = sender.clone();
        let motion_preview = Rc::new(std::cell::RefCell::new(crate::animation::Preview::new(
            move || {
                let _ = motion_sender.try_send(Message::MotionPreviewReady);
            },
        )));
        let composition_sender = sender.clone();
        let composition_preview = Rc::new(std::cell::RefCell::new(
            crate::animation::CompositionPreview::new(move || {
                let _ = composition_sender.try_send(Message::MotionPreviewReady);
            }),
        ));
        #[cfg(target_os = "macos")]
        let mut status = "Practice on this canvas, or capture your screen with ⌘⌥2".to_string();
        #[cfg(target_os = "linux")]
        let status =
            "Practice here, or launch glance --capture-area from a Hyprland binding".to_string();
        #[cfg(target_os = "macos")]
        let area = HotKey::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::Digit2);
        #[cfg(target_os = "macos")]
        let full = HotKey::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::Digit3);
        #[cfg(target_os = "macos")]
        let hotkeys = if !native {
            None
        } else {
            match GlobalHotKeyManager::new() {
                Ok(manager) => {
                    for key in [area, full] {
                        if let Err(e) = manager.register(key) {
                            status = format!("Shortcut unavailable: {e}. Use the capture buttons.");
                        }
                    }
                    Some(manager)
                }
                Err(e) => {
                    status = format!("Global shortcuts unavailable: {e}");
                    None
                }
            }
        };
        if native
            && std::env::args().any(|arg| arg == "--automation")
            && let Err(error) = crate::automation::listen(sender.clone())
        {
            eprintln!("Glance automation: {error}");
        }
        #[cfg(target_os = "macos")]
        let hotkey_sender = sender.clone();
        #[cfg(target_os = "macos")]
        if native {
            GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
                if event.state == HotKeyState::Pressed {
                    if event.id == area.id() {
                        let _ = hotkey_sender.try_send(Message::Hotkey(true));
                    }
                    if event.id == full.id() {
                        let _ = hotkey_sender.try_send(Message::Hotkey(false));
                    }
                }
            }));
        }
        cx.spawn(async move |view, cx| {
            while let Ok(message) = receiver.recv().await {
                if view
                    .update(cx, |editor, cx| editor.receive(message, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let canvas_bounds = Rc::new(Cell::new(Bounds::default()));
        let focus = cx.focus_handle();
        let color_pickers = std::array::from_fn(|stop| {
            cx.new(|cx| {
                crate::color_picker::ColorPicker::new(
                    if stop == 0 { "Color" } else { "Color 2" },
                    focus.clone(),
                    cx,
                )
            })
        });
        let mut color_subscriptions: Vec<_> = color_pickers
            .iter()
            .enumerate()
            .map(|(stop, picker)| {
                cx.subscribe(picker, move |this, _, event, cx| {
                    use crate::color_picker::ColorPickerEvent;
                    let action = match event {
                        ColorPickerEvent::Changed(rgb) => {
                            actions::Action::SetBackdropColor { stop, rgb: *rgb }
                        }
                        ColorPickerEvent::PickScreen => {
                            actions::Action::PickBackdropScreenColor { stop }
                        }
                    };
                    this.dispatch_ui(action, cx);
                })
            })
            .collect();
        let tool_color_picker =
            cx.new(|cx| crate::color_picker::ColorPicker::new("Custom color", focus.clone(), cx));
        color_subscriptions.push(cx.subscribe(&tool_color_picker, |this, _, event, cx| {
            use crate::color_picker::ColorPickerEvent;
            let action = match event {
                ColorPickerEvent::Changed(rgb) => {
                    let mut color = this.tool_settings().color;
                    color[..3].copy_from_slice(rgb);
                    actions::Action::SetColor { color }
                }
                ColorPickerEvent::PickScreen => actions::Action::PickToolScreenColor,
            };
            this.dispatch_ui(action, cx);
        }));
        let number_inputs = panels::number::FIELDS
            .into_iter()
            .map(|label| {
                let input = cx.new(|cx| panels::number::NumberInput::new(label, focus.clone(), cx));
                color_subscriptions.push(cx.subscribe(&input, move |this, _, event, cx| {
                    if event.scope == this.tool_scope() {
                        this.dispatch_ui(this.number_action(label, event.value), cx);
                    }
                }));
                (label, input)
            })
            .collect();
        #[cfg(target_os = "macos")]
        let gestures =
            native.then(|| gestures::Monitor::new(sender.clone(), canvas_bounds.clone()));
        Self {
            accessibility: crate::accessibility::Tree::new(native),
            color_pickers,
            tool_color_picker,
            tool_picker_target: None,
            number_inputs,
            extraction: None,
            chatgpt: chatgpt::State::new(native, sender.clone()),
            #[cfg(test)]
            recognize_text: crate::ocr::extract,
            _color_subscriptions: color_subscriptions,
            document,
            interaction: InteractionState {
                gesture: Gesture::Idle,
                selected: None,
                selected_others: Vec::new(),

                tool: Tool::Select,
                color: [255, 56, 100, 255],
                width: 5.,
                defaults: [Default::default(); 11],
                crop_ratio: None,
                next_counter: None,

                text_edit: None,
                text_session: 0,
            },
            viewport: ViewportState {
                canvas_bounds,
                space_down: false,
                zoom_down: false,
                zoom: None,
                pan: (0., 0.),

                layout: Rc::new(Cell::new(Layout::default())),
            },
            preview: PreviewState {
                inside_padding: 0,
                lens: None,
                lens_wanted: None,
                lens_rendering: false,
                revision: 0,
                mark_count: 0,
                rendering: false,
                waiting: false,
                image: preview,
                retired: vec![],
            },
            playback: PlaybackState {
                motion_preview,
                composition_preview,
                seek: 0,
                epoch: std::time::Instant::now(),
                paused: false,
                preparing: false,
                position: 0.,
            },
            video_export: VideoExportState {
                last_video: None,
                progress: None,
                cancel: None,
            },
            panels: PanelState {
                sampling_color: None,
                sampling_tool_color: false,
                backdrop_disabled: None,
                popup: None,
                popup_index: 0,
                backdrop: false,
                enhance: false,
                animation: false,
                resize_scale: 2.,
                resize_smart: true,
            },
            feedback: FeedbackState {
                status,
                copy: None,
                timer: None,
            },
            #[cfg(target_os = "macos")]
            _gestures: gestures,
            focus,
            operations: OperationState::default(),
            sender,
            #[cfg(target_os = "macos")]
            _hotkeys: hotkeys,
        }
    }
}
