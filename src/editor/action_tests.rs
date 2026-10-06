use super::{Editor, actions::Action, jobs::OperationKind};
use crate::document::{Document, Tool};
use gpui::{AppContext, TestAppContext, VisualTestContext};

#[gpui::test]
fn ocr_bridge_copies_text_and_preserves_image_history(cx: &mut TestAppContext) {
    use super::jobs::{Message, OperationResult};
    use crate::automation::Request;
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        e.recognize_text = |_, _| Ok("First line\n第二行".into());
        e.document.remember();
        e.document.marks.push(crate::document::Mark {
            tool: Tool::Text,
            points: vec![(10., 10.)],
            color: [0, 0, 0, 255],
            width: 3.,
            text: "An editable annotation".into(),
            curve: None,
            style: Default::default(),
        });
        let image = e.document.base.clone();
        let revision = e.preview.revision;
        let bridge = |e: &mut Editor, cx: &mut gpui::Context<Editor>, action, revision| {
            let (reply, response) = std::sync::mpsc::channel();
            e.automation(
                Request::Dispatch {
                    action,
                    expected_revision: Some(revision),
                    reply,
                },
                cx,
            );
            response.recv().unwrap()
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("Existing clipboard".into()));
        assert!(
            bridge(
                e,
                cx,
                Action::CopyOcr {
                    rectangle: None,
                    engine: None
                },
                revision + 1
            )
            .is_err()
        );
        assert!(e.operations.active.is_none());
        assert!(
            bridge(
                e,
                cx,
                Action::CopyOcr {
                    engine: None,
                    rectangle: Some([0, 0, 0, 10])
                },
                revision
            )
            .is_err()
        );
        assert!(e.operations.active.is_none());
        let receipt = bridge(
            e,
            cx,
            Action::CopyOcr {
                engine: None,
                rectangle: Some([0, 0, 100, 80]),
            },
            revision,
        )
        .unwrap();
        let id = e.operations.active.as_ref().unwrap().id;
        assert_eq!(receipt.operation_id, Some(id.value()));
        assert_eq!(receipt.revision, revision);
        assert!(!e.panels.enhance, "copy OCR must not open a result pane");
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "Existing clipboard"
        );
        assert_eq!(
            e.feedback.copy,
            Some(super::feedback::CopyFeedback::ReadingText)
        );
        assert!(
            bridge(
                e,
                cx,
                Action::CopyOcr {
                    rectangle: None,
                    engine: None
                },
                revision
            )
            .is_err()
        );
        e.receive(
            Message::Operation(
                id,
                OperationResult::ExtractedText {
                    engine: crate::chatgpt::OcrEngine::Local,
                    revision,
                    rectangle: [0, 0, 100, 80],
                    result: Ok("First line\n第二行".into()),
                },
            ),
            cx,
        );
        let (reply, response) = std::sync::mpsc::channel();
        e.automation(Request::State(reply), cx);
        let state = response.recv().unwrap().unwrap();
        assert_eq!(state["extraction"]["text"], "First line\n第二行");
        assert_eq!(
            state["extraction"]["rectangle"],
            serde_json::json!([0, 0, 100, 80])
        );
        assert_eq!(state["extraction"]["engine"], "local");
        assert_eq!(state["busy"], false);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "First line\n第二行"
        );
        assert_eq!(
            e.feedback.copy,
            Some(super::feedback::CopyFeedback::TextCopied)
        );
        assert_eq!(e.preview.revision, revision);
        assert!(std::sync::Arc::ptr_eq(&image, &e.document.base));
        assert_eq!(e.document.marks[0].text, "An editable annotation");
        assert!(!e.panels.enhance);
        bridge(e, cx, Action::Undo, revision).unwrap();
        assert!(
            e.document.marks.is_empty(),
            "OCR must not insert document undo steps"
        );
        assert!(e.extraction.is_none());
    });
}

#[gpui::test]
fn ocr_stale_empty_and_failed_results_preserve_clipboard(cx: &mut TestAppContext) {
    use super::jobs::{Message, OperationResult};
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("Keep this".into()));
        let revision = e.preview.revision;
        let old = e.start_operation(OperationKind::Ocr).unwrap();
        e.changed();
        e.receive(
            Message::Operation(
                old,
                OperationResult::ExtractedText {
                    engine: crate::chatgpt::OcrEngine::Local,
                    revision,
                    rectangle: [0, 0, 100, 100],
                    result: Ok("stale".into()),
                },
            ),
            cx,
        );
        assert!(e.extraction.is_none());
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "Keep this"
        );
        let current = e.start_operation(OperationKind::Ocr).unwrap();
        e.receive(
            Message::Operation(
                old,
                OperationResult::ExtractedText {
                    engine: crate::chatgpt::OcrEngine::Local,
                    revision: e.preview.revision,
                    rectangle: [0, 0, 100, 100],
                    result: Ok("wrong operation".into()),
                },
            ),
            cx,
        );
        assert_eq!(e.operations.active.as_ref().unwrap().id, current);
        assert!(e.extraction.is_none());
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "Keep this"
        );
        e.receive(
            Message::Operation(
                current,
                OperationResult::ExtractedText {
                    engine: crate::chatgpt::OcrEngine::Local,
                    revision: e.preview.revision,
                    rectangle: [0, 0, 100, 100],
                    result: Ok(String::new()),
                },
            ),
            cx,
        );
        assert_eq!(e.extraction.as_ref().unwrap().text, "");
        assert!(e.feedback.status.contains("No text found"));
        assert_eq!(e.feedback.copy, Some(super::feedback::CopyFeedback::NoText));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "Keep this"
        );
        e.recognize_text = |_, _| Err("OCR unavailable".into());
        e.dispatch(
            Action::CopyOcr {
                rectangle: None,
                engine: None,
            },
            cx,
        )
        .unwrap();
        assert!(e.extraction.is_none());
        let id = e.operations.active.as_ref().unwrap().id;
        e.receive(
            Message::Operation(
                id,
                OperationResult::ExtractedText {
                    engine: crate::chatgpt::OcrEngine::Local,
                    revision: e.preview.revision,
                    rectangle: [0, 0, 100, 100],
                    result: Err("OCR unavailable".into()),
                },
            ),
            cx,
        );
        assert_eq!(e.feedback.status, "OCR unavailable");
        assert!(e.feedback.copy.is_none());
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "Keep this"
        );
    });
}

#[gpui::test]
fn toolbar_ocr_starts_shared_copy_action_without_opening_panels(cx: &mut TestAppContext) {
    use gpui::{px, size};
    let window = cx.add_window(|_, cx| {
        let mut e = Editor::with_native(cx, false);
        e.recognize_text = |_, _| Ok("Toolbar text".into());
        e
    });
    let entity = window.root(cx).unwrap();
    let mut visual = VisualTestContext::from_window(*window, cx);
    visual.simulate_resize(size(px(1200.), px(760.)));
    visual.run_until_parked();
    let bounds = visual.debug_bounds("copy-ocr").unwrap();
    visual.simulate_click(bounds.center(), Default::default());
    visual.run_until_parked();
    entity.read_with(&visual, |e, _| {
        assert!(!e.panels.enhance && !e.panels.backdrop && !e.panels.animation);
        assert!(
            e.operations
                .active
                .as_ref()
                .is_some_and(|op| op.kind == OperationKind::Ocr)
                || e.extraction
                    .as_ref()
                    .is_some_and(|result| result.text == "Toolbar text")
        );
        assert_eq!(e.preview.revision, 0);
    });
}

