//! The window: rune-kit's shell, its toolbar over a card of the canvas and sidebar.
use super::{
    CopyFeedback, Editor,
    actions::{Action, AnimationFormat},
    icons,
};
use crate::document::Tool;
use rune::{Cx, El, Icon, PopupItem, PopupMenu, col, el, row, text};
use rune_kit::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit, SplitView, Toolbar,
};

/// The tools, in the toolbar's order: key, icon, name and shortcut.
const TOOLS: [(Tool, &str, &Icon, &str, &str); 11] = [
    (
        Tool::Select,
        "tool-select",
        &icons::MOUSE_POINTER_2,
        "Select",
        "V",
    ),
    (Tool::Pen, "tool-pen", &icons::PEN_LINE, "Draw", "P"),
    (
        Tool::Arrow,
        "tool-arrow",
        &icons::ARROW_UP_RIGHT,
        "Arrow",
        "A",
    ),
    (
        Tool::Rectangle,
        "tool-rectangle",
        &icons::SQUARE,
        "Box",
        "R",
    ),
    (Tool::Text, "tool-text", &icons::TYPE, "Text", "T"),
    (
        Tool::Highlight,
        "tool-highlight",
        &icons::HIGHLIGHTER,
        "Highlight",
        "H",
    ),
    (
        Tool::Pixelate,
        "tool-pixelate",
        &icons::GRID_2X2,
        "Pixelate",
        "B",
    ),
    (Tool::Crop, "tool-crop", &icons::CROP, "Crop", "X"),
    (
        Tool::Counter,
        "tool-counter",
        &icons::LIST_ORDERED,
        "Step",
        "N",
    ),
    (
        Tool::Spotlight,
        "tool-spotlight",
        &icons::SCAN,
        "Spotlight",
        "S",
    ),
    (
        Tool::Magnifier,
        "tool-magnifier",
        &icons::SEARCH,
        "Magnifier",
        "M",
    ),
];

/// A toolbar button: a ghost icon button named `label`. `toggle` makes it a toggle,
/// on or off, that a screen reader reads as pressed or not.
fn tool_button(
    k: &Kit,
    key: &str,
    glyph: &'static Icon,
    label: &str,
    shortcut: Option<&str>,
    toggle: Option<bool>,
    action: Action,
) -> El {
    let mut button = IconButton::new(glyph)
        .variant(IconButtonVariant::Ghost)
        .label(label)
        .on_press_tx(move |s: &mut Editor, tx| s.dispatch_ui(action.clone(), tx));
    if let Some(shortcut) = shortcut {
        button = button.shortcut(shortcut);
    }
    if let Some(on) = toggle {
        button = button.selected(on);
    }
    button.build(k).key(key.to_string())
}

fn separator(k: &Kit) -> El {
    el().w(1.).h(20.).mx(4.).fill(k.c.border_base).a11y_hidden()
}

/// The toolbar. A press on its buttons leaves focus where it was, as a macOS toolbar
/// does, so the canvas's keys (tools, Escape, Delete) keep working after a click.
fn toolbar(s: &Editor, k: &Kit) -> Toolbar {
    let action = |key, glyph, label, shortcut, action| {
        tool_button(k, key, glyph, label, shortcut, None, action)
    };
    let toggle =
        |key, glyph, label, on, action| tool_button(k, key, glyph, label, None, Some(on), action);
    let mut tools = row()
        .key("tools")
        .preserve_focus_on_press(true)
        .gap(2.)
        .center_y()
        .child(action(
            "open",
            &icons::FOLDER_OPEN,
            "Open image",
            Some("⌘O"),
            Action::OpenImage,
        ))
        .child(separator(k));
    for (tool, key, glyph, label, shortcut) in TOOLS {
        tools = tools.child(tool_button(
            k,
            key,
            glyph,
            label,
            Some(shortcut),
            Some(s.tool == tool),
            Action::SelectTool(tool),
        ));
    }
    tools = tools
        .child(separator(k))
        .child(toggle(
            "backdrop-button",
            &icons::WALLPAPER,
            "Backdrop",
            s.panel == super::Panel::Backdrop,
            Action::ToggleBackdrop,
        ))
        .child(toggle(
            "animation-button",
            &icons::PLAY,
            "Animation",
            s.panel == super::Panel::Animation,
            Action::ToggleAnimationPanel,
        ))
        .child(toggle(
            "image-tools-button",
            &icons::IMAGE_UPSCALE,
            "Image tools",
            s.panel == super::Panel::Image,
            Action::ToggleImageTools,
        ));
    let feedback = s.copy_feedback.map(|feedback| match feedback {
        CopyFeedback::Copied => "Copied!",
        CopyFeedback::ReadingText => "Reading text…",
        CopyFeedback::TextCopied => "Text copied!",
        CopyFeedback::NoText => "No text found",
    });
    let actions = row()
        .key("actions")
        .preserve_focus_on_press(true)
        .gap(2.)
        .center_y()
        .child(action(
            "undo",
            &icons::UNDO_2,
            "Undo",
            Some("⌘Z"),
            Action::Undo,
        ))
        .child(action(
            "redo",
            &icons::REDO_2,
            "Redo",
            Some("⇧⌘Z"),
            Action::Redo,
        ))
        .child(separator(k))
        .child(
            row()
                .key("copy-group")
                .gap(2.)
                .center_y()
                .child(action(
                    "copy",
                    &icons::COPY,
                    "Copy image",
                    Some("⌘C"),
                    Action::CopyImage,
                ))
                .child(action(
                    "copy-ocr",
                    &icons::SCAN_TEXT,
                    "Copy as OCR",
                    None,
                    Action::CopyOcr,
                ))
                // Floats under the copy buttons, so it never moves the toolbar.
                .children(feedback.map(|feedback| {
                    let pill = text(feedback)
                        .key("copy-feedback")
                        .font(k.ty.small_medium)
                        .color(k.c.text_base)
                        .px(8.)
                        .py(4.)
                        .radius(k.radius.md)
                        .fill(k.c.layer_01)
                        .border(1., k.c.border_muted)
                        .live_region(rune::LiveRegion::Polite);
                    k.elevate(pill, &k.elevation.floating)
                        .absolute(0., 30.)
                        .raise()
                })),
        )
        .child(action(
            "save",
            &icons::SAVE,
            "Save PNG",
            Some("⌘S"),
            Action::SaveImage,
        ))
        .child(export_menu(s, k));
    let mut bar = Toolbar::new();
    if s.still.is_some() {
        bar = bar.leading(
            Button::new("Back")
                .icon(&icons::CHEVRON_LEFT)
                .variant(ButtonVariant::Neutral)
                .help("Back to the capture")
                .on_press_tx(|s: &mut Editor, tx| s.dispatch_ui(Action::BackToCapture, tx))
                .build(k)
                .key("back")
                .preserve_focus_on_press(true),
        );
    }
    bar = bar.leading(tools).trailing(actions);
    if s.still.is_some() {
        bar = bar.trailing(
            Button::new("Send")
                .variant(ButtonVariant::Submit)
                .shortcut("↩")
                .on_press_tx(|s: &mut Editor, tx| s.dispatch_ui(Action::SendCapture, tx))
                .build(k)
                .key("send-capture")
                .preserve_focus_on_press(true),
        );
    }
    bar
}

