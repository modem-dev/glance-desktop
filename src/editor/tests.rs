//! Tests use GPUI's virtual platform. No desktop interaction or capture permission.
use super::feedback::CopyFeedback;
use super::{Document, Editor, Layout, Message, Tool, render_image};
use gpui::{
    Bounds, EntityInputHandler, KeyDownEvent, Keystroke, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, TestAppContext, WindowHandle, point, px, size,
};
use std::sync::Arc;
fn editor(cx: &mut TestAppContext) -> WindowHandle<Editor> {
    cx.add_window(|window, cx| {
        let mut e = Editor::with_native(cx, false);
        e.document = Document::new(image::RgbaImage::from_pixel(
            100,
            100,
            image::Rgba([0, 0, 0, 255]),
        ));
        e.preview.image = render_image((*e.document.base).clone());
        e.viewport.layout.set(Layout {
            x: 0.,
            y: 0.,
            scale: 1.,
            width: 100.,
            height: 100.,
        });
        e.viewport
            .canvas_bounds
            .set(Bounds::new(point(px(0.), px(0.)), size(px(100.), px(100.))));
        e.focus.focus(window);
        e
    })
}
fn reset_layout(e: &Editor) {
    e.viewport.layout.set(Layout {
        x: 0.,
        y: 0.,
        scale: 1.,
        width: 100.,
        height: 100.,
    });
    e.viewport
        .canvas_bounds
        .set(Bounds::new(point(px(0.), px(0.)), size(px(100.), px(100.))));
}
fn down(x: f32, y: f32) -> MouseDownEvent {
    MouseDownEvent {
        position: point(px(x), px(y)),
        button: MouseButton::Left,
        modifiers: Default::default(),
        click_count: 1,
        first_mouse: false,
    }
}
fn up(x: f32, y: f32) -> MouseUpEvent {
    MouseUpEvent {
        position: point(px(x), px(y)),
        button: MouseButton::Left,
        modifiers: Default::default(),
        click_count: 1,
    }
}
fn motion(x: f32, y: f32) -> MouseMoveEvent {
    MouseMoveEvent {
        position: point(px(x), px(y)),
        pressed_button: Some(MouseButton::Left),
        modifiers: Default::default(),
    }
}
fn key(k: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke::parse(k).unwrap(),
        is_held: false,
    }
}

#[gpui::test]
fn animation_timing_labels_do_not_overlap_at_minimum_size(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, _, cx| {
        e.dispatch_ui(super::actions::Action::ToggleAnimationPanel, cx)
    })
    .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(*view, cx);
    visual.simulate_resize(size(px(1050.), px(600.)));
    visual.run_until_parked();
    for (label, title, value) in [
        (
            "Entrance duration",
            "animation-label-Entrance duration",
            "animation-value-Entrance duration",
        ),
        ("Delay", "animation-label-Delay", "animation-value-Delay"),
    ] {
        let title = visual.debug_bounds(title).unwrap();
        let value = visual.debug_bounds(value).unwrap();
        assert!(title.bottom() <= value.top(), "{label} overlaps its value");
        assert!(title.right() <= px(1050.) && value.right() <= px(1050.));
    }
    let duration = visual
        .debug_bounds("animation-label-Entrance duration")
        .unwrap();
    let delay = visual.debug_bounds("animation-label-Delay").unwrap();
    assert!(duration.right() < delay.left());
}