#[gpui::test]
fn nebula_and_legacy_stars_use_shared_motion_action_and_canonical_readback(
    cx: &mut TestAppContext,
) {
    use crate::{animation::Motion, automation::Request, backdrop::Backdrop};
    for name in ["nebula", "stars"] {
        let entity = cx.new(|cx| Editor::with_native(cx, false));
        entity.update(cx, |e, cx| {
            let original = Backdrop {
                motion: Motion::Lava,
                seed: 42,
                ..Default::default()
            };
            e.dispatch(
                Action::SetBackdrop {
                    backdrop: Some(original),
                },
                cx,
            )
            .unwrap();
            let revision = e.preview.revision;
            let action =
                Action::from_json(serde_json::json!({"type":"select_motion","motion":name}))
                    .unwrap();
            let (reply, response) = std::sync::mpsc::channel();
            e.automation(
                Request::Dispatch {
                    action,
                    expected_revision: Some(revision),
                    reply,
                },
                cx,
            );
            response.recv().unwrap().unwrap();
            assert_eq!(e.document.backdrop.unwrap().motion, Motion::Nebula);
            assert_eq!(e.document.backdrop.unwrap().seed, 42);
            assert_eq!(e.preview.revision, revision + 1);
            let (reply, response) = std::sync::mpsc::channel();
            e.automation(Request::Snapshot(reply), cx);
            let snapshot = response.recv().unwrap().unwrap();
            assert_eq!(
                crate::automation::state(&snapshot)["backdrop"]["motion"],
                "nebula"
            );
            e.dispatch(Action::Undo, cx).unwrap();
            assert_eq!(e.document.backdrop, Some(original));
        });
    }
}

#[gpui::test]
fn randomize_motion_uses_bridge_state_revisions_and_undo(cx: &mut TestAppContext) {
    use crate::{animation::Motion, automation::Request, backdrop::Backdrop};
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    let original = entity.update(cx, |e, cx| {
        let revision = e.preview.revision;
        assert!(
            e.dispatch(Action::RandomizeMotion { seed: None }, cx)
                .is_err()
        );
        assert_eq!(e.preview.revision, revision);
        let original = Backdrop {
            motion: Motion::Lava,
            colors: Some([[50, 80, 100], [100, 30, 60]]),
            ..Default::default()
        };
        e.dispatch(
            Action::SetBackdrop {
                backdrop: Some(original),
            },
            cx,
        )
        .unwrap();
        let base = e.document.base.clone();
        e.playback.paused = true;
        e.playback.position = 1.25;
        let bridge = |e: &mut Editor, cx: &mut gpui::Context<Editor>, seed, revision| {
            let (reply, response) = std::sync::mpsc::channel();
            e.automation(
                Request::Dispatch {
                    action: Action::RandomizeMotion { seed },
                    expected_revision: Some(revision),
                    reply,
                },
                cx,
            );
            response.recv().unwrap()
        };
        let revision = e.preview.revision;
        assert!(bridge(e, cx, Some(42), revision - 1).is_err());
        assert_eq!(e.document.backdrop, Some(original));
        bridge(e, cx, Some(42), revision).unwrap();
        assert_eq!(
            e.document.backdrop,
            Some(Backdrop {
                seed: 42,
                ..original
            })
        );
        assert_eq!(e.preview.revision, revision + 1);
        assert!(e.playback.paused);
        assert_eq!(e.playback.position, 1.25);
        assert!(std::sync::Arc::ptr_eq(&base, &e.document.base));
        let (reply, response) = std::sync::mpsc::channel();
        e.automation(Request::Snapshot(reply), cx);
        let snapshot = response.recv().unwrap().unwrap();
        assert_eq!(crate::automation::state(&snapshot)["backdrop"]["seed"], 42);
        bridge(e, cx, None, e.preview.revision).unwrap();
        let fresh = e.document.backdrop.unwrap().seed;
        assert_ne!(fresh, 0);
        assert_ne!(fresh, 42);
        bridge(e, cx, Some(0), e.preview.revision).unwrap();
        assert_eq!(e.document.backdrop, Some(original));
        bridge(e, cx, Some(42), e.preview.revision).unwrap();
        e.dispatch(Action::Undo, cx).unwrap();
        assert_eq!(e.document.backdrop, Some(original));
        original
    });
    entity.update(cx, |e, cx| {
        // Undo queues a foreground refresh; finish it before the next command.
        e.receive(
            super::Message::Preview(
                e.preview.revision,
                e.document.marks.len(),
                original.inside_padding,
                super::render_image(super::preview_base(&e.document)),
            ),
            cx,
        );
        e.dispatch(Action::Redo, cx).unwrap();
        assert_eq!(
            e.document.backdrop,
            Some(Backdrop {
                seed: 42,
                ..original
            })
        );
    });
}

#[gpui::test]
fn randomize_button_fits_and_changes_only_motion_seed(cx: &mut TestAppContext) {
    let view = cx.add_window(|window, cx| {
        let mut editor = Editor::with_native(cx, false);
        editor.focus.focus(window);
        editor.panels.backdrop = true;
        editor
            .dispatch(
                Action::SelectMotion {
                    motion: crate::animation::Motion::Prism,
                },
                cx,
            )
            .unwrap();
        editor
    });
    let root = view.root(cx).unwrap();
    let mut visual = VisualTestContext::from_window(*view, cx);
    visual.simulate_window_resize(*view, gpui::size(gpui::px(1050.), gpui::px(600.)));
    visual.run_until_parked();
    let button = visual.debug_bounds("backdrop-randomize").unwrap();
    let panel = visual.debug_bounds("backdrop-panel").unwrap();
    assert!(
        panel.contains(&button.origin) && panel.contains(&button.bottom_right()),
        "panel {panel:?}, button {button:?}"
    );
    let original = root.read_with(&visual, |e, _| e.document.backdrop.unwrap());
    visual.simulate_click(button.center(), Default::default());
    root.read_with(&visual, |e, _| {
        let randomized = e.document.backdrop.unwrap();
        assert_ne!(randomized.seed, original.seed);
        assert_eq!(
            randomized,
            crate::backdrop::Backdrop {
                seed: randomized.seed,
                ..original
            }
        );
    });
    root.update(&mut visual, |e, cx| e.dispatch(Action::Undo, cx).unwrap());
    root.read_with(&visual, |e, _| {
        assert_eq!(e.document.backdrop, Some(original))
    });
}

#[gpui::test]
fn incompatible_backdrop_duration_rejects_action_before_mutation(cx: &mut TestAppContext) {
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        e.dispatch(
            Action::SetImageAnimation {
                animation: crate::animation::ImageAnimation {
                    effect: crate::animation::Entrance::Pop,
                    duration_ms: 2000,
                    delay_ms: 1000,
                    seconds: 5,
                    exit: true,
                },
            },
            cx,
        )
        .unwrap();
        let revision = e.preview.revision;
        assert!(
            e.dispatch(
                Action::SetBackdrop {
                    backdrop: Some(crate::backdrop::Backdrop {
                        seconds: 2,
                        ..Default::default()
                    }),
                },
                cx
            )
            .is_err()
        );
        assert_eq!(e.preview.revision, revision);
        assert!(e.document.backdrop.is_none());
        assert_eq!(e.document.image_animation.seconds, 5);
    });
}

#[gpui::test]
fn entrance_actions_preserve_pixels_group_slider_undo_and_seek_without_history(
    cx: &mut TestAppContext,
) {
    use crate::animation::{AnimationControl, Entrance, ImageAnimation};
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        let base = e.document.base.clone();
        e.dispatch(
            Action::SelectEntrance {
                effect: Entrance::Tilt,
            },
            cx,
        )
        .unwrap();
        assert!(e.panels.animation && !e.panels.backdrop && !e.panels.enhance);
        assert!(
            e.document.backdrop.is_none(),
            "image animation does not force backdrop motion"
        );
        assert!(std::sync::Arc::ptr_eq(&base, &e.document.base));
        e.dispatch(
            Action::BeginAnimationAdjustment {
                control: AnimationControl::Duration,
                track: (0., 0., 180., 24.),
                position: (40., 12.),
            },
            cx,
        )
        .unwrap();
        e.dispatch(
            Action::SetAnimationControl {
                control: AnimationControl::Duration,
                value: 800,
            },
            cx,
        )
        .unwrap();
        e.dispatch(
            Action::SetAnimationControl {
                control: AnimationControl::Duration,
                value: 1500,
            },
            cx,
        )
        .unwrap();
        e.cancel_gesture();
        e.dispatch(Action::SeekAnimation { seconds: 1.5 }, cx)
            .unwrap();
        assert!(e.playback.paused);
        assert_eq!(e.clip_time(), 1.5);
        let revision = e.preview.revision;
        assert!(
            e.dispatch(
                Action::SetImageAnimation {
                    animation: ImageAnimation {
                        seconds: 2,
                        duration_ms: 2000,
                        delay_ms: 1000,
                        ..e.document.image_animation
                    }
                },
                cx
            )
            .is_err()
        );
        assert_eq!(e.preview.revision, revision);
        assert!(
            e.dispatch(Action::SeekAnimation { seconds: f32::NAN }, cx)
                .is_err()
        );
        e.document.undo();
        assert_eq!(
            e.document.image_animation.duration_ms, 1000,
            "one undo restores the entire slider drag; seeking adds none"
        );
        assert_eq!(e.document.image_animation.effect, Entrance::Tilt);
        e.document.undo();
        assert_eq!(e.document.image_animation.effect, Entrance::None);
        e.document.redo();
        assert_eq!(e.document.image_animation.effect, Entrance::Tilt);
        e.dispatch(Action::ReplayAnimation, cx).unwrap();
        assert!(!e.playback.paused);
        assert!(e.clip_time() < 0.1);
        e.dispatch(Action::SelectTool { tool: Tool::Pen }, cx)
            .unwrap();
        assert!(!e.panels.animation);
    });
}