/// Zoom out, the zoom that fits the window, and zoom in, at the status line's end.
fn zoom_controls(s: &Editor, k: &Kit) -> El {
    let zoom = if s.doc.is_some() {
        format!("{:.0}%", (s.layout().scale * 100.).round())
    } else {
        "—".into()
    };
    let step = |key: &str, glyph, label, shortcut, factor| {
        IconButton::new(glyph)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .label(label)
            .shortcut(shortcut)
            .on_press_tx(move |s: &mut Editor, tx| s.dispatch_ui(Action::Zoom(factor), tx))
            .build(k)
            .key(key.to_string())
    };
    row()
        .key("zoom")
        .preserve_focus_on_press(true)
        .gap(2.)
        .center_y()
        .child(step("zoom-out", &icons::ZOOM_OUT, "Zoom out", "⌘−", 0.8))
        .child(
            Button::new(zoom.clone())
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .help("Fit to window")
                .shortcut("⌘1")
                .on_press_tx(|s: &mut Editor, tx| s.dispatch_ui(Action::Fit, tx))
                .build(k)
                .key("zoom-label")
                .label("Fit to window")
                .a11y_value(zoom)
                .min_w(48.),
        )
        .child(step("zoom-in", &icons::ZOOM_IN, "Zoom in", "⌘=", 1.25))
}

/// Export PNG, MP4 or GIF, or cancel the export running.
fn export_menu(s: &Editor, k: &Kit) -> El {
    let exporting = s.export.is_some();
    IconButton::new(&icons::VIDEO)
        .variant(IconButtonVariant::Ghost)
        .label("Export")
        .help("Export PNG / MP4 / GIF • ⌘S saves PNG")
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
        .build(k)
        .key("export")
}

/// The bar along the bottom while an animation exports.
fn export_progress(s: &Editor, cx: &Cx<Editor>, k: &Kit) -> Option<El> {
    let export = s.export.as_ref()?;
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
            .fill(k.c.bg_base)
            .border_sides([1., 0., 0., 0.], k.c.border_muted)
            .child(
                text(format!(
                    "Exporting {}… {}%",
                    export.format.label(),
                    export.percent
                ))
                .key("export-percent")
                .font(k.ty.ui)
                .color(k.c.text_base),
            )
            .child(
                el().w(track)
                    .h(4.)
                    .radius(2.)
                    .fill(k.c.border_base)
                    .child(
                        el().w(track * export.percent as f32 / 100.)
                            .h(4.)
                            .radius(2.)
                            .fill(k.c.bg_accent),
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

/// The window: the toolbar across the top, over a card of the canvas and the sidebar,
/// with the status line along its bottom.
pub fn view(s: &Editor, cx: &mut Cx<Editor>) -> El {
    let k = crate::kit::kit(cx.system_dark());
    // Overlays the sidebar's controls open, mounted over the whole window.
    let mut layer = Vec::new();
    let main = col()
        .key("editor")
        .instant_changes()
        .w_full()
        .h_full()
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
        .child(
            row()
                .w_full()
                .grow(1.)
                .min_h(0.)
                .child(s.canvas_view(cx))
                .child(s.sidebar(cx, &mut layer)),
        )
        .children(export_progress(s, cx, k))
        // Always there, so a message never moves the canvas under the pointer.
        .child(
            row()
                .key("status-bar")
                .w_full()
                .h(26.)
                .pl(12.)
                .pr(4.)
                .gap(8.)
                .center_y()
                .fill(k.c.bg_base)
                .border_sides([1., 0., 0., 0.], k.c.border_muted)
                .child(
                    text(s.status.clone())
                        .key("status")
                        .font(k.ty.small)
                        .color(k.c.text_muted)
                        .truncate()
                        .grow(1.)
                        .min_w(0.)
                        .live_region(rune::LiveRegion::Polite),
                )
                .child(zoom_controls(s, k)),
        );
    let frame = SplitView::new(main)
        .toolbar(toolbar(s, k))
        .titlebar_inset(cx.titlebar_inset())
        .frame_inset(0.)
        .build(k);
    rune_kit::root(frame, layer)
}