#[gpui::test]
fn accessible_controls_dispatch_edit_undo_and_reject_stale_targets(cx: &mut TestAppContext) {
    use super::actions::Action;
    use crate::accessibility::Request;
    let view = editor(cx);
    let mut visual = gpui::VisualTestContext::from_window(*view, cx);
    visual.simulate_resize(size(px(1050.), px(600.)));
    visual.run_until_parked();
    let nodes = view
        .read_with(&visual, |e, _| e.accessibility.nodes())
        .unwrap();
    assert!(
        nodes
            .iter()
            .any(|n| n.label.starts_with("Screenshot canvas"))
    );
    let arrow = nodes
        .iter()
        .find(|n| n.label == Tool::Arrow.label())
        .unwrap();
    assert!(arrow.invoke(Request::Press));
    visual.run_until_parked();
    assert_eq!(
        view.read_with(&visual, |e, _| e.interaction.tool).unwrap(),
        Tool::Arrow
    );
    let width = view
        .read_with(&visual, |e, _| {
            e.accessibility
                .nodes()
                .into_iter()
                .find(|n| n.label == "Thickness")
                .unwrap()
        })
        .unwrap();
    assert!(!width.invoke(Request::SetValue(f64::NAN)));
    assert!(!width.invoke(Request::SetValue(99999.)));
    assert!(width.invoke(Request::SetValue(12.)));
    visual.run_until_parked();
    assert_eq!(
        view.read_with(&visual, |e, _| e.interaction.width).unwrap(),
        12.
    );
    let color = view
        .read_with(&visual, |e, _| {
            e.accessibility
                .nodes()
                .into_iter()
                .find(|n| n.label == "Custom color hex")
                .unwrap()
        })
        .unwrap();
    color.invoke(Request::SetText("#abc".into()));
    visual.run_until_parked();
    assert_eq!(
        view.read_with(&visual, |e, _| e.interaction.color).unwrap(),
        [170, 187, 204, 255]
    );

    view.update(&mut visual, |e, w, cx| {
        reset_layout(e);
        e.begin(&down(10., 10.), w, cx);
        e.finish(&up(70., 30.), cx);
    })
    .unwrap();
    visual.run_until_parked();
    let selected_width = view
        .read_with(&visual, |e, _| {
            e.accessibility
                .nodes()
                .into_iter()
                .find(|n| n.label == "Thickness")
                .unwrap()
        })
        .unwrap();
    assert!(selected_width.invoke(Request::SetValue(8.)));
    visual.run_until_parked();
    assert_eq!(
        view.read_with(&visual, |e, _| e.document.marks[0].width)
            .unwrap(),
        8.
    );
    view.update(&mut visual, |e, _, cx| e.dispatch_ui(Action::Undo, cx))
        .unwrap();
    visual.run_until_parked();
    assert_eq!(
        view.read_with(&visual, |e, _| e.document.marks[0].width)
            .unwrap(),
        12.
    );
    // A queued native action from the previous revision must not modify the mark.
    selected_width.invoke(Request::SetValue(9.));
    visual.run_until_parked();
    assert_eq!(
        view.read_with(&visual, |e, _| e.document.marks[0].width)
            .unwrap(),
        12.
    );
    view.update(&mut visual, |e, _, cx| {
        e.receive(
            Message::Preview(
                e.preview.revision,
                e.document.marks.len(),
                0,
                render_image(e.document.render(None)),
            ),
            cx,
        );
        e.dispatch_ui(Action::ToggleAnimationPanel, cx)
    })
    .unwrap();
    visual.run_until_parked();
    let nodes = view
        .read_with(&visual, |e, _| e.accessibility.nodes())
        .unwrap();
    let duration = nodes
        .iter()
        .find(|n| n.label == "Entrance duration")
        .unwrap_or_else(|| {
            panic!(
                "nodes: {:?}",
                nodes.iter().map(|n| &n.label).collect::<Vec<_>>()
            )
        });
    assert_eq!(duration.value, Some(1.));
    duration.invoke(Request::SetValue(1.5));
    visual.run_until_parked();
    assert_eq!(
        view.read_with(&visual, |e, _| e.document.image_animation.duration_ms)
            .unwrap(),
        1500
    );
    assert!(
        nodes
            .iter()
            .all(|n| n.bounds.top() >= px(0.) && n.bounds.bottom() <= px(600.))
    );
}
#[gpui::test]
fn remote_copy_toolbar_fits_at_minimum_window_width(cx: &mut TestAppContext) {
    let view = editor(cx);
    let mut visual = gpui::VisualTestContext::from_window(*view, cx);
    visual.simulate_resize(size(px(1050.), px(600.)));
    visual.run_until_parked();
    for selector in ["copy-remote", "header-zoom"] {
        let bounds = visual.debug_bounds(selector).unwrap();
        assert!(bounds.size.width > px(0.));
        assert!(bounds.origin.x >= px(0.));
        assert!(
            bounds.right() <= px(1050.),
            "{selector} overflows: {bounds:?}"
        );
    }
}
#[gpui::test]
fn remote_copy_updates_clipboard_only_after_upload_success(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, _, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("existing clipboard".into()));
        e.start_operation(super::jobs::OperationKind::Upload)
            .unwrap();
        e.copy_remote(cx); // A second request must not start another upload.
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "existing clipboard"
        );
        complete(
            e,
            super::jobs::OperationResult::RemoteCopied(Ok(crate::glance::Share {
                url: "https://glance.sh/example.png".into(),
                expires_at: u64::MAX,
            })),
            cx,
        );
        assert!(!e.is_busy());
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "Screenshot: https://glance.sh/example.png"
        );
        assert!(e.feedback.status.contains("Glance link copied"));
        assert!(matches!(e.feedback.copy, Some(CopyFeedback::LinkCopied(_))));
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("keep on failure".into()));
        e.start_operation(super::jobs::OperationKind::Upload)
            .unwrap();
        complete(
            e,
            super::jobs::OperationResult::RemoteCopied(Err("Offline".into())),
            cx,
        );
        assert!(!e.is_busy());
        assert_eq!(e.feedback.status, "Offline");
        assert_eq!(e.feedback.copy, None);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "keep on failure"
        );
    })
    .unwrap();
}
#[gpui::test]
fn copy_confirmation_expires_without_dismissing_a_new_upload(cx: &mut TestAppContext) {
    use std::time::Duration;
    let view = editor(cx);
    view.update(cx, |e, _, cx| {
        complete(e, super::jobs::OperationResult::Copied(Ok(())), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    view.update(cx, |e, _, cx| {
        assert_eq!(e.feedback.copy, Some(CopyFeedback::Copied));
        e.set_copy_feedback(Some(CopyFeedback::Uploading), cx);
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    view.update(cx, |e, _, cx| {
        assert_eq!(e.feedback.copy, Some(CopyFeedback::Uploading));
        complete(
            e,
            super::jobs::OperationResult::RemoteCopied(Ok(crate::glance::Share {
                url: "https://glance.sh/example.png".into(),
                expires_at: u64::MAX,
            })),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    view.update(cx, |e, _, _| assert_eq!(e.feedback.copy, None))
        .unwrap();
}
#[gpui::test]
fn copy_confirmation_fits_and_refreshes_on_repeated_copy(cx: &mut TestAppContext) {
    use std::time::Duration;
    let view = editor(cx);
    view.update(cx, |e, _, cx| {
        complete(e, super::jobs::OperationResult::Copied(Ok(())), cx)
    })
    .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(*view, cx);
    visual.simulate_resize(size(px(1050.), px(600.)));
    visual.run_until_parked();
    let bounds = visual.debug_bounds("copy-feedback").unwrap();
    assert!(bounds.origin.x >= px(0.));
    assert!(bounds.origin.y >= px(48.));
    assert!(bounds.right() <= px(1050.));
    cx.executor().advance_clock(Duration::from_secs(1));
    view.update(cx, |e, _, cx| {
        complete(e, super::jobs::OperationResult::Copied(Ok(())), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    view.update(cx, |e, _, _| {
        assert_eq!(e.feedback.copy, Some(CopyFeedback::Copied))
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    view.update(cx, |e, _, cx| {
        assert_eq!(e.feedback.copy, None);
        complete(
            e,
            super::jobs::OperationResult::Copied(Err("Clipboard unavailable".into())),
            cx,
        );
        assert_eq!(e.feedback.copy, None);
    })
    .unwrap();
}
#[gpui::test]
fn drawing_pick_move_duplicate_delete_and_undo(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        e.set_tool(Tool::Pen, cx);
        e.begin(&down(10., 10.), w, cx);
        e.motion(&motion(30., 30.), cx);
        e.finish(&up(40., 40.), cx);
        assert_eq!(e.document.marks.len(), 1);
        e.set_tool(Tool::Select, cx);
        e.begin(&down(20., 20.), w, cx);
        assert_eq!(e.interaction.selected, Some(0));
        e.motion(&motion(40., 30.), cx);
        assert_eq!(e.document.marks[0].points[0], (10., 10.));
        e.finish(&up(40., 30.), cx);
        assert_eq!(e.document.marks[0].points[0], (30., 20.));
        e.duplicate_selected(cx);
        assert_eq!(e.document.marks.len(), 2);
        assert_eq!(e.interaction.selected, Some(1));
        e.delete_selected(cx);
        assert_eq!(e.document.marks.len(), 1);
        e.document.undo();
        assert_eq!(e.document.marks.len(), 2);
        e.document.undo();
        assert_eq!(e.document.marks.len(), 1);
        e.document.undo();
        assert_eq!(e.document.marks[0].points[0], (10., 10.));
    })
    .unwrap();
}
#[gpui::test]
fn cancel_gesture_and_shift_release_preserve_document(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        e.set_tool(Tool::Rectangle, cx);
        e.begin(&down(10., 10.), w, cx);
        let mut release = up(50., 35.);
        release.modifiers.shift = true;
        e.finish(&release, cx);
        assert_eq!(e.document.marks[0].points.last(), Some(&(50., 50.)));
        e.set_tool(Tool::Select, cx);
        e.begin(&down(10., 30.), w, cx);
        e.motion(&motion(30., 40.), cx);
        e.key(&key("escape"), w, cx);
        e.finish(&up(30., 40.), cx);
        assert_eq!(e.document.marks[0].points[0], (10., 10.));
        assert!(e.interaction.selected.is_none());
    })
    .unwrap();
}
#[gpui::test]
fn space_pan_and_zoom_do_not_add_marks(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        e.set_tool(Tool::Pen, cx);
        e.key(&key("space"), w, cx);
        e.begin(&down(10., 10.), w, cx);
        e.motion(&motion(30., 40.), cx);
        e.finish(&up(30., 40.), cx);
        assert_eq!(e.viewport.pan, (20., 30.));
        assert!(e.document.marks.is_empty());
        e.viewport.space_down = false;
        e.receive(Message::Magnify(0.5, (30., 40.), false), cx);
        assert_eq!(e.viewport.zoom, Some(1.5));
        assert!(e.document.marks.is_empty());
        e.key(&key("cmd-1"), w, cx);
        assert!(e.viewport.zoom.is_none());
        assert_eq!(e.viewport.pan, (0., 0.));
        e.viewport.zoom_down = true;
        e.begin(&down(20., 20.), w, cx);
        assert_eq!(e.viewport.zoom, Some(2.));
    })
    .unwrap();
}
#[gpui::test]
fn text_typing_unicode_editing_commit_and_cancel(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        e.set_tool(Tool::Text, cx);
        e.begin(&down(10., 10.), w, cx);
        e.replace_text_in_range(None, "Hi 👨‍👩‍👧‍👦 café 日本語", w, cx);
        e.key(&key("backspace"), w, cx);
        e.key(&key("cmd-z"), w, cx);
        assert_eq!(
            e.interaction.text_edit.as_ref().unwrap().buffer.text(),
            "Hi 👨‍👩‍👧‍👦 café 日本語"
        );
        e.key(&key("enter"), w, cx);
        assert_eq!(e.document.marks.len(), 1);
        e.begin(&down(50., 50.), w, cx);
        e.replace_text_in_range(None, "discard", w, cx);
        e.key(&key("escape"), w, cx);
        assert_eq!(e.document.marks.len(), 1);
        assert!(e.interaction.text_edit.is_none());
    })
    .unwrap();
}
#[gpui::test]
fn counter_numbers_remain_unique_after_deleting_a_step(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        e.set_tool(Tool::Counter, cx);
        e.begin(&down(10., 10.), w, cx);
        e.begin(&down(40., 40.), w, cx);
        e.document.delete_mark(0);
        e.begin(&down(70., 70.), w, cx);
        let labels: Vec<_> = e.document.marks.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(labels, vec!["2", "3"]);
    })
    .unwrap();
}
#[gpui::test]
fn stale_previews_cannot_overwrite_new_edits(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, _, cx| {
        let old = e.preview.image.clone();
        e.preview.revision = 5;
        e.receive(
            Message::Preview(4, 99, 40, render_image(image::RgbaImage::new(2, 2))),
            cx,
        );
        assert!(Arc::ptr_eq(&e.preview.image, &old));
        assert_ne!(e.preview.mark_count, 99);
        assert_eq!(e.preview.inside_padding, 0);
        e.receive(
            Message::Preview(5, 0, 12, render_image(image::RgbaImage::new(100, 100))),
            cx,
        );
        assert!(!Arc::ptr_eq(&e.preview.image, &old));
        assert_eq!(e.preview.mark_count, 0);
        assert_eq!(e.preview.inside_padding, 12);
    })
    .unwrap();
}
#[gpui::test]
fn real_event_dispatch_draws_and_commits_text_without_double_actions(cx: &mut TestAppContext) {
    let window = editor(cx);
    let view = window.root(cx).unwrap();
    let mut visual = gpui::VisualTestContext::from_window(*window, cx);
    visual.update(|_, cx| crate::menus::install(cx));
    visual.simulate_keystrokes("p");
    assert_eq!(
        view.read_with(&visual, |e, _| e.interaction.tool),
        Tool::Pen
    );
    let l = view.read_with(&visual, |e, _| e.viewport.layout.get());
    let pos = |x, y| point(px(l.x + x * l.scale), px(l.y + y * l.scale));
    visual.simulate_mouse_down(pos(10., 10.), MouseButton::Left, Default::default());
    visual.simulate_mouse_move(pos(40., 40.), MouseButton::Left, Default::default());
    visual.simulate_mouse_up(pos(40., 40.), MouseButton::Left, Default::default());
    assert_eq!(view.read_with(&visual, |e, _| e.document.marks.len()), 1);
    visual.simulate_keystrokes("t");
    visual.simulate_mouse_down(pos(50., 50.), MouseButton::Left, Default::default());
    visual.simulate_mouse_up(pos(50., 50.), MouseButton::Left, Default::default());
    assert_eq!(
        view.read_with(&visual, |e, _| e.interaction.tool),
        Tool::Text
    );
    assert!(
        view.read_with(&visual, |e, _| e.interaction.text_edit.is_some()),
        "text click must open canvas editor"
    );
    visual.simulate_input("p a r t h b x n v z hello café 日本語");
    assert_eq!(
        view.read_with(&visual, |e, _| e
            .interaction
            .text_edit
            .as_ref()
            .unwrap()
            .buffer
            .text()
            .to_owned()),
        "p a r t h b x n v z hello café 日本語"
    );
    visual.simulate_keystrokes("enter");
    assert_eq!(view.read_with(&visual, |e, _| e.document.marks.len()), 2);
    assert_eq!(
        view.read_with(&visual, |e, _| e.document.marks[1].text.clone()),
        "p a r t h b x n v z hello café 日本語"
    );
    visual.simulate_keystrokes("cmd-z");
    assert_eq!(
        view.read_with(&visual, |e, _| e.document.marks.len()),
        1,
        "one cmd-z must undo once despite a native menu binding"
    );
}
#[gpui::test]
fn ime_preedit_replacement_and_commit_use_utf16_ranges(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        e.set_tool(Tool::Text, cx);
        e.begin(&down(10., 10.), w, cx);
        e.replace_text_in_range(None, "👋 ", w, cx);
        e.replace_and_mark_text_in_range(None, "に", Some(1..1), w, cx);
        e.replace_and_mark_text_in_range(None, "日本", Some(2..2), w, cx);
        assert_eq!(
            e.interaction.text_edit.as_ref().unwrap().buffer.text(),
            "👋 日本"
        );
        assert_eq!(e.marked_text_range(w, cx), Some(3..5));
        assert_eq!(e.selected_text_range(false, w, cx).unwrap().range, 5..5);
        e.key(&key("enter"), w, cx);
        assert!(
            e.interaction.text_edit.is_some(),
            "Enter during composition must be handled by the IME"
        );
        e.unmark_text(w, cx);
        e.key(&key("enter"), w, cx);
        assert_eq!(e.document.marks[0].text, "👋 日本");
    })
    .unwrap();
}
#[gpui::test]
fn selected_style_and_held_nudges_are_undoable(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        e.set_tool(Tool::Arrow, cx);
        e.begin(&down(10., 10.), w, cx);
        e.finish(&up(70., 70.), cx);
        e.interaction.selected = Some(0);
        let color = e.document.marks[0].color;
        e.interaction.color = [0, 120, 255, 255];
        e.apply_style(true, cx);
        assert_eq!(e.document.marks[0].color, e.interaction.color);
        e.document.undo();
        assert_eq!(e.document.marks[0].color, color);
        e.key(&key("right"), w, cx);
        let mut repeat = key("right");
        repeat.is_held = true;
        for _ in 0..20 {
            e.key(&repeat, w, cx);
        }
        assert_eq!(e.document.marks[0].points[0], (31., 10.));
        e.document.undo();
        assert_eq!(e.document.marks[0].points[0], (10., 10.));
    })
    .unwrap();
}