#[gpui::test]
fn animation_sidebar_buttons_and_timeline_dispatch_at_minimum_window(cx: &mut TestAppContext) {
    use crate::animation::Entrance;
    let view = cx.add_window(|window, cx| {
        let editor = Editor::with_native(cx, false);
        editor.focus.focus(window);
        editor
    });
    let root = view.root(cx).unwrap();
    let mut visual = VisualTestContext::from_window(*view, cx);
    visual.simulate_window_resize(*view, gpui::size(gpui::px(1050.), gpui::px(600.)));
    root.update(&mut visual, |e, cx| {
        e.dispatch(Action::ToggleAnimationPanel, cx).unwrap()
    });
    visual.run_until_parked();
    let panel = visual.debug_bounds("animation-panel").unwrap();
    for selector in ["entrance-Diagonal", "entrance-Pop", "entrance-Tilt"] {
        let b = visual.debug_bounds(selector).unwrap();
        assert!(panel.contains(&b.origin) && panel.contains(&b.bottom_right()));
    }
    let b = visual.debug_bounds("entrance-Diagonal").unwrap();
    visual.simulate_click(b.center(), Default::default());
    root.read_with(&visual, |e, _| {
        assert_eq!(e.document.image_animation.effect, Entrance::Diagonal)
    });
    let timeline = visual.debug_bounds("animation-Preview time").unwrap();
    visual.simulate_click(timeline.center(), Default::default());
    root.read_with(&visual, |e, _| {
        assert!(e.playback.paused);
        assert!((e.clip_time() - 2.5).abs() < 0.05);
        assert!(e.document.marks.is_empty());
    });
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: panel.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-180.))),
        ..Default::default()
    });
    root.read_with(&visual, |e, _| {
        assert_eq!(
            e.viewport.pan,
            (0., 0.),
            "sidebar scrolling must not pan the image"
        );
    });
    let canvas = root.read_with(&visual, |e, _| e.viewport.canvas_bounds.get());
    visual.simulate_event(gpui::ScrollWheelEvent {
        position: canvas.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-20.))),
        ..Default::default()
    });
    root.read_with(&visual, |e, _| assert_eq!(e.viewport.pan, (0., -20.)));
}

#[gpui::test]
fn mcp_transport_dispatches_actions_to_the_live_editor(cx: &mut TestAppContext) {
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    let (sender, receiver) = async_channel::unbounded();
    let (messages, incoming) = std::sync::mpsc::channel();
    let proxy = std::thread::spawn(move || {
        while let Ok(message) = receiver.recv_blocking() {
            if messages.send(message).is_err() {
                break;
            }
        }
    });
    // Use the real transport parser and response path, without a desktop/socket.
    let worker = std::thread::spawn(move || {
        let selected = crate::automation::dispatch(
            &sender,
            "dispatch_action",
            serde_json::json!({
                "action": {"type":"select_tool", "tool":"arrow"}, "expected_revision":0
            }),
        )
        .unwrap();
        assert_eq!(selected["revision"], 0);
        assert!(selected["operation_id"].is_null());
        let state = crate::automation::dispatch(&sender, "get_editor_state", serde_json::json!({}))
            .unwrap();
        assert_eq!(state["tool"], "arrow");
        assert_eq!(state["busy"], false);
        assert!(
            crate::automation::dispatch(
                &sender,
                "dispatch_action",
                serde_json::json!({
                    "action": {"type":"select_tool", "tool":"pen"}, "expected_revision":99
                })
            )
            .is_err()
        );
        let formatted = crate::automation::dispatch(
            &sender,
            "dispatch_action",
            serde_json::json!({
                "action":{"type":"set_backdrop_format","format":"shorts"},
                "expected_revision":0
            }),
        )
        .unwrap();
        assert_eq!(formatted["revision"], 1);
        let document =
            crate::automation::dispatch(&sender, "get_document", serde_json::json!({})).unwrap();
        assert_eq!(document["backdrop"]["format"], "shorts");
        let toggled = crate::automation::dispatch(
            &sender,
            "dispatch_action",
            serde_json::json!({
                "action":{"type":"toggle_backdrop_enabled"},"expected_revision":1
            }),
        )
        .unwrap();
        assert_eq!(toggled["revision"], 2);
        let document =
            crate::automation::dispatch(&sender, "get_document", serde_json::json!({})).unwrap();
        assert!(document["backdrop"].is_null());
        let undone = crate::automation::dispatch(
            &sender,
            "dispatch_action",
            serde_json::json!({"action":{"type":"undo"},"expected_revision":2}),
        )
        .unwrap();
        assert_eq!(undone["revision"], 3);
        // Undo starts a preview refresh. Its receipt accepts the work; document
        // snapshot tools correctly reject requests until that work finishes.
    });
    for _ in 0..8 {
        let message = incoming
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        entity.update(cx, |e, cx| e.receive(message, cx));
    }
    worker.join().unwrap();
    proxy.join().unwrap();
    entity.read_with(cx, |e, _| {
        assert_eq!(e.interaction.tool, Tool::Arrow);
        assert_eq!(
            e.document.backdrop.unwrap().format,
            crate::backdrop::Format::Shorts
        );
    });
}

#[gpui::test]
fn native_and_mcp_edits_produce_the_same_document_and_undo(cx: &mut TestAppContext) {
    use crate::{automation::Snapshot, document::actions::DocumentAction};
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        let mark = crate::document::Mark {
            style: Default::default(),
            tool: Tool::Arrow,
            points: vec![(10., 10.), (70., 50.)],
            curve: Some((30., 5.)),
            color: [255, 0, 0, 255],
            width: 3.,
            text: String::new(),
        };
        let mut snapshot = Snapshot {
            document: e.document.clone(),
            revision: 0,
            phase: 0.,
        };
        e.dispatch(
            Action::Edit {
                edit: DocumentAction::AddAnnotation { mark: mark.clone() },
            },
            cx,
        )
        .unwrap();
        crate::mcp::operate(
            "add_annotation",
            &serde_json::json!({"mark":mark}),
            &mut snapshot,
        )
        .unwrap();
        e.dispatch(
            Action::NudgeSelection {
                delta: (8., 3.),
                remember: true,
            },
            cx,
        )
        .unwrap();
        crate::mcp::operate(
            "move_annotation",
            &serde_json::json!({"id":"0:0","dx":8,"dy":3}),
            &mut snapshot,
        )
        .unwrap();
        assert_eq!(e.document.marks, snapshot.document.marks);
        assert_eq!(e.document.render(None), snapshot.document.render(None));
        e.dispatch(Action::Undo, cx).unwrap();
        crate::mcp::operate("undo", &serde_json::json!({}), &mut snapshot).unwrap();
        assert_eq!(e.document.marks, snapshot.document.marks);
        assert_eq!(e.document.marks[0].curve, Some((30., 5.)));
    });
}

