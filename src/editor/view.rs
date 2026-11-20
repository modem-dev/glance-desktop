//! The window: toolbar over canvas and sidebar.
use super::{
    CopyFeedback, Editor,
    actions::{Action, AnimationFormat},
    icons,
};
use crate::document::Tool;
use rune::{Cursor, Cx, El, Icon, PopupItem, PopupMenu, col, el, icon, row, text, tok};

const TOOLS: [(Tool, &str, &Icon, &str); 11] = [
    (
        Tool::Select,
        "tool-select",
        &icons::MOUSE_POINTER_2,
        "Select (V)",
    ),
    (Tool::Pen, "tool-pen", &icons::PEN_LINE, "Draw (P)"),
    (
        Tool::Arrow,
        "tool-arrow",
        &icons::ARROW_UP_RIGHT,
        "Arrow (A)",
    ),
    (Tool::Rectangle, "tool-rectangle", &icons::SQUARE, "Box (R)"),
    (Tool::Text, "tool-text", &icons::TYPE, "Text (T)"),
    (
        Tool::Highlight,
        "tool-highlight",
        &icons::HIGHLIGHTER,
        "Highlight (H)",
    ),
    (
        Tool::Pixelate,
        "tool-pixelate",
        &icons::GRID_2X2,
        "Pixelate (B)",
    ),
    (Tool::Crop, "tool-crop", &icons::CROP, "Crop (X)"),
    (
        Tool::Counter,
        "tool-counter",
        &icons::LIST_ORDERED,
        "Step (N)",
    ),
    (
        Tool::Spotlight,
        "tool-spotlight",
        &icons::SCAN,
        "Spotlight (S)",
    ),
    (
        Tool::Magnifier,
        "tool-magnifier",
        &icons::SEARCH,
        "Magnifier (M)",
    ),
];

fn toolbar(s: &Editor, cx: &Cx<Editor>) -> El {
    let t = tok::theme(cx.system_dark());
    let button =
        |key: &str, label: &str, glyph: &'static Icon, toggle: Option<bool>, action: Action| {
            super::panels::controls::icon_button(cx, key, label.to_string(), glyph, toggle, action)
        };
    let mut tools = row()
        .gap(2.)
        .center_y()
        .children(
            s.still
                .map(|_| capture_button(cx, "back", "‹ Back", Action::BackToCapture)),
        )
        .child(
            text("attn lens")
                .font(tok::type_::SECTION)
                .color(t.text)
                .px(8.),
        )
        .child(button(
            "open",
            "Open image (⌘O)",
            &icons::FOLDER_OPEN,
            None,
            Action::OpenImage,
        ))
        .child(el().w(1.).h(22.).fill(t.separator));
    for (tool, key, glyph, label) in TOOLS {
        tools = tools.child(button(
            key,
            label,
            glyph,
            Some(s.tool == tool),
            Action::SelectTool(tool),
        ));
    }
    tools = tools
        .child(el().w(1.).h(22.).fill(t.separator))
        .child(button(
            "backdrop-button",
            "Backdrop",
            &icons::SQUARE,
            Some(s.panel == super::Panel::Backdrop),
            Action::ToggleBackdrop,
        ))
        .child(button(
            "animation-button",
            "Animation",
            &icons::PLAY,
            Some(s.panel == super::Panel::Animation),
            Action::ToggleAnimationPanel,
        ))
        .child(button(
            "image-tools-button",
            "Image tools",
            &icons::MAXIMIZE,
            Some(s.panel == super::Panel::Image),
            Action::ToggleImageTools,
        ));
    let feedback = s.copy_feedback.map(|feedback| match feedback {
        CopyFeedback::Copied => "Copied!",
        CopyFeedback::ReadingText => "Reading text…",
        CopyFeedback::TextCopied => "Text copied!",
        CopyFeedback::NoText => "No text found",
    });
    let zoom = if s.doc.is_some() {
        format!("{:.0}%", (s.layout().scale * 100.).round())
    } else {
        "—".into()
    };
    let actions = row()
        .gap(2.)
        .center_y()
        .child(button(
            "undo",
            "Undo (⌘Z)",
            &icons::UNDO_2,
            None,
            Action::Undo,
        ))
        .child(button(
            "redo",
            "Redo (⇧⌘Z)",
            &icons::REDO_2,
            None,
            Action::Redo,
        ))
        .child(button(
            "copy",
            "Copy image (⌘C)",
            &icons::COPY,
            None,
            Action::CopyImage,
        ))
        .child(button(
            "copy-ocr",
            "Copy as OCR",
            &icons::SCAN_TEXT,
            None,
            Action::CopyOcr,
        ))
        .child(button(
            "save",
            "Save PNG (⌘S)",
            &icons::SAVE,
            None,
            Action::SaveImage,
        ))
        .child(export_menu(s, cx))
        .child(button(
            "zoom-out",
            "Zoom out (⌘−)",
            &icons::ZOOM_OUT,
            None,
            Action::Zoom(0.8),
        ))
        .child(
            el().key("zoom-label")
                .role("button")
                .label("Fit to window (⌘1)")
                .a11y_value(zoom.clone())
                .px(4.)
                .center()
                .cursor(Cursor::Pointer)
                .on_press_tx(|s: &mut Editor, tx| s.dispatch_ui(Action::Fit, tx))
                .child(
                    text(zoom)
                        .font(tok::type_::LABEL)
                        .color(t.text_secondary)
                        .inert(),
                ),
        )
        .child(button(
            "zoom-in",
            "Zoom in (⌘=)",
            &icons::ZOOM_IN,
            None,
            Action::Zoom(1.25),
        ))
        .children(feedback.map(|feedback| {
            text(feedback)
                .key("copy-feedback")
                .font(tok::type_::LABEL)
                .color(t.text)
                .px(6.)
                .live_region(rune::LiveRegion::Polite)
        }));
    row()
        .key("toolbar")
        .w_full()
        .h(48.)
        .pl(8. + cx.titlebar_inset())
        .pr(8.)
        .center_y()
        .justify_between()
        .fill(t.card)
        .child(tools)
        .child(
            row().gap(8.).center_y().child(actions).children(
                s.still
                    .map(|_| capture_button(cx, "send-capture", "Send ↩", Action::SendCapture)),
            ),
        )
}