#[gpui::test]
fn arrow_handles_edit_independently_and_new_marks_stay_selected(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        e.set_tool(Tool::Arrow, cx);
        e.begin(&down(10., 50.), w, cx);
        e.motion(&motion(90., 50.), cx);
        e.finish(&up(90., 50.), cx);
        assert_eq!(e.interaction.selected, Some(0));
        assert_eq!(e.interaction.tool, Tool::Arrow);
        assert_eq!(e.document.marks[0].points, vec![(10., 50.), (90., 50.)]);
        e.begin(&down(90., 50.), w, cx);
        e.motion(&motion(85., 70.), cx);
        e.finish(&up(85., 70.), cx);
        assert_eq!(e.document.marks[0].points, vec![(10., 50.), (85., 70.)]);
        e.begin(&down(10., 50.), w, cx);
        e.finish(&up(15., 60.), cx);
        assert_eq!(e.document.marks[0].points, vec![(15., 60.), (85., 70.)]);
        let mid = crate::arrow::at(&e.document.marks[0], 0.5);
        e.begin(&down(mid.0, mid.1), w, cx);
        e.motion(&motion(50., 25.), cx);
        e.finish(&up(50., 25.), cx);
        let m = &e.document.marks[0];
        assert_eq!(m.points, vec![(15., 60.), (85., 70.)]);
        assert_eq!(crate::arrow::at(m, 0.5), (50., 25.));
        assert!(m.hit((50., 25.), 1.));
        assert!(!m.hit((50., 65.), 1.));
        let curved = m.clone();
        e.begin(&down(85., 70.), w, cx);
        e.motion(&motion(95., 80.), cx);
        e.key(&key("escape"), w, cx);
        assert_eq!(e.document.marks[0].points, curved.points);
        e.document.undo();
        assert!(e.document.marks[0].curve.is_none());
        e.document.redo();
        assert_eq!(e.document.marks[0].curve, curved.curve);
        e.interaction.selected = Some(0);
        // Move the shaft while the Arrow tool remains active, away from the handles.
        let p = crate::arrow::at(&curved, 0.25);
        e.begin(&down(p.0, p.1), w, cx);
        e.finish(&up(p.0 + 5., p.1 + 5.), cx);
        assert_eq!(e.document.marks[0].points[0], (20., 65.));
        assert_eq!(crate::arrow::at(&e.document.marks[0], 0.5), (55., 30.));
        e.key(&key("backspace"), w, cx);
        assert!(e.document.marks.is_empty());
        e.receive(
            Message::Preview(
                e.preview.revision,
                0,
                0,
                render_image((*e.document.base).clone()),
            ),
            cx,
        );
        e.set_tool(Tool::Rectangle, cx);
        e.begin(&down(10., 10.), w, cx);
        e.finish(&up(40., 40.), cx);
        assert_eq!(e.interaction.selected, Some(0));
        e.key(&key("backspace"), w, cx);
        assert!(e.document.marks.is_empty());
    })
    .unwrap();
}