#[gpui::test]
fn backdrop_actions_invalidate_prepared_mcp_work(cx: &mut TestAppContext) {
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        let prepared = Box::new(e.document.clone());
        let revision = e.preview.revision;
        e.dispatch(
            Action::SetBackdrop {
                backdrop: Some(crate::backdrop::Backdrop::default()),
            },
            cx,
        )
        .unwrap();
        assert!(
            e.dispatch(
                Action::ApplyPreparedDocument {
                    document: prepared,
                    revision,
                    replace: false
                },
                cx
            )
            .is_err()
        );
        assert!(e.document.backdrop.is_some());
        // A slider tick must also invalidate work snapped before the drag began.
        let prepared = Box::new(e.document.clone());
        let revision = e.preview.revision;
        e.dispatch(
            Action::BeginBackdropAdjustment {
                control: crate::backdrop::Control::Padding,
                track: (0., 0., 100., 24.),
                position: (90., 12.),
            },
            cx,
        )
        .unwrap();
        e.interaction.gesture = super::state::Gesture::Idle;
        assert!(e.preview.revision > revision);
        assert!(
            e.dispatch(
                Action::ApplyPreparedDocument {
                    document: prepared,
                    revision,
                    replace: false
                },
                cx
            )
            .is_err()
        );
        assert_eq!(e.document.backdrop.unwrap().padding, 180);
    });
}

#[gpui::test]
fn rectangle_gesture_dispatches_a_normalized_annotation(cx: &mut TestAppContext) {
    let view = cx.add_window(|window, cx| {
        let editor = Editor::with_native(cx, false);
        editor.focus.focus(window);
        editor
    });
    let root = view.root(cx).unwrap();
    let mut visual = VisualTestContext::from_window(*view, cx);
    visual.update(|_, cx| crate::menus::install(cx));
    visual.run_until_parked();
    visual.simulate_keystrokes("r");
    let layout = root.read_with(&visual, |e, _| e.viewport.layout.get());
    let p = |x, y| {
        gpui::point(
            gpui::px(layout.x + x * layout.scale),
            gpui::px(layout.y + y * layout.scale),
        )
    };
    visual.simulate_mouse_down(p(10., 10.), gpui::MouseButton::Left, Default::default());
    visual.simulate_mouse_move(p(80., 70.), gpui::MouseButton::Left, Default::default());
    visual.simulate_mouse_up(p(80., 70.), gpui::MouseButton::Left, Default::default());
    root.read_with(&visual, |e, _| {
        assert_eq!(e.document.marks.len(), 1);
        assert_eq!(e.document.marks[0].tool, Tool::Rectangle);
        assert_eq!(e.document.marks[0].points.len(), 2);
    });
}

#[gpui::test]
fn buttons_shortcuts_and_menu_actions_share_tool_selection(cx: &mut TestAppContext) {
    cx.update(crate::menus::install);
    let view = cx.add_window(|window, cx| {
        let editor = Editor::with_native(cx, false);
        editor.focus.focus(window);
        editor
    });
    let mut visual = VisualTestContext::from_window(*view, cx);
    visual.run_until_parked();
    visual.simulate_keystrokes("a");
    visual.run_until_parked();
    view.update(&mut visual, |e, _, _| {
        assert_eq!(e.interaction.tool, Tool::Arrow)
    })
    .unwrap();
    visual.dispatch_action(crate::menus::Pen);
    visual.run_until_parked();
    view.update(&mut visual, |e, _, _| {
        assert_eq!(e.interaction.tool, Tool::Pen)
    })
    .unwrap();
    let button = visual.debug_bounds("tool-arrow-up-right").unwrap();
    visual.simulate_click(button.center(), Default::default());
    visual.run_until_parked();
    view.update(&mut visual, |e, _, _| {
        assert_eq!(e.interaction.tool, Tool::Arrow)
    })
    .unwrap();
}

#[gpui::test]
fn rejected_actions_do_not_modify_state(cx: &mut TestAppContext) {
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        assert!(
            e.dispatch(
                Action::Resize {
                    scale: f32::NAN,
                    smart: false
                },
                cx
            )
            .is_err()
        );
        assert_eq!(e.panels.resize_scale, 2.);
        assert!(e.panels.resize_smart);
        e.start_operation(OperationKind::Upload).unwrap();
        assert!(
            e.dispatch(Action::SelectTool { tool: Tool::Arrow }, cx)
                .is_err()
        );
        assert_eq!(e.interaction.tool, Tool::Select);
        assert!(e.dispatch(Action::CancelExport, cx).is_ok());
        assert!(e.operations.active.is_some());
    });
}

#[test]
fn malformed_mcp_actions_are_rejected_before_entering_the_editor_queue() {
    let (sender, receiver) = async_channel::unbounded();
    for action in [
        serde_json::json!({"type":"fit","scale":2}),
        serde_json::json!({"type":"select_tool","tool":"unknown"}),
        serde_json::json!({"type":"set_backdrop","backdrop":{"paddding":20}}),
        serde_json::json!({"type":"apply_prepared_document","revision":0}),
    ] {
        assert!(
            crate::automation::dispatch(
                &sender,
                "dispatch_action",
                serde_json::json!({"action":action})
            )
            .is_err()
        );
        assert!(receiver.try_recv().is_err());
    }
}

#[gpui::test]
fn menu_undo_operates_on_inline_text_without_touching_document(cx: &mut TestAppContext) {
    let view = cx.add_window(|window, cx| {
        let mut editor = Editor::with_native(cx, false);
        editor.document = Document::new(image::RgbaImage::new(100, 100));
        editor.focus.focus(window);
        editor
    });
    let mut visual = VisualTestContext::from_window(*view, cx);
    view.update(&mut visual, |e, _, _| {
        e.interaction.text_edit = Some(crate::text::Edit::new(crate::document::Mark {
            style: Default::default(),
            tool: Tool::Text,
            points: vec![(10., 10.)],
            curve: None,
            color: [255; 4],
            width: 3.,
            text: String::new(),
        }));
        e.interaction
            .text_edit
            .as_mut()
            .unwrap()
            .replace_text("hello");
    })
    .unwrap();
    visual.run_until_parked();
    visual.dispatch_action(crate::menus::Undo);
    visual.run_until_parked();
    view.update(&mut visual, |e, _, _| {
        assert_eq!(e.interaction.text_edit.as_ref().unwrap().buffer.text(), "");
        assert!(e.document.marks.is_empty());
        assert_eq!(e.preview.revision, 0);
    })
    .unwrap();
}

#[test]
fn sidebar_actions_are_accepted_by_the_mcp_schema() {
    for action in [
        serde_json::json!({"type":"set_appearance","style":{"dash":"dashed","start":"dot","end":"arrow"}}),
        serde_json::json!({"type":"set_magnifier_zoom","zoom":3}),
        serde_json::json!({"type":"set_counter_number","number":7}),
        serde_json::json!({"type":"set_crop_ratio","ratio":null}),
        serde_json::json!({"type":"add_line_point"}),
        serde_json::json!({"type":"toggle_animation_panel"}),
        serde_json::json!({"type":"select_entrance","effect":"diagonal"}),
        serde_json::json!({"type":"set_image_animation","animation":{"effect":"tilt","exit":true}}),
        serde_json::json!({"type":"set_animation_control","control":"duration","value":800}),
        serde_json::json!({"type":"seek_animation","seconds":1.5}),
        serde_json::json!({"type":"replay_animation"}),
        serde_json::json!({"type":"straighten_line"}),
    ] {
        crate::mcp::validate_tool("dispatch_action", &serde_json::json!({"action":action}))
            .unwrap();
        Action::from_json(action).unwrap();
    }
}