/// A toolbar button that belongs to the capture a still came from.
fn capture_button(cx: &Cx<Editor>, key: &str, label: &str, action: Action) -> El {
    let t = tok::theme(cx.system_dark());
    el().key(key)
        .h(32.)
        .px(10.)
        .center()
        .radius(8.)
        .fill(t.control_track)
        .cursor(Cursor::Pointer)
        .on_press_tx(move |s: &mut Editor, tx| s.dispatch_ui(action.clone(), tx))
        .child(
            text(label.to_string())
                .font(tok::type_::LABEL)
                .color(t.text)
                .inert(),
        )
}

/// Export PNG, MP4 or GIF, or cancel the export running.
fn export_menu(s: &Editor, cx: &Cx<Editor>) -> El {
    let t = tok::theme(cx.system_dark());
    let exporting = s.export.is_some();
    el().key("export")
        .role("button")
        .label("Export")
        .help("Export PNG / MP4 / GIF • ⌘S saves PNG")
        .size(32., 32.)
        .radius(7.)
        .center()
        .cursor(Cursor::Pointer)
        .hover(t.hover)
        .press(t.press)
        .on_press_tx(move |_: &mut Editor, tx| {
            let menu = if exporting {
                PopupMenu::new().item(PopupItem::new("cancel", "Cancel export"))
            } else {
                PopupMenu::new()
                    .item(PopupItem::new("png", "Save PNG…"))
                    .item(PopupItem::new("mp4", "Export MP4…"))
                    .item(PopupItem::new("gif", "Export GIF…"))
            };
            tx.popup_menu(menu, "export", |s: &mut Editor, picked, tx| {
                let action = match picked {
                    Some("png") => Action::SaveImage,
                    Some("mp4") => Action::Export(AnimationFormat::Mp4),
                    Some("gif") => Action::Export(AnimationFormat::Gif),
                    Some("cancel") => Action::CancelExport,
                    _ => return,
                };
                s.dispatch_ui(action, tx);
            });
        })
        .child(icon(&icons::VIDEO, 18.).color(t.text).inert())
}

/// The bar along the bottom while an animation exports.
fn export_progress(s: &Editor, cx: &Cx<Editor>) -> Option<El> {
    let export = s.export.as_ref()?;
    let t = tok::theme(cx.system_dark());
    let accent = super::live::rgba(super::live::ACCENT);
    let track = 220.;
    Some(
        row()
            .key("export-progress")
            .role("progressbar")
            .label(format!("Exporting {}", export.format.label()))
            .range_value(export.percent as f64, 0., 100.)
            .w_full()
            .h(34.)
            .px(12.)
            .gap(10.)
            .center_y()
            .fill(t.card)
            .child(
                text(format!(
                    "Exporting {}… {}%",
                    export.format.label(),
                    export.percent
                ))
                .key("export-percent")
                .font(tok::type_::LABEL)
                .color(t.text),
            )
            .child(
                el().w(track)
                    .h(4.)
                    .radius(2.)
                    .fill(t.control_track)
                    .child(
                        el().w(track * export.percent as f32 / 100.)
                            .h(4.)
                            .radius(2.)
                            .fill(accent),
                    )
                    .inert(),
            )
            .child(super::panels::controls::button(
                cx,
                "export-cancel",
                "Cancel export",
                true,
                |s: &mut Editor, tx| s.dispatch_ui(Action::CancelExport, tx),
            )),
    )
}

pub fn view(s: &Editor, cx: &mut Cx<Editor>) -> El {
    let t = tok::theme(cx.system_dark());
    // Overlays the sidebar's controls open, mounted over the whole window.
    let mut layer = Vec::new();
    let root = col()
        .key("editor")
        .instant_changes()
        .w_full()
        .h_full()
        .fill(t.bg)
        .accepts_extensions(&["png", "jpg", "jpeg"])
        .on_drop_files(|s: &mut Editor, paths, tx| {
            if let Some(path) = paths.first() {
                s.dispatch_ui(Action::OpenPath(path.clone()), tx);
            }
        })
        .on_drop_rejected(|s: &mut Editor, _, _| {
            s.status = "Drop a PNG or JPEG image.".into();
        })
        .on_key_up(|s: &mut Editor, event, _| s.on_key_up(event))
        .children(s.state_probe())
        .child(toolbar(s, cx))
        .child(
            row()
                .w_full()
                .grow(1.)
                .min_h(0.)
                .child(s.canvas_view(cx))
                .child(s.sidebar(cx, &mut layer)),
        )
        .children(export_progress(s, cx));
    // Always there, so a message never moves the canvas under the pointer.
    let root = root.child(
        row()
            .key("status-bar")
            .w_full()
            .h(26.)
            .px(12.)
            .center_y()
            .fill(t.card)
            .child(
                text(s.status.clone())
                    .key("status")
                    .font(tok::type_::LABEL)
                    .color(t.text_secondary)
                    .truncate()
                    .live_region(rune::LiveRegion::Polite),
            ),
    );
    rune_kit::root(root, layer)
}