#[gpui::test]
fn motion_controls_pause_duration_and_history(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        e.backdrop_style(
            |b| {
                b.motion = crate::animation::Motion::Lava;
                b.seconds = 5;
            },
            cx,
        );
        let base = e.document.base.clone();
        e.playback.paused = true;
        e.playback.position = 1.25;
        assert!((e.animation_phase() - 0.25).abs() < 0.001);
        let track = Bounds::new(point(px(0.), px(0.)), size(px(130.), px(24.)));
        e.interaction.gesture =
            super::Gesture::AdjustingBackdrop(crate::backdrop::Control::Duration, track);
        e.backdrop_slider_move(point(px(80.), px(12.)), cx);
        assert_eq!(e.document.backdrop.unwrap().seconds, 10);
        assert!(
            (e.animation_phase() - 0.25).abs() < 0.001,
            "duration preserves current phase"
        );
        e.backdrop_slider_move(point(px(-20.), px(12.)), cx);
        assert_eq!(e.document.backdrop.unwrap().seconds, 2);
        e.backdrop_slider_move(point(px(200.), px(12.)), cx);
        assert_eq!(e.document.backdrop.unwrap().seconds, 15);
        e.interaction.gesture = super::Gesture::Idle;
        e.toggle_animation(cx);
        assert!(!e.playback.paused);
        e.toggle_animation(cx);
        assert!(e.playback.paused);
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        e.video_export.cancel = Some(cancel.clone());
        e.key(&key("escape"), w, cx);
        assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
        e.video_export.cancel = None;
        e.document.undo();
        assert!(e.document.backdrop.is_none());
        e.document.redo();
        assert_eq!(
            e.document.backdrop.unwrap().motion,
            crate::animation::Motion::Lava
        );
        assert!(Arc::ptr_eq(&base, &e.document.base));
    })
    .unwrap();
}

#[gpui::test]
fn switching_pointer_gestures_cancels_the_previous_drag(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        e.set_tool(Tool::Rectangle, cx);
        e.begin(&down(10., 10.), w, cx);
        e.finish(&up(30., 30.), cx);
        let original = e.document.marks[0].points.clone();
        e.begin(&down(10., 10.), w, cx);
        e.motion(&motion(20., 20.), cx);
        assert!(e.interaction.gesture.drag().is_some());
        e.begin_pan(point(px(40.), px(40.)), cx);
        assert!(matches!(e.interaction.gesture, super::Gesture::Panning(_)));
        assert_eq!(e.document.marks[0].points, original);
        e.key(&key("escape"), w, cx);
        assert!(!e.interaction.gesture.is_active());
        e.set_tool(Tool::Pen, cx);
        e.begin(&down(50., 50.), w, cx);
        assert!(e.interaction.gesture.draft().is_some());
        e.key(&key("escape"), w, cx);
        e.finish(&up(60., 60.), cx);
        assert_eq!(e.document.marks.len(), 1);
    })
    .unwrap();
}

fn complete(e: &mut Editor, result: super::jobs::OperationResult, cx: &mut gpui::Context<Editor>) {
    let kind = match &result {
        super::jobs::OperationResult::RemoteCopied(_) => super::jobs::OperationKind::Upload,
        super::jobs::OperationResult::Copied(_) => super::jobs::OperationKind::Copy,
        _ => panic!("Choose the operation kind for this test result"),
    };
    let id = e
        .operations
        .active
        .as_ref()
        .map(|op| op.id)
        .unwrap_or_else(|| e.start_operation(kind).unwrap());
    e.receive(Message::Operation(id, result), cx);
}

#[gpui::test]
fn stale_operation_completion_cannot_change_clipboard_or_clear_new_job(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, _, cx| {
        use super::jobs::{OperationKind, OperationResult};
        let old = e.start_operation(OperationKind::Upload).unwrap();
        e.receive(
            Message::Operation(old, OperationResult::RemoteCopied(Err("Offline".into()))),
            cx,
        );
        let current = e.start_operation(OperationKind::Copy).unwrap();
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("keep this".into()));
        e.set_copy_feedback(Some(CopyFeedback::Copying), cx);
        e.receive(
            Message::Operation(
                old,
                OperationResult::RemoteCopied(Ok(crate::glance::Share {
                    url: "https://glance.sh/stale.png".into(),
                    expires_at: u64::MAX,
                })),
            ),
            cx,
        );
        assert_eq!(e.operations.active.as_ref().unwrap().id, current);
        assert_eq!(e.feedback.copy, Some(CopyFeedback::Copying));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "keep this"
        );
        e.receive(
            Message::Operation(current, OperationResult::Copied(Ok(()))),
            cx,
        );
        assert!(!e.is_busy());
        assert_eq!(e.feedback.copy, Some(CopyFeedback::Copied));
    })
    .unwrap();
}

#[gpui::test]
fn preview_completion_does_not_unlock_an_active_operation(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, _, cx| {
        use super::jobs::{OperationKind, OperationResult};
        let id = e.start_operation(OperationKind::Copy).unwrap();
        e.preview.waiting = true;
        e.receive(
            Message::Preview(
                e.preview.revision,
                0,
                0,
                render_image((*e.document.base).clone()),
            ),
            cx,
        );
        assert!(!e.preview.waiting);
        assert!(e.is_busy());
        assert!(e.start_operation(OperationKind::Upload).is_none());
        e.receive(Message::Operation(id, OperationResult::Copied(Ok(()))), cx);
        assert!(!e.is_busy());
        let id = e.start_operation(OperationKind::Open).unwrap();
        e.receive(
            Message::Operation(
                id,
                OperationResult::Image(Ok(Some(image::RgbaImage::new(30, 20)))),
            ),
            cx,
        );
        assert!(e.operations.active.is_none());
        assert!(e.is_busy()); // The new image must finish preparing before editing resumes.
        e.receive(
            Message::Preview(
                e.preview.revision,
                0,
                0,
                render_image((*e.document.base).clone()),
            ),
            cx,
        );
        assert!(!e.is_busy());
    })
    .unwrap();
}

#[gpui::test]
fn export_progress_stays_visible_and_cancel_uses_shared_action(cx: &mut TestAppContext) {
    use super::jobs::{OperationKind, OperationResult};
    use crate::automation;
    use std::sync::atomic::{AtomicBool, Ordering};

    let view = editor(cx);
    let cancel = Arc::new(AtomicBool::new(false));
    let id = view
        .update(cx, |e, _, cx| {
            let id = e.start_operation(OperationKind::Video).unwrap();
            e.video_export.cancel = Some(cancel.clone());
            e.receive(Message::VideoProgress(id, 42), cx);
            id
        })
        .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(*view, cx);
    visual.simulate_resize(size(px(1050.), px(600.)));
    visual.run_until_parked();
    let bar = visual.debug_bounds("export-progress").unwrap();
    let track = visual.debug_bounds("export-progress-track").unwrap();
    let fill = visual.debug_bounds("export-progress-fill").unwrap();
    assert!(bar.top() >= px(48.) && bar.bottom() <= px(600.));
    assert!(track.size.width > px(800.));
    assert!((fill.size.width / track.size.width - 0.42).abs() < 0.01);
    view.update(&mut visual, |e, _, cx| {
        assert!(!e.panels.animation && !e.panels.backdrop);
        let (reply, response) = std::sync::mpsc::channel();
        e.automation(automation::Request::State(reply), cx);
        assert_eq!(
            response.recv().unwrap().unwrap()["operation"]["progress"],
            42
        );
    })
    .unwrap();
    let button = visual.debug_bounds("export-cancel").unwrap();
    assert!(bar.contains(&button.origin) && bar.contains(&button.bottom_right()));
    visual.simulate_click(button.center(), Default::default());
    visual.run_until_parked();
    assert!(cancel.load(Ordering::Relaxed));
    assert!(visual.debug_bounds("export-progress").is_some());
    view.update(&mut visual, |e, _, cx| {
        e.receive(
            Message::Operation(id, OperationResult::VideoSaved(Ok(None))),
            cx,
        );
    })
    .unwrap();
    visual.run_until_parked();
    view.read_with(&visual, |e, _| {
        assert_eq!(e.video_export.progress, None);
        assert!(!e.is_busy());
    })
    .unwrap();
}

#[gpui::test]
fn video_progress_belongs_to_the_active_export(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, _, cx| {
        use super::jobs::{OperationKind, OperationResult};
        let old = e.start_operation(OperationKind::Video).unwrap();
        e.receive(Message::VideoProgress(old, 40), cx);
        assert_eq!(e.video_export.progress, Some(40));
        e.receive(
            Message::Operation(old, OperationResult::VideoSaved(Ok(None))),
            cx,
        );
        let current = e.start_operation(OperationKind::Video).unwrap();
        e.video_export.progress = Some(0);
        e.receive(Message::VideoProgress(old, 90), cx);
        assert_eq!(e.video_export.progress, Some(0));
        e.receive(Message::VideoProgress(current, 25), cx);
        assert_eq!(e.video_export.progress, Some(25));
        e.receive(
            Message::Operation(current, OperationResult::VideoSaved(Ok(None))),
            cx,
        );
        assert!(!e.is_busy());
        assert_eq!(e.video_export.progress, None);
    })
    .unwrap();
}