#[gpui::test]
fn inside_padding_preserves_capture_and_annotations_and_groups_slider_history(
    cx: &mut TestAppContext,
) {
    use crate::backdrop::{Backdrop, Control};
    use std::sync::Arc;
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        e.dispatch(
            Action::SetBackdrop {
                backdrop: Some(Backdrop::default()),
            },
            cx,
        )
        .unwrap();
        let base = e.document.base.clone();
        let marks = e.document.marks.clone();
        let dimensions = base.dimensions();
        e.dispatch(
            Action::BeginBackdropAdjustment {
                control: Control::InsidePadding,
                track: (0., 0., 200., 24.),
                position: (20., 12.),
            },
            cx,
        )
        .unwrap();
        e.dispatch(
            Action::SetBackdropControl {
                control: Control::InsidePadding,
                value: 48,
            },
            cx,
        )
        .unwrap();
        e.cancel_gesture();
        assert!(Arc::ptr_eq(&base, &e.document.base));
        assert_eq!(e.document.marks, marks);
        assert_eq!(
            e.document.export().dimensions(),
            (dimensions.0 + 224, dimensions.1 + 224)
        );
        let preview = super::preview_base(&e.document);
        let expected_ratio = (dimensions.0 + 96) as f32 / (dimensions.1 + 96) as f32;
        assert!((preview.width() as f32 / preview.height() as f32 - expected_ratio).abs() < 0.002);
        let revision = e.preview.revision;
        e.dispatch(Action::Undo, cx).unwrap();
        assert_eq!(e.document.backdrop.unwrap().inside_padding, 0);
        assert!(e.preview.revision > revision);
        e.receive(
            super::Message::Preview(
                e.preview.revision,
                marks.len(),
                0,
                super::render_image(super::preview_base(&e.document)),
            ),
            cx,
        );
        e.dispatch(Action::Redo, cx).unwrap();
        assert_eq!(e.document.backdrop.unwrap().inside_padding, 48);
        e.receive(
            super::Message::Preview(
                e.preview.revision,
                marks.len(),
                48,
                super::render_image(super::preview_base(&e.document)),
            ),
            cx,
        );
        e.dispatch(Action::ToggleBackdropEnabled, cx).unwrap();
        assert_eq!(e.document.export().dimensions(), dimensions);
        let revision = e.preview.revision;
        // A different slider can re-enable a saved style with inside padding.
        e.dispatch(
            Action::BeginBackdropAdjustment {
                control: Control::Shadow,
                track: (0., 0., 60., 24.),
                position: (24., 12.),
            },
            cx,
        )
        .unwrap();
        e.cancel_gesture();
        assert_eq!(e.document.backdrop.unwrap().inside_padding, 48);
        assert!(e.preview.revision > revision);
        assert!(e.preview.rendering);
        assert!(
            e.dispatch(
                Action::SetBackdropControl {
                    control: Control::InsidePadding,
                    value: 201
                },
                cx
            )
            .is_err()
        );
        assert!(
            e.dispatch(
                Action::SetBackdrop {
                    backdrop: Some(Backdrop {
                        inside_padding: 513,
                        ..Default::default()
                    })
                },
                cx
            )
            .is_err()
        );
    });
}

#[gpui::test]
fn custom_backdrop_colors_use_bridge_revision_checks_and_undo(cx: &mut TestAppContext) {
    use crate::automation::Request;
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        e.document = Document::new(image::RgbaImage::from_pixel(
            4,
            3,
            image::Rgba([41, 82, 123, 255]),
        ));
        let bridge = |e: &mut Editor, cx: &mut gpui::Context<Editor>, payload, revision| {
            let (reply, response) = std::sync::mpsc::channel();
            e.automation(
                Request::Dispatch {
                    action: Action::from_json(payload).unwrap(),
                    expected_revision: Some(revision),
                    reply,
                },
                cx,
            );
            response.recv().unwrap()
        };
        bridge(
            e,
            cx,
            serde_json::json!({"type":"set_backdrop_color","stop":1,"rgb":[1,2,3]}),
            0,
        )
        .unwrap();
        let custom = e.document.backdrop.unwrap();
        assert_eq!(custom.colors, Some([[50, 180, 155], [1, 2, 3]]));
        assert_eq!(e.preview.revision, 1);
        assert!(
            bridge(
                e,
                cx,
                serde_json::json!({"type":"set_backdrop_color","stop":0,"rgb":[9,9,9]}),
                0
            )
            .is_err()
        );
        assert!(
            bridge(
                e,
                cx,
                serde_json::json!({"type":"set_backdrop_color","stop":2,"rgb":[9,9,9]}),
                1
            )
            .is_err()
        );
        assert!(
            bridge(
                e,
                cx,
                serde_json::json!({"type":"sample_backdrop_color","stop":0,"position":[4,0]}),
                1
            )
            .is_err()
        );
        assert_eq!(e.document.backdrop, Some(custom));
        bridge(
            e,
            cx,
            serde_json::json!({"type":"sample_backdrop_color","stop":0,"position":[3,2]}),
            1,
        )
        .unwrap();
        assert_eq!(
            e.document.backdrop.unwrap().colors,
            Some([[41, 82, 123], [1, 2, 3]])
        );
        e.document.undo();
        assert_eq!(e.document.backdrop, Some(custom));
        e.document.redo();
        e.dispatch(
            Action::SelectMotion {
                motion: crate::animation::Motion::Lava,
            },
            cx,
        )
        .unwrap();
        assert_eq!(
            e.document.backdrop.unwrap().colors,
            Some([[41, 82, 123], [1, 2, 3]])
        );
        e.dispatch(Action::SetBackdropFill { gradient: false }, cx)
            .unwrap();
        assert_eq!(
            e.document.backdrop.unwrap().colors,
            Some([[41, 82, 123], [1, 2, 3]])
        );
        e.dispatch(Action::SetBackdropPreset { preset: 3 }, cx)
            .unwrap();
        assert!(e.document.backdrop.unwrap().colors.is_none());
        e.document.undo();
        assert_eq!(
            e.document.backdrop.unwrap().colors,
            Some([[41, 82, 123], [1, 2, 3]])
        );
        let (reply, response) = std::sync::mpsc::channel();
        e.automation(Request::Snapshot(reply), cx);
        let snapshot = response.recv().unwrap().unwrap();
        assert_eq!(
            crate::automation::state(&snapshot)["backdrop"]["colors"],
            serde_json::json!([[41, 82, 123], [1, 2, 3]])
        );
    });
}

#[gpui::test]
fn screen_color_results_require_matching_operation_and_revision(cx: &mut TestAppContext) {
    use super::jobs::{Message, OperationResult};
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        let id = e.start_operation(OperationKind::ColorSample).unwrap();
        let revision = e.preview.revision;
        e.receive(
            Message::Operation(
                id,
                OperationResult::ColorSample {
                    stop: 0,
                    revision,
                    result: Ok(None),
                },
            ),
            cx,
        );
        assert!(e.document.backdrop.is_none());
        assert_eq!(e.preview.revision, revision);
        let stale = e.start_operation(OperationKind::ColorSample).unwrap();
        e.preview.revision += 1;
        e.receive(
            Message::Operation(
                stale,
                OperationResult::ColorSample {
                    stop: 0,
                    revision,
                    result: Ok(Some([10, 20, 30])),
                },
            ),
            cx,
        );
        assert!(e.document.backdrop.is_none());
        let current = e.start_operation(OperationKind::ColorSample).unwrap();
        e.receive(
            Message::Operation(
                stale,
                OperationResult::ColorSample {
                    stop: 0,
                    revision: e.preview.revision,
                    result: Ok(Some([10, 20, 30])),
                },
            ),
            cx,
        );
        assert_eq!(e.operations.active.as_ref().unwrap().id, current);
        e.receive(
            Message::Operation(
                current,
                OperationResult::ColorSample {
                    stop: 1,
                    revision: e.preview.revision,
                    result: Ok(Some([10, 20, 30])),
                },
            ),
            cx,
        );
        assert_eq!(
            e.document.backdrop.unwrap().colors.unwrap()[1],
            [10, 20, 30]
        );
        e.document.undo();
        assert!(e.document.backdrop.is_none());
    });
}