#[gpui::test]
fn automation_applies_to_native_editor_and_rejects_stale_work(cx: &mut TestAppContext) {
    use crate::automation::{Request, Snapshot};
    use serde_json::json;
    let view = editor(cx);
    view.update(cx, |e, _, cx| {
        let mut snapshot = Snapshot {
            document: e.document.clone(),
            revision: e.preview.revision,
            phase: 0.,
        };
        crate::mcp::operate(
            "add_annotation",
            &json!({"mark": {
                "tool": "arrow", "points": [[10, 10], [60, 40]],
                "color": [255, 0, 0, 255], "width": 3, "text": ""
            }}),
            &mut snapshot,
        )
        .unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        e.automation(
            Request::Apply {
                document: snapshot.document.clone(),
                revision: snapshot.revision,
                replace: false,
                reply: tx,
            },
            cx,
        );
        assert!(rx.recv().unwrap().is_ok());
        assert_eq!(e.interaction.selected, Some(0));
        assert_eq!(e.interaction.tool, Tool::Select);
        assert_eq!(e.document.marks.len(), 1);
        let (tx, rx) = std::sync::mpsc::channel();
        e.automation(
            Request::Apply {
                document: snapshot.document,
                revision: snapshot.revision,
                replace: false,
                reply: tx,
            },
            cx,
        );
        assert!(rx.recv().unwrap().is_err());
        assert_eq!(e.document.marks.len(), 1);
        e.document.undo();
        assert!(e.document.marks.is_empty());
        e.start_operation(super::jobs::OperationKind::Upload)
            .unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        e.automation(Request::Snapshot(tx), cx);
        assert!(rx.recv().unwrap().is_err());
    })
    .unwrap();
}

#[gpui::test]
fn spotlight_magnifier_creation_handles_and_native_undo(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        e.set_tool(Tool::Spotlight, cx);
        e.begin(&down(10., 10.), w, cx);
        e.motion(&motion(60., 60.), cx);
        e.finish(&up(60., 60.), cx);
        assert_eq!(e.document.marks[0].tool, Tool::Spotlight);
        assert_eq!(e.interaction.selected, Some(0));
        assert_eq!(e.document.marks[0].points.len(), 2);
        e.begin(&down(60., 60.), w, cx);
        assert_eq!(
            e.interaction.gesture.drag().and_then(|(_, handle)| handle),
            Some(2)
        );
        e.finish(&up(70., 65.), cx);
        assert_eq!(e.document.marks[0].points, vec![(10., 10.), (70., 65.)]);
        e.document.undo();
        e.set_tool(Tool::Magnifier, cx);
        e.begin(&down(20., 20.), w, cx);
        e.motion(&motion(75., 75.), cx);
        e.finish(&up(75., 75.), cx);
        assert_eq!(e.document.marks[1].points, vec![(20., 20.), (75., 75.)]);
        assert_eq!(e.interaction.selected, Some(1));
        e.begin(&down(75., 75.), w, cx);
        assert_eq!(
            e.interaction.gesture.drag().and_then(|(_, handle)| handle),
            Some(2)
        );
        e.motion(&motion(85., 70.), cx);
        e.finish(&up(85., 70.), cx);
        assert_eq!(e.document.marks[1].points, vec![(20., 20.), (85., 70.)]);
        e.document.undo();
        assert_eq!(e.document.marks[1].points[1], (75., 75.));
        e.begin(&down(20., 20.), w, cx);
        assert_eq!(
            e.interaction.gesture.drag().and_then(|(_, handle)| handle),
            Some(0)
        );
        e.finish(&up(30., 25.), cx);
        assert_eq!(e.document.marks[1].points, vec![(30., 25.), (75., 75.)]);
        e.delete_selected(cx);
        assert_eq!(e.document.marks.len(), 1);
        e.document.undo();
        assert_eq!(e.document.marks.len(), 2);
    })
    .unwrap();
}

#[gpui::test]
fn backdrop_grid_modes_and_format_menu_work_at_minimum_window_size(cx: &mut TestAppContext) {
    use crate::backdrop::Format;
    let view = editor(cx);
    view.update(cx, |e, _, cx| e.toggle_backdrop(cx)).unwrap();
    let mut visual = gpui::VisualTestContext::from_window(*view, cx);
    visual.simulate_resize(size(px(1050.), px(600.)));
    visual.run_until_parked();
    let click = |visual: &mut gpui::VisualTestContext, selector: &'static str| {
        let point = visual.debug_bounds(selector).unwrap().center();
        visual.simulate_mouse_down(point, MouseButton::Left, Default::default());
        visual.simulate_mouse_up(point, MouseButton::Left, Default::default());
        visual.run_until_parked();
    };
    for (mode, selector) in [
        ("Motion", "backdrop-mode-Motion"),
        ("Gradient", "backdrop-mode-Gradient"),
        ("Solid", "backdrop-mode-Solid"),
    ] {
        click(&mut visual, selector);
        let actual = view
            .read_with(&visual, |e, _| e.document.backdrop.unwrap())
            .unwrap();
        assert_eq!(
            actual.motion != crate::animation::Motion::Still,
            mode == "Motion"
        );
        if mode != "Motion" {
            assert_eq!(actual.gradient, mode == "Gradient");
        }
        let mode_bounds = visual.debug_bounds(selector).unwrap();
        let presets = visual.debug_bounds("backdrop-presets").unwrap();
        let first_preset = visual.debug_bounds("backdrop-preset-0").unwrap();
        for selector in [
            "backdrop-preset-0",
            "backdrop-preset-1",
            "backdrop-preset-2",
            "backdrop-preset-3",
            "backdrop-preset-4",
            "backdrop-preset-5",
            "backdrop-preset-6",
            "backdrop-preset-7",
        ] {
            let swatch = visual.debug_bounds(selector).unwrap();
            assert_eq!(
                swatch.top(),
                first_preset.top(),
                "All eight presets fit one row"
            );
            assert!(swatch.left() >= presets.left() && swatch.right() <= presets.right());
        }
        let bounds: Vec<_> = [
            "backdrop-Outside padding",
            "backdrop-Inside padding",
            "backdrop-Image corners",
            "backdrop-Shadow",
        ]
        .into_iter()
        .map(|selector| visual.debug_bounds(selector).unwrap())
        .collect();
        assert_eq!(bounds[0].top(), bounds[1].top());
        assert_eq!(bounds[2].top(), bounds[3].top());
        assert!(bounds[0].right() < bounds[1].left());
        for bound in bounds {
            assert!(
                bound.bottom() < mode_bounds.top(),
                "Shared control must be above the modes"
            );
            assert!(bound.right() <= px(1050.) && bound.bottom() <= px(600.));
        }
        // GPUI retains debug selectors from older frames; only assert presence
        // for visible controls, and verify mode transitions through document state.
        if mode == "Motion" {
            assert!(visual.debug_bounds("backdrop-duration").is_some());
            assert!(visual.debug_bounds("backdrop-motion-effects").is_some());
        }
    }
    click(&mut visual, "backdrop-format");
    assert!(visual.debug_bounds("backdrop-popup").is_some());
    visual.simulate_keystrokes("down enter");
    visual.run_until_parked();
    assert_eq!(
        view.read_with(&visual, |e, _| e.document.backdrop.unwrap().format)
            .unwrap(),
        Format::Square
    );
    assert!(
        view.read_with(&visual, |e, _| e.panels.popup.is_none())
            .unwrap()
    );
    view.update(&mut visual, |e, _, _| e.document.undo())
        .unwrap();
    assert_eq!(
        view.read_with(&visual, |e, _| e.document.backdrop.unwrap().format)
            .unwrap(),
        Format::Auto
    );
    click(&mut visual, "backdrop-format");
    click(&mut visual, "popup-option-6");
    assert_eq!(
        view.read_with(&visual, |e, _| e.document.backdrop.unwrap().format)
            .unwrap(),
        Format::Vertical
    );
    click(&mut visual, "backdrop-enabled");
    assert!(
        view.read_with(&visual, |e, _| e.document.backdrop.is_none())
            .unwrap()
    );
    click(&mut visual, "backdrop-enabled");
    assert_eq!(
        view.read_with(&visual, |e, _| e.document.backdrop.unwrap().format)
            .unwrap(),
        Format::Vertical
    );
    click(&mut visual, "export-trigger");
    assert!(visual.debug_bounds("backdrop-popup").is_some());
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert!(
        view.read_with(&visual, |e, _| e.panels.popup.is_none())
            .unwrap()
    );
}

#[cfg(target_os = "linux")]
#[gpui::test]
fn linux_control_shortcuts_edit_text_and_undo_once(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        e.set_tool(Tool::Text, cx);
        e.begin(&down(10., 10.), w, cx);
        e.replace_text_in_range(None, "Replace me", w, cx);
        e.key(&key("ctrl-a"), w, cx);
        e.replace_text_in_range(None, "Linux label", w, cx);
        e.key(&key("enter"), w, cx);
        assert_eq!(e.document.marks[0].text, "Linux label");
        e.key(&key("ctrl-z"), w, cx);
        assert!(e.document.marks.is_empty());
        // Undo waits for its raster preview before accepting the next edit.
        e.receive(
            Message::Preview(
                e.preview.revision,
                0,
                0,
                render_image(e.document.render(None)),
            ),
            cx,
        );
        e.key(&key("ctrl-shift-z"), w, cx);
        assert_eq!(e.document.marks.len(), 1);
    })
    .unwrap();
}

#[gpui::test]
fn sidebar_options_edit_selection_remember_each_tool_and_undo(cx: &mut TestAppContext) {
    use super::actions::Action;
    use crate::style::{Cleanup, Fill, Style};
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        e.dispatch(
            Action::SelectTool {
                tool: Tool::Rectangle,
            },
            cx,
        )
        .unwrap();
        e.dispatch(Action::SetStrokeWidth { width: 8. }, cx)
            .unwrap();
        e.dispatch(
            Action::SetAppearance {
                style: Style {
                    fill: Fill::Filled,
                    radius: 12.,
                    ..Default::default()
                },
            },
            cx,
        )
        .unwrap();
        reset_layout(e);
        e.begin(&down(10., 10.), w, cx);
        e.finish(&up(80., 80.), cx);
        assert_eq!(e.document.marks[0].width, 8.);
        assert_eq!(e.document.marks[0].style.fill, Fill::Filled);
        e.dispatch(
            Action::SetColor {
                color: [255, 0, 0, 128],
            },
            cx,
        )
        .unwrap();
        assert_eq!(e.document.export().get_pixel(45, 45).0, [128, 0, 0, 255]);
        e.dispatch(Action::Undo, cx).unwrap();
        assert_eq!(e.document.marks[0].color[3], 255);
    })
    .unwrap();
    cx.run_until_parked();
    view.update(cx, |e, _, cx| {
        e.receive(
            Message::Preview(
                e.preview.revision,
                e.document.marks.len(),
                0,
                render_image(e.document.render(None)),
            ),
            cx,
        );
        e.dispatch(Action::SelectTool { tool: Tool::Pen }, cx)
            .unwrap();
        assert_eq!(e.tool_settings().width, 5.);
        e.dispatch(
            Action::SetAppearance {
                style: Style {
                    cleanup: Cleanup::Adaptive,
                    ..Default::default()
                },
            },
            cx,
        )
        .unwrap();
        e.dispatch(
            Action::SelectTool {
                tool: Tool::Rectangle,
            },
            cx,
        )
        .unwrap();
        assert_eq!(e.tool_settings().width, 8.);
        assert_eq!(e.tool_settings().style.fill, Fill::Filled);
        e.dispatch(Action::SelectTool { tool: Tool::Pen }, cx)
            .unwrap();
        assert_eq!(e.tool_settings().style.cleanup, Cleanup::Adaptive);
    })
    .unwrap();
}
#[gpui::test]
fn sidebar_add_point_and_magnifier_defaults_work_before_drawing(cx: &mut TestAppContext) {
    use super::actions::Action;
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        e.dispatch(Action::SelectTool { tool: Tool::Arrow }, cx)
            .unwrap();
        reset_layout(e);
        e.begin(&down(10., 50.), w, cx);
        e.finish(&up(90., 50.), cx);
        e.dispatch(Action::AddLinePoint, cx).unwrap();
        assert_eq!(e.document.marks[0].points.len(), 3);
        e.begin(&down(50., 50.), w, cx);
        e.finish(&up(50., 20.), cx);
        assert_eq!(e.document.marks[0].points[1], (50., 20.));
        e.dispatch(Action::StraightenLine, cx).unwrap();
        assert_eq!(e.document.marks[0].points.len(), 2);
        e.dispatch(Action::Undo, cx).unwrap();
        assert_eq!(e.document.marks[0].points.len(), 3);
    })
    .unwrap();
    cx.run_until_parked();
    view.update(cx, |e, w, cx| {
        e.receive(
            Message::Preview(
                e.preview.revision,
                e.document.marks.len(),
                0,
                render_image(e.document.render(None)),
            ),
            cx,
        );
        e.dispatch(
            Action::SelectTool {
                tool: Tool::Magnifier,
            },
            cx,
        )
        .unwrap();
        e.dispatch(Action::SetMagnifierZoom { zoom: 4. }, cx)
            .unwrap();
        reset_layout(e);
        e.begin(&down(20., 20.), w, cx);
        e.finish(&up(70., 70.), cx);
        assert_eq!(crate::effects::zoom(e.document.marks.last().unwrap()), 4.);
    })
    .unwrap();
}
#[gpui::test]
fn every_tool_sidebar_fits_the_minimum_window_and_sidebar_scroll_does_not_pan(
    cx: &mut TestAppContext,
) {
    use super::actions::Action;
    let view = editor(cx);
    let mut visual = gpui::VisualTestContext::from_window(*view, cx);
    visual.simulate_resize(size(px(1050.), px(600.)));
    for tool in [
        Tool::Select,
        Tool::Pen,
        Tool::Arrow,
        Tool::Rectangle,
        Tool::Text,
        Tool::Highlight,
        Tool::Pixelate,
        Tool::Crop,
        Tool::Counter,
        Tool::Spotlight,
        Tool::Magnifier,
    ] {
        view.update(&mut visual, |e, _, cx| {
            e.dispatch(Action::SelectTool { tool }, cx).unwrap()
        })
        .unwrap();
        visual.run_until_parked();
        let b = visual.debug_bounds("tool-panel").unwrap();
        assert_eq!(b.size.width, px(260.));
        assert!(b.origin.x >= px(0.) && b.right() <= px(1050.));
        for selector in [
            "thickness-field",
            "opacity-field",
            "font-size-field",
            "block-size-field",
            "badge-diameter-field",
            "next-number-field",
            "lens-diameter-field",
            "dim-surroundings-field",
            "intensity-field",
            "corner-radius-field",
            "field-stroke",
            "field-fill",
            "field-start",
            "field-end",
            "field-zoom",
            "field-aspect-ratio",
            "color-picker-custom-color",
        ] {
            if let Some(control) = visual.debug_bounds(selector) {
                assert!(
                    control.left() >= b.left() && control.right() <= b.right(),
                    "{tool:?}: {selector} extends outside the inspector"
                );
                assert!(
                    control.top() >= b.top() && control.bottom() <= b.bottom(),
                    "{tool:?}: {selector} needs scrolling at the minimum window size"
                );
            }
        }
        if tool == Tool::Arrow {
            assert!(visual.debug_bounds("field-stroke").unwrap().size.width <= px(110.));
        }
        if tool == Tool::Magnifier {
            let diameter = visual.debug_bounds("lens-diameter-field").unwrap();
            let zoom = visual.debug_bounds("field-zoom").unwrap();
            assert!(zoom.left() > diameter.right());
            assert!(zoom.top() < diameter.bottom() && zoom.bottom() > diameter.top());
        }
        let pan = view.update(&mut visual, |e, _, _| e.viewport.pan).unwrap();
        visual.simulate_event(gpui::ScrollWheelEvent {
            position: point(b.origin.x + px(100.), b.origin.y + px(100.)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-30.))),
            modifiers: Default::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        view.update(&mut visual, |e, _, _| assert_eq!(e.viewport.pan, pan))
            .unwrap();
    }
}