#[gpui::test]
fn color_popup_hex_wheel_and_source_eyedropper_are_isolated_from_tools(cx: &mut TestAppContext) {
    use gpui::{MouseButton, point, px, size};
    let view = cx.add_window(|window, cx| {
        let mut editor = Editor::with_native(cx, false);
        editor.focus.focus(window);
        editor.document = Document::new(image::RgbaImage::from_pixel(
            100,
            100,
            image::Rgba([22, 44, 88, 255]),
        ));
        editor.dispatch(Action::ToggleBackdrop, cx).unwrap();
        editor
    });
    let root = view.root(cx).unwrap();
    let mut visual = VisualTestContext::from_window(*view, cx);
    visual.update(|_, cx| crate::menus::install(cx));
    visual.simulate_resize(size(px(1050.), px(600.)));
    visual.run_until_parked();
    let click = |visual: &mut VisualTestContext, selector| {
        let bounds = visual.debug_bounds(selector).unwrap();
        visual.simulate_click(bounds.center(), Default::default());
        visual.run_until_parked();
    };
    click(&mut visual, "color-picker-color");
    let popup = visual.debug_bounds("color-picker-popup").unwrap();
    assert!(popup.top() >= px(0.) && popup.bottom() <= px(600.));
    click(&mut visual, "color-picker-hex");
    visual.simulate_input("#12abEF");
    visual.simulate_keystrokes("enter");
    visual.run_until_parked();
    root.read_with(&visual, |e, _| {
        assert_eq!(
            e.document.backdrop.unwrap().colors.unwrap()[0],
            [18, 171, 239]
        );
        assert_eq!(e.interaction.tool, Tool::Select);
        assert!(e.document.marks.is_empty());
    });
    let revision = root.read_with(&visual, |e, _| e.preview.revision);
    click(&mut visual, "color-picker-hex");
    visual.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("#abcdef".into())));
    visual.simulate_keystrokes(&crate::platform::key_binding("cmd-v cmd-z enter"));
    root.read_with(&visual, |e, _| {
        assert_eq!(
            e.preview.revision, revision,
            "hex undo stays in the input field"
        )
    });
    click(&mut visual, "color-picker-hex");
    visual.simulate_input("oops");
    visual.simulate_keystrokes("enter");
    root.read_with(&visual, |e, _| {
        assert_eq!(
            e.document.backdrop.unwrap().colors.unwrap()[0],
            [18, 171, 239]
        )
    });
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    root.read_with(&visual, |e, cx| {
        assert!(!e.color_pickers[0].read(cx).is_open())
    });
    click(&mut visual, "color-picker-color");
    let wheel = visual.debug_bounds("color-picker-wheel").unwrap();
    let start = wheel.center();
    let end = point(wheel.right() - px(6.), wheel.center().y);
    let original = root.read_with(&visual, |e, _| e.document.backdrop);
    visual.simulate_mouse_down(start, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    root.read_with(&visual, |e, _| {
        assert_eq!(
            e.document.backdrop, original,
            "drag is committed on release"
        )
    });
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    visual.run_until_parked();
    root.update(&mut visual, |e, cx| {
        assert_ne!(e.document.backdrop, original);
        e.document.undo();
        assert_eq!(e.document.backdrop, original);
        cx.notify();
    });
    visual.run_until_parked();
    let brightness = visual.debug_bounds("color-picker-brightness").unwrap();
    let end = point(brightness.left() - px(80.), brightness.center().y);
    visual.simulate_mouse_down(brightness.center(), MouseButton::Left, Default::default());
    visual.simulate_mouse_move(end, MouseButton::Left, Default::default());
    visual.simulate_mouse_up(end, MouseButton::Left, Default::default());
    visual.run_until_parked();
    root.update(&mut visual, |e, cx| {
        assert_eq!(
            e.document.backdrop.unwrap().colors.unwrap()[0],
            [0, 0, 0],
            "brightness drag clamps outside popup"
        );
        e.document.undo();
        assert_eq!(
            e.document.backdrop, original,
            "one undo restores an entire brightness drag"
        );
        cx.notify();
    });
    assert!(visual.debug_bounds("Pick from image").is_none());
    assert!(visual.debug_bounds("Pick from screen").is_some());
    visual.simulate_keystrokes("escape");
    root.update(&mut visual, |e, cx| {
        e.dispatch(Action::BeginBackdropColorSampling { stop: 0 }, cx)
            .unwrap();
        e.dispatch(Action::Zoom { factor: 2. }, cx).unwrap();
    });
    visual.run_until_parked();
    let layout = root.read_with(&visual, |e, _| e.viewport.layout.get());
    visual.simulate_click(
        point(
            px(layout.x + 25. * layout.scale),
            px(layout.y + 40. * layout.scale),
        ),
        Default::default(),
    );
    visual.run_until_parked();
    root.read_with(&visual, |e, _| {
        assert_eq!(
            e.document.backdrop.unwrap().colors.unwrap()[0],
            [22, 44, 88]
        );
        assert!(e.panels.sampling_color.is_none());
        assert!(e.document.marks.is_empty());
    });
    click(&mut visual, "backdrop-mode-Gradient");
    click(&mut visual, "color-picker-color-2");
    click(&mut visual, "color-picker-hex");
    visual.simulate_input("#f80");
    visual.simulate_keystrokes("enter escape");
    root.read_with(&visual, |e, _| {
        assert_eq!(
            e.document.backdrop.unwrap().colors,
            Some([[22, 44, 88], [255, 136, 0]])
        )
    });
    click(&mut visual, "color-picker-color-2");
    visual.simulate_keystrokes("escape");
    root.update(&mut visual, |e, cx| {
        e.dispatch(Action::BeginBackdropColorSampling { stop: 1 }, cx)
            .unwrap();
    });
    let revision = root.read_with(&visual, |e, _| e.preview.revision);
    visual.simulate_keystrokes("escape");
    root.read_with(&visual, |e, _| {
        assert!(e.panels.sampling_color.is_none());
        assert_eq!(e.preview.revision, revision);
    });
}

#[gpui::test]
fn tool_color_sampling_bridge_checks_revisions_bounds_state_and_undo(cx: &mut TestAppContext) {
    use crate::{
        automation::Request,
        document::{Mark, actions::DocumentAction},
    };
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        e.document = Document::new(image::RgbaImage::from_pixel(
            4,
            3,
            image::Rgba([41, 82, 123, 255]),
        ));
        let bridge = |e: &mut Editor, cx: &mut gpui::Context<Editor>, position, revision| {
            let payload = serde_json::json!({"type":"sample_tool_color","position":position});
            crate::mcp::validate_tool("dispatch_action", &serde_json::json!({"action":payload}))
                .unwrap();
            let (reply, response) = std::sync::mpsc::channel();
            e.automation(
                Request::Dispatch {
                    action: Action::from_json(payload).unwrap(),
                    expected_revision: Some(revision),
                    reply,
                },
                cx,
            );
            response.recv().unwrap()
        };
        for tool in [
            Tool::Pen,
            Tool::Arrow,
            Tool::Rectangle,
            Tool::Text,
            Tool::Highlight,
            Tool::Counter,
            Tool::Spotlight,
            Tool::Magnifier,
        ] {
            e.dispatch(Action::SelectTool { tool }, cx).unwrap();
            e.dispatch(
                Action::SetColor {
                    color: [1, 2, 3, 64],
                },
                cx,
            )
            .unwrap();
            bridge(e, cx, [3, 2], e.preview.revision).unwrap();
            assert_eq!(e.tool_settings().color, [41, 82, 123, 64]);
            assert!(e.document.backdrop.is_none());
        }
        e.dispatch(
            Action::Edit {
                edit: DocumentAction::AddAnnotation {
                    mark: Mark {
                        tool: Tool::Rectangle,
                        points: vec![(0., 0.), (3., 2.)],
                        color: [9, 8, 7, 128],
                        width: 1.,
                        text: String::new(),
                        curve: None,
                        style: Default::default(),
                    },
                },
            },
            cx,
        )
        .unwrap();
        let revision = e.preview.revision;
        assert!(bridge(e, cx, [4, 0], revision).is_err());
        assert!(bridge(e, cx, [0, 0], revision - 1).is_err());
        assert_eq!(e.document.marks[0].color, [9, 8, 7, 128]);
        e.dispatch(Action::BeginToolColorSampling, cx).unwrap();
        let (reply, response) = std::sync::mpsc::channel();
        e.automation(Request::State(reply), cx);
        assert_eq!(
            response.recv().unwrap().unwrap()["sampling_tool_color"],
            true
        );
        bridge(e, cx, [3, 2], revision).unwrap();
        assert_eq!(e.document.marks[0].color, [41, 82, 123, 128]);
        assert_eq!(e.preview.revision, revision + 1);
        assert!(!e.panels.sampling_tool_color);
        e.dispatch(Action::Undo, cx).unwrap();
        assert_eq!(e.document.marks[0].color, [9, 8, 7, 128]);
    });
}