#[gpui::test]
fn sidebar_buttons_change_objects_without_starting_canvas_gestures(cx: &mut TestAppContext) {
    use super::actions::Action;
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        e.dispatch(
            Action::SelectTool {
                tool: Tool::Rectangle,
            },
            cx,
        )
        .unwrap();
        reset_layout(e);
        e.begin(&down(10., 10.), w, cx);
        e.finish(&up(80., 80.), cx);
    })
    .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(*view, cx);
    visual.simulate_resize(size(px(1050.), px(860.)));
    visual.run_until_parked();
    for selector in ["thickness-more", "fill-Filled", "corner-radius-more"] {
        let p = visual.debug_bounds(selector).unwrap().center();
        visual.simulate_mouse_down(p, MouseButton::Left, Default::default());
        visual.simulate_mouse_up(p, MouseButton::Left, Default::default());
        visual.run_until_parked();
    }
    view.update(&mut visual, |e, _, _| {
        assert_eq!(e.document.marks.len(), 1);
        assert_eq!(e.document.marks[0].width, 6.);
        assert_eq!(e.document.marks[0].style.fill, crate::style::Fill::Filled);
        assert_eq!(e.document.marks[0].style.radius, 4.);
        assert!(!e.interaction.gesture.is_active());
        assert_eq!(e.interaction.selected, Some(0));
    })
    .unwrap();
}

#[gpui::test]
fn compact_numeric_entry_preserves_selection_and_local_text_history(cx: &mut TestAppContext) {
    use super::actions::Action;
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        e.dispatch(
            Action::SelectTool {
                tool: Tool::Rectangle,
            },
            cx,
        )
        .unwrap();
        reset_layout(e);
        e.begin(&down(10., 10.), w, cx);
        e.finish(&up(80., 80.), cx);
    })
    .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(*view, cx);
    visual.update(|window, cx| {
        window.activate_window();
        crate::menus::install(cx);
    });
    visual.simulate_resize(size(px(1050.), px(600.)));
    visual.run_until_parked();
    let click = |visual: &mut gpui::VisualTestContext, selector| {
        let p = visual.debug_bounds(selector).unwrap().center();
        visual.simulate_click(p, Default::default());
        visual.run_until_parked();
    };
    click(&mut visual, "thickness-value");
    visual.simulate_input("12");
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    view.update(&mut visual, |e, _, _| {
        assert_eq!(e.document.marks[0].width, 12.);
        assert_eq!(e.interaction.selected, Some(0));
        assert!(!e.interaction.gesture.is_active());
    })
    .unwrap();
    click(&mut visual, "thickness-value");
    visual.simulate_input("NaN");
    visual.simulate_keystrokes("enter");
    view.update(&mut visual, |e, _, _| {
        assert_eq!(e.document.marks[0].width, 12.);
        assert_eq!(e.options_tool(), Tool::Rectangle);
    })
    .unwrap();
    visual.simulate_keystrokes("escape");
    click(&mut visual, "thickness-value");
    visual.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("8".into())));
    visual.simulate_keystrokes(&crate::platform::key_binding("cmd-v cmd-z enter"));
    visual.run_until_parked();
    view.update(&mut visual, |e, _, _| {
        assert_eq!(e.document.marks[0].width, 12.)
    })
    .unwrap();
    // A valid draft commits when another field takes focus.
    click(&mut visual, "thickness-value");
    visual.simulate_input("9");
    click(&mut visual, "opacity-value");
    view.update(&mut visual, |e, _, _| {
        assert_eq!(e.document.marks[0].width, 9.)
    })
    .unwrap();
    visual.simulate_keystrokes("escape");
    visual.simulate_keystrokes(&crate::platform::key_binding("cmd-z"));
    visual.run_until_parked();
    view.update(&mut visual, |e, _, _| {
        assert_eq!(e.document.marks[0].width, 12.)
    })
    .unwrap();
}

#[gpui::test]
fn tool_picker_custom_colors_preserve_opacity_and_source_sampling_keeps_selection(
    cx: &mut TestAppContext,
) {
    use super::actions::Action;
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        e.document = Document::new(image::RgbaImage::from_pixel(
            100,
            100,
            image::Rgba([22, 44, 88, 255]),
        ));
        e.dispatch(Action::SelectTool { tool: Tool::Arrow }, cx)
            .unwrap();
        e.dispatch(
            Action::SetColor {
                color: [255, 56, 100, 128],
            },
            cx,
        )
        .unwrap();
        reset_layout(e);
        e.begin(&down(10., 50.), w, cx);
        e.finish(&up(90., 50.), cx);
    })
    .unwrap();
    let mut visual = gpui::VisualTestContext::from_window(*view, cx);
    visual.update(|_, cx| crate::menus::install(cx));
    visual.simulate_resize(size(px(1050.), px(600.)));
    visual.run_until_parked();
    let click = |visual: &mut gpui::VisualTestContext, selector| {
        let p = visual.debug_bounds(selector).unwrap().center();
        visual.simulate_click(p, Default::default());
        visual.run_until_parked();
    };
    click(&mut visual, "color-picker-custom-color");
    let popup = visual.debug_bounds("color-picker-popup").unwrap();
    assert!(
        popup.top() >= px(0.) && popup.bottom() <= px(600.),
        "popup: {popup:?}"
    );
    click(&mut visual, "color-picker-hex");
    visual.simulate_input("#12abEF");
    visual.simulate_keystrokes("enter escape");
    visual.run_until_parked();
    view.update(&mut visual, |e, _, _| {
        assert_eq!(e.document.marks[0].color, [18, 171, 239, 128]);
        assert_eq!(e.interaction.selected, Some(0));
        assert!(e.document.backdrop.is_none());
    })
    .unwrap();
    click(&mut visual, "color-picker-custom-color");
    assert!(visual.debug_bounds("Pick from image").is_none());
    assert!(visual.debug_bounds("Pick from screen").is_some());
    visual.simulate_keystrokes("escape");
    view.update(&mut visual, |e, w, cx| {
        e.dispatch(Action::BeginToolColorSampling, cx).unwrap();
        assert!(e.panels.sampling_tool_color);
        reset_layout(e);
        e.begin(&down(20., 20.), w, cx);
        assert_eq!(e.document.marks.len(), 1);
        assert_eq!(e.document.marks[0].color, [22, 44, 88, 128]);
        assert_eq!(e.interaction.selected, Some(0));
        assert!(!e.panels.sampling_tool_color);
        assert!(!e.interaction.gesture.is_active());
        e.dispatch(Action::Undo, cx).unwrap();
        assert_eq!(e.document.marks[0].color, [18, 171, 239, 128]);
    })
    .unwrap();
}

#[gpui::test]
fn incurs_catalog_uses_native_dispatch_state_undo_and_revision_guards(cx: &mut TestAppContext) {
    use incurs::tool::{ToolCallOptions, ToolCallOutcome};
    use serde_json::{Value, json};
    let view = editor(cx);
    let (sender, receiver) = async_channel::unbounded();
    let worker = std::thread::spawn(move || {
        let dispatch = Arc::new(move |name: &str, args: Value| {
            crate::automation::dispatch(&sender, name, args)
        });
        let catalog = crate::cli::editor_cli_with(dispatch).tool_catalog();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            async fn call(catalog: &incurs::tool::ToolCatalog, name: &str, args: Value) -> Value {
                let args = args.as_object().unwrap().iter().map(|(k,v)| (k.clone(),v.clone())).collect();
                match catalog.call(name, args, ToolCallOptions::isolated()).await {
                    ToolCallOutcome::Ok { data, .. } => data,
                    other => panic!("{name}: {other:?}"),
                }
            }
            let initial = call(&catalog, "get_document", json!({})).await;
            let revision = initial["revision"].as_u64().unwrap();
            assert_eq!((initial["width"].as_u64(), initial["height"].as_u64()), (Some(100), Some(100)));
            let added = call(&catalog, "add_annotation", json!({"mark":{
                "tool":"rectangle","points":[[10,10],[60,40]],"color":[255,0,0,255],"width":3,"text":""
            },"expected_revision":revision})).await;
            assert_eq!(added["objects"].as_array().unwrap().len(), 1);
            let args = json!({"action":{"type":"rotate"},"expected_revision":revision})
                .as_object().unwrap().iter().map(|(k,v)|(k.clone(),v.clone())).collect();
            assert!(matches!(catalog.call("dispatch_action", args, ToolCallOptions::isolated()).await,
                ToolCallOutcome::Error { message, .. } if message.contains("Stale")));
            let state = call(&catalog, "get_document", json!({})).await;
            assert_eq!(state["objects"].as_array().unwrap().len(), 1);
            call(&catalog, "undo", json!({})).await;
            let undone = call(&catalog, "get_document", json!({})).await;
            assert!(undone["objects"].as_array().unwrap().is_empty());
            call(&catalog, "redo", json!({})).await;
            let redone = call(&catalog, "get_document", json!({})).await;
            assert_eq!(redone["objects"].as_array().unwrap().len(), 1);
            call(&catalog, "dispatch_action", json!({"action":{"type":"set_color","color":[17,34,51,128]}})).await;
            let state = call(&catalog, "get_editor_state", json!({})).await;
            assert_eq!(state["tool_options"]["color"], json!([17,34,51,128]));
            call(&catalog, "resize_image", json!({"scale":0.5,"smart":false})).await;
            let resized = call(&catalog, "get_document", json!({})).await;
            assert_eq!(resized["width"], 50);
            assert_eq!(resized["height"], 50);
        });
    });
    while let Ok(Message::Automation(request)) = receiver.recv_blocking() {
        view.update(cx, |e, _, cx| e.automation(request, cx))
            .unwrap();
    }
    worker.join().unwrap();
}