#[gpui::test]
fn tool_screen_color_results_reject_stale_operations_revisions_and_targets(
    cx: &mut TestAppContext,
) {
    use super::jobs::{Message, OperationResult};
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        e.dispatch(Action::SelectTool { tool: Tool::Pen }, cx)
            .unwrap();
        e.dispatch(
            Action::SetColor {
                color: [1, 2, 3, 64],
            },
            cx,
        )
        .unwrap();
        let revision = e.preview.revision;
        let id = e.start_operation(OperationKind::ColorSample).unwrap();
        e.receive(
            Message::Operation(
                id,
                OperationResult::ToolColorSample {
                    tool: Tool::Pen,
                    selected: None,
                    revision: revision + 1,
                    result: Ok(Some([4, 5, 6])),
                },
            ),
            cx,
        );
        assert_eq!(e.tool_settings().color, [1, 2, 3, 64]);
        let current = e.start_operation(OperationKind::ColorSample).unwrap();
        e.receive(
            Message::Operation(
                id,
                OperationResult::ToolColorSample {
                    tool: Tool::Pen,
                    selected: None,
                    revision,
                    result: Ok(Some([4, 5, 6])),
                },
            ),
            cx,
        );
        assert_eq!(e.operations.active.as_ref().unwrap().id, current);
        e.receive(
            Message::Operation(
                current,
                OperationResult::ToolColorSample {
                    tool: Tool::Arrow,
                    selected: None,
                    revision,
                    result: Ok(Some([4, 5, 6])),
                },
            ),
            cx,
        );
        assert_eq!(e.tool_settings().color, [1, 2, 3, 64]);
        let current = e.start_operation(OperationKind::ColorSample).unwrap();
        e.receive(
            Message::Operation(
                current,
                OperationResult::ToolColorSample {
                    tool: Tool::Pen,
                    selected: None,
                    revision,
                    result: Ok(Some([4, 5, 6])),
                },
            ),
            cx,
        );
        assert_eq!(e.tool_settings().color, [4, 5, 6, 64]);
    });
}

#[gpui::test]
fn preview_preparation_holds_playback_and_reports_loading_through_bridge(cx: &mut TestAppContext) {
    use crate::{animation::Entrance, automation::Request};
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::mpsc,
        time::{Duration, Instant},
    };
    let (release, gate) = mpsc::channel();
    let (notify, notifications) = mpsc::channel();
    let preview = Rc::new(RefCell::new(
        crate::animation::CompositionPreview::blocked_for_test(gate, move || {
            let _ = notify.send(());
        }),
    ));
    let view = cx.add_window(|_, cx| {
        let mut editor = Editor::with_native(cx, false);
        editor.document = Document::new(image::RgbaImage::new(2, 2));
        editor.document.image_animation.effect = Entrance::Tilt;
        editor.playback.composition_preview = preview;
        editor.panels.animation = true;
        editor.dispatch(Action::ReplayAnimation, cx).unwrap();
        editor
    });
    let root = view.root(cx).unwrap();
    let mut visual = VisualTestContext::from_window(*view, cx);
    visual.simulate_resize(gpui::size(gpui::px(1050.), gpui::px(600.)));
    visual.run_until_parked();
    root.update(&mut visual, |e, cx| {
        e.playback.epoch = Instant::now() - Duration::from_secs(3);
        assert_eq!(e.clip_time(), 0., "loading must not skip the entrance");
        assert!(e.playback.preparing);
        assert!(
            e.accessibility
                .nodes()
                .iter()
                .any(|node| node.label.ends_with("preparing preview")),
            "the canvas must expose its loading cover"
        );
        let (reply, response) = mpsc::channel();
        e.automation(Request::State(reply), cx);
        let state = response.recv().unwrap().unwrap();
        assert_eq!(state["playback"]["preparing"], true);
        assert_eq!(state["playback"]["time"], 0.);
    });
    release.send(()).unwrap();
    notifications.recv_timeout(Duration::from_secs(3)).unwrap();
    root.update(&mut visual, |_, cx| cx.notify());
    visual.run_until_parked();
    root.update(&mut visual, |e, cx| {
        assert!(!e.playback.preparing);
        assert!(e.clip_time() < 0.5, "the clock must restart after loading");
        e.dispatch(Action::SeekAnimation { seconds: 1.25 }, cx)
            .unwrap();
    });
    visual.run_until_parked();
    root.update(&mut visual, |e, cx| {
        assert!(e.playback.preparing);
        assert!(e.playback.paused);
        assert_eq!(e.clip_time(), 1.25);
        e.dispatch(Action::TogglePlayback, cx).unwrap();
        e.playback.epoch = Instant::now() - Duration::from_secs(3);
        e.dispatch(Action::TogglePlayback, cx).unwrap();
        assert!(e.playback.paused);
        assert_eq!(
            e.clip_time(),
            1.25,
            "pausing while loading preserves the sought time"
        );
    });
    // Allow any obsolete in-flight request and the sought frame to finish.
    drop(release);
    notifications.recv_timeout(Duration::from_secs(3)).unwrap();
    root.update(&mut visual, |_, cx| cx.notify());
    visual.run_until_parked();
    root.update(&mut visual, |e, cx| {
        assert!(!e.playback.preparing);
        assert!(e.playback.paused);
        assert_eq!(e.clip_time(), 1.25);
        e.dispatch(Action::TogglePlayback, cx).unwrap();
        assert!(
            e.playback.preparing,
            "Play must wait for its playback-quality frame"
        );
        e.playback.epoch = Instant::now() - Duration::from_secs(3);
        assert_eq!(e.clip_time(), 1.25);
    });
}

#[gpui::test]
fn multi_selection_bridge_checks_state_revisions_and_group_undo(cx: &mut TestAppContext) {
    use crate::{automation::Request, document::Mark};
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        e.document = Document::new(image::RgbaImage::new(100, 100));
        for x in [10., 40., 75.] {
            e.document.commit(Mark {
                tool: Tool::Rectangle,
                points: vec![(x, 10.), (x + 10., 20.)],
                color: [255, 0, 0, 255],
                width: 2.,
                text: String::new(),
                curve: None,
                style: Default::default(),
            });
        }
        let original = e.document.marks.clone();
        let bridge = |e: &mut Editor, cx: &mut gpui::Context<Editor>, payload, revision| {
            let action = Action::from_json(payload).unwrap();
            let (reply, response) = std::sync::mpsc::channel();
            e.automation(
                Request::Dispatch {
                    action,
                    expected_revision: Some(revision),
                    reply,
                },
                cx,
            );
            response.recv().unwrap()
        };
        let revision = e.preview.revision;
        bridge(e, cx, serde_json::json!({"type":"select_annotations","ids":[format!("{revision}:1")]}), revision).unwrap();
        assert_eq!(e.selected_indices(), vec![1]);
        assert!(bridge(e, cx, serde_json::json!({"type":"select_annotations","ids":[format!("{}:0", revision + 1)]}), revision).is_err());
        assert_eq!(e.selected_indices(), vec![1]);
        bridge(e, cx, serde_json::json!({"type":"select_annotations","ids":[]}), revision).unwrap();
        assert!(
            bridge(
                e,
                cx,
                serde_json::json!({"type":"select_all"}),
                revision + 1
            )
            .is_err()
        );
        assert!(e.selected_indices().is_empty());
        bridge(
            e,
            cx,
            serde_json::json!({"type":"select_region","rectangle":[5,5,50,20]}),
            revision,
        )
        .unwrap();
        assert_eq!(e.selected_indices(), vec![0, 1]);
        assert_eq!(e.preview.revision, revision);
        bridge(
            e,
            cx,
            serde_json::json!({"type":"select_region","rectangle":[70,5,20,20],"additive":true}),
            revision,
        )
        .unwrap();
        assert_eq!(e.selected_indices(), vec![0, 1, 2]);
        assert!(
            bridge(
                e,
                cx,
                serde_json::json!({"type":"select_region","rectangle":[0,0,-1,10]}),
                revision
            )
            .is_err()
        );
        assert_eq!(e.selected_indices(), vec![0, 1, 2]);
        let (reply, response) = std::sync::mpsc::channel();
        e.automation(Request::State(reply), cx);
        let state = response.recv().unwrap().unwrap();
        assert_eq!(state["selected_indices"], serde_json::json!([0, 1, 2]));
        assert_eq!(
            state["selected_ids"],
            serde_json::json!([
                format!("{revision}:0"),
                format!("{revision}:1"),
                format!("{revision}:2")
            ])
        );
        assert_eq!(state["selected"], 2);
        bridge(
            e,
            cx,
            serde_json::json!({"type":"nudge_selection","delta":[3,4],"remember":true}),
            revision,
        )
        .unwrap();
        assert!(bridge(e, cx, serde_json::json!({"type":"delete"}), revision).is_err());
        assert_eq!(e.document.marks[0].points[0], (13., 14.));
        assert_eq!(e.document.marks[2].points[0], (78., 14.));
        e.document.undo();
        assert_eq!(e.document.marks, original);
        bridge(
            e,
            cx,
            serde_json::json!({"type":"set_color","color":[10,20,30,128]}),
            e.preview.revision,
        )
        .unwrap();
        assert!(
            e.document
                .marks
                .iter()
                .all(|m| m.color == [10, 20, 30, 128])
        );
        e.document.undo();
        assert_eq!(e.document.marks, original);
        bridge(
            e,
            cx,
            serde_json::json!({"type":"duplicate_selection"}),
            e.preview.revision,
        )
        .unwrap();
        assert_eq!(e.selected_indices(), vec![3, 4, 5]);
        assert_eq!(e.document.marks.len(), 6);
        e.document.undo();
        assert_eq!(e.document.marks, original);
        bridge(
            e,
            cx,
            serde_json::json!({"type":"select_all"}),
            e.preview.revision,
        )
        .unwrap();
        bridge(
            e,
            cx,
            serde_json::json!({"type":"delete"}),
            e.preview.revision,
        )
        .unwrap();
        assert!(e.selected_indices().is_empty());
        assert!(e.document.marks.is_empty());
        e.document.undo();
        assert_eq!(e.document.marks, original);
    });
}

#[gpui::test]
fn chatgpt_bridge_requires_sign_in_for_remote_ocr_and_preserves_document(cx: &mut TestAppContext) {
    use crate::{automation::Request, chatgpt::OcrEngine};
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        let revision = e.preview.revision;
        let base = e.document.base.clone();
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("Keep this".into()));
        let dispatch = |e: &mut Editor, cx: &mut gpui::Context<Editor>, action| {
            let (reply, response) = std::sync::mpsc::channel();
            e.automation(
                Request::Dispatch {
                    action,
                    expected_revision: Some(revision),
                    reply,
                },
                cx,
            );
            response.recv().unwrap()
        };
        for action in [
            Action::CopyOcr {
                rectangle: None,
                engine: Some(OcrEngine::Chatgpt),
            },
            Action::SetOcrEngine {
                engine: OcrEngine::Chatgpt,
            },
            Action::ChatgptSignIn {
                account_id: Some("unknown".into()),
            },
            Action::SelectChatgptAccount {
                account_id: "unknown".into(),
            },
            Action::SetChatgptModel {
                model: "unknown".into(),
            },
            Action::SignOutChatgpt,
        ] {
            assert!(dispatch(e, cx, action).is_err());
        }
        assert!(e.operations.active.is_none());
        assert!(!e.chatgpt.busy);
        dispatch(e, cx, Action::ToggleChatgptAccountMenu).unwrap();
        let (reply, response) = std::sync::mpsc::channel();
        e.automation(Request::State(reply), cx);
        let state = response.recv().unwrap().unwrap();
        assert_eq!(state["chatgpt"]["menu_open"], true);
        assert_eq!(state["chatgpt"]["account"]["ocr_engine"], "local");
        assert_eq!(e.preview.revision, revision);
        assert!(std::sync::Arc::ptr_eq(&base, &e.document.base));
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "Keep this"
        );
    });
}
#[gpui::test]
fn canceled_and_stale_account_jobs_cannot_open_browser_or_replace_state(cx: &mut TestAppContext) {
    use super::{chatgpt::Outcome, jobs::Message};
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        e.chatgpt.generation = 7;
        e.chatgpt.busy = true;
        e.chatgpt.signing_in = true;
        e.dispatch(Action::CancelChatgptSignIn, cx).unwrap();
        assert!(e.chatgpt.cancel.load(std::sync::atomic::Ordering::Relaxed));
        e.receive(
            Message::ChatgptAuthorization {
                generation: 7,
                url: "https://example.invalid/must-not-open".into(),
            },
            cx,
        );
        e.receive(
            Message::ChatgptFinished {
                generation: 6,
                outcome: Outcome {
                    snapshot: None,
                    status: "Stale".into(),
                },
            },
            cx,
        );
        assert!(e.chatgpt.busy);
        assert_ne!(e.chatgpt.status, "Stale");
        e.receive(
            Message::ChatgptFinished {
                generation: 7,
                outcome: Outcome {
                    snapshot: None,
                    status: "Canceled".into(),
                },
            },
            cx,
        );
        assert!(!e.chatgpt.busy && !e.chatgpt.signing_in);
        assert_eq!(e.chatgpt.status, "Canceled");
        assert_eq!(e.preview.revision, 0);
    });
}
#[gpui::test]
fn account_menu_is_accessible_and_fits_small_windows(cx: &mut TestAppContext) {
    use gpui::{px, size};
    let window = cx.add_window(|_, cx| {
        let mut e = Editor::with_native(cx, false);
        e.chatgpt.menu = true;
        e
    });
    let entity = window.root(cx).unwrap();
    let mut visual = VisualTestContext::from_window(*window, cx);
    visual.simulate_resize(size(px(1050.), px(500.)));
    visual.run_until_parked();
    let bounds = visual.debug_bounds("chatgpt-menu").unwrap();
    assert!(bounds.top() >= px(0.) && bounds.bottom() <= px(500.));
    entity.read_with(&visual, |e, _| {
        for label in [
            "Continue with ChatGPT",
            "Manage usage",
            "This device · Offline",
        ] {
            assert!(
                e.accessibility
                    .nodes()
                    .iter()
                    .any(|node| node.label == label),
                "{label}"
            );
        }
        assert!(e.document.marks.is_empty());
        assert_eq!(e.preview.revision, 0);
    });
}

#[gpui::test]
fn sign_out_cancellation_rejects_queued_success_before_clipboard_write(cx: &mut TestAppContext) {
    use super::jobs::{Message, OperationResult};
    use crate::chatgpt::OcrEngine;
    let entity = cx.new(|cx| Editor::with_native(cx, false));
    entity.update(cx, |e, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("Keep this".into()));
        let id = e.start_operation(OperationKind::Ocr).unwrap();
        e.chatgpt
            .inference_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
        e.receive(
            Message::Operation(
                id,
                OperationResult::ExtractedText {
                    revision: e.preview.revision,
                    rectangle: [0, 0, 100, 100],
                    engine: OcrEngine::Chatgpt,
                    result: Ok("Must not copy".into()),
                },
            ),
            cx,
        );
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "Keep this"
        );
        assert!(e.extraction.is_none());
        assert_eq!(e.feedback.status, "ChatGPT OCR canceled");
    });
}