fn selection_marks(e: &mut Editor) {
    for x in [10., 40., 75.] {
        e.document.commit(crate::document::Mark {
            tool: Tool::Rectangle,
            points: vec![(x, 10.), (x + 10., 20.)],
            color: [255, 0, 0, 255],
            width: 2.,
            text: String::new(),
            curve: None,
            style: Default::default(),
        });
    }
}

#[gpui::test]
fn marquee_selection_moves_group_atomically_and_preserves_relative_positions(
    cx: &mut TestAppContext,
) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        selection_marks(e);
        let original = e.document.marks.clone();
        let revision = e.preview.revision;
        // Drag backwards from empty canvas; both intersecting marks are selected.
        e.begin(&down(60., 30.), w, cx);
        e.motion(&motion(5., 5.), cx);
        assert_eq!(e.selected_indices(), vec![0, 1]);
        e.finish(&up(5., 5.), cx);
        assert_eq!(e.document.marks, original);
        assert_eq!(e.preview.revision, revision);
        e.begin(&down(10., 15.), w, cx);
        assert_eq!(e.interaction.gesture.first_drag_index(), Some(0));
        e.motion(&motion(20., 25.), cx);
        assert_eq!(e.document.marks, original);
        assert_eq!(
            e.interaction.gesture.moved_mark(0).unwrap().points[0],
            (20., 20.)
        );
        assert_eq!(
            e.interaction.gesture.moved_mark(1).unwrap().points[0],
            (50., 20.)
        );
        e.finish(&up(20., 25.), cx);
        assert_eq!(e.selected_indices(), vec![0, 1]);
        assert_eq!(e.document.marks[0].points[0], (20., 20.));
        assert_eq!(e.document.marks[1].points[0], (50., 20.));
        assert_eq!(e.document.marks[2], original[2]);
        e.document.undo();
        assert_eq!(e.document.marks, original);
        e.document.redo();
        assert_eq!(e.document.marks[1].points[0], (50., 20.));
        // A selected group click without motion must not create an undo entry.
        let moved = e.document.marks.clone();
        e.begin(&down(20., 25.), w, cx);
        e.finish(&up(20., 25.), cx);
        assert_eq!(e.document.marks, moved);
        e.document.undo();
        assert_eq!(e.document.marks, original);
    })
    .unwrap();
}

#[gpui::test]
fn additive_selection_clicks_cancel_and_select_all_are_contextual(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        reset_layout(e);
        selection_marks(e);
        e.begin(&down(5., 5.), w, cx);
        e.finish(&up(25., 25.), cx);
        assert_eq!(e.selected_indices(), vec![0]);
        let mut press = down(35., 5.);
        press.modifiers.shift = true;
        e.begin(&press, w, cx);
        e.finish(&up(55., 25.), cx);
        assert_eq!(e.selected_indices(), vec![0, 1]);
        let mut toggle = down(10., 15.);
        toggle.modifiers.shift = true;
        e.begin(&toggle, w, cx);
        e.finish(&up(10., 15.), cx);
        assert_eq!(e.selected_indices(), vec![1]);
        e.begin(&toggle, w, cx);
        e.finish(&up(10., 15.), cx);
        assert_eq!(e.selected_indices(), vec![0, 1]);
        let original = e.document.marks.clone();
        e.begin(&down(10., 15.), w, cx);
        e.motion(&motion(20., 25.), cx);
        e.key(&key("escape"), w, cx);
        e.finish(&up(20., 25.), cx);
        assert_eq!(e.document.marks, original);
        assert!(e.selected_indices().is_empty());
        e.key(&key(&crate::platform::key_binding("cmd-a")), w, cx);
        assert_eq!(e.selected_indices(), vec![0, 1, 2]);
        // Empty clicks clear selection, including inside an outline rectangle.
        e.begin(&down(65., 50.), w, cx);
        e.finish(&up(65., 50.), cx);
        assert!(e.selected_indices().is_empty());
        e.set_tool(Tool::Text, cx);
        e.begin(&down(5., 50.), w, cx);
        e.interaction
            .text_edit
            .as_mut()
            .unwrap()
            .replace_text("Hello 👋");
        e.key(&key(&crate::platform::key_binding("cmd-a")), w, cx);
        let edit = e.interaction.text_edit.as_ref().unwrap();
        assert_eq!(edit.buffer.selection(), 0.."Hello 👋".len());
        assert_eq!(e.document.marks, original);
        assert!(e.selected_indices().is_empty());
    })
    .unwrap();
}

#[gpui::test]
fn marquee_uses_source_coordinates_at_zoom_and_cancel_restores_selection(cx: &mut TestAppContext) {
    let view = editor(cx);
    view.update(cx, |e, w, cx| {
        selection_marks(e);
        e.set_selection(vec![2]);
        e.viewport.layout.set(Layout {
            x: 15.,
            y: 25.,
            scale: 2.,
            width: 100.,
            height: 100.,
        });
        e.begin(&down(25., 35.), w, cx); // source (5, 5)
        e.motion(&motion(125., 75.), cx); // source (55, 25)
        assert_eq!(e.selected_indices(), vec![0, 1]);
        e.cancel_gesture();
        assert_eq!(e.selected_indices(), vec![2]);
        e.begin(&down(25., 35.), w, cx);
        e.finish(&up(125., 75.), cx);
        assert_eq!(e.selected_indices(), vec![0, 1]);
    })
    .unwrap();
}

#[gpui::test]
fn select_all_menu_and_shortcut_use_full_event_dispatch(cx: &mut TestAppContext) {
    let window = editor(cx);
    let view = window.root(cx).unwrap();
    window.update(cx, |e, _, _| selection_marks(e)).unwrap();
    let mut visual = gpui::VisualTestContext::from_window(*window, cx);
    visual.update(|_, cx| crate::menus::install(cx));
    visual.simulate_keystrokes(&crate::platform::key_binding("cmd-a"));
    assert_eq!(
        view.read_with(&visual, |e, _| e.selected_indices()),
        vec![0, 1, 2]
    );
    visual.simulate_keystrokes("t");
    let l = view.read_with(&visual, |e, _| e.viewport.layout.get());
    visual.simulate_click(
        point(px(l.x + 5. * l.scale), px(l.y + 50. * l.scale)),
        Default::default(),
    );
    visual.simulate_input("Hello");
    visual.simulate_keystrokes(&crate::platform::key_binding("cmd-a"));
    assert_eq!(
        view.read_with(&visual, |e, _| e
            .interaction
            .text_edit
            .as_ref()
            .unwrap()
            .buffer
            .selection()),
        0..5
    );
    assert!(view.read_with(&visual, |e, _| e.selected_indices().is_empty()));
    visual.simulate_keystrokes("escape");
    visual.dispatch_action(crate::menus::SelectAll);
    assert_eq!(
        view.read_with(&visual, |e, _| e.selected_indices()),
        vec![0, 1, 2]
    );
}
