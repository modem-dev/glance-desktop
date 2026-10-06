use super::Editor;
use super::actions::Action;
use super::state::{AnnotationDrag, Gesture};
use crate::{
    arrow,
    document::{Mark, Tool, actions::DocumentAction},
    navigation, text,
};
use gpui::*;
impl Editor {
    pub(super) fn outside_text(&mut self, e: &MouseDownEvent, cx: &mut Context<Self>) {
        let layout = self.viewport.layout.get();
        let image = Bounds::new(
            point(px(layout.x), px(layout.y)),
            size(
                px(layout.width * layout.scale),
                px(layout.height * layout.scale),
            ),
        );
        if !image.contains(&e.position) {
            self.dispatch_ui(Action::CommitText, cx);
        }
    }
    pub(super) fn coordinate(&self, p: Point<Pixels>, clamp: bool) -> Option<(f32, f32)> {
        let layout = self.viewport.layout.get();
        if layout.scale <= 0. {
            return None;
        }
        let x = (f32::from(p.x) - layout.x) / layout.scale;
        let y = (f32::from(p.y) - layout.y) / layout.scale;
        if !clamp && (x < 0. || y < 0. || x >= layout.width || y >= layout.height) {
            return None;
        }
        Some((
            x.clamp(0., layout.width - 1.),
            y.clamp(0., layout.height - 1.),
        ))
    }
    pub(super) fn begin(
        &mut self,
        e: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window);
        if self.is_busy() {
            return;
        }
        if self.panels.sampling_tool_color {
            if let Some((x, y)) = self.coordinate(e.position, false) {
                self.dispatch_ui(
                    Action::SampleToolColor {
                        position: (x.floor() as u32, y.floor() as u32),
                    },
                    cx,
                );
            }
            return;
        }
        if let Some(stop) = self.panels.sampling_color {
            if let Some((x, y)) = self.coordinate(e.position, false) {
                self.dispatch_ui(
                    Action::SampleBackdropColor {
                        stop,
                        position: (x.floor() as u32, y.floor() as u32),
                    },
                    cx,
                );
            }
            return;
        }
        // Return to the editing pose before hit-testing annotations.
        if self.panels.animation {
            self.panels.animation = false;
        }
        if self.viewport.space_down {
            self.begin_pan(e.position, cx);
            return;
        }
        if self.viewport.zoom_down {
            self.dispatch_ui(
                Action::ZoomAt {
                    factor: if e.modifiers.shift { 0.5 } else { 2. },
                    anchor: (f32::from(e.position.x), f32::from(e.position.y)),
                },
                cx,
            );
            return;
        }
        if let Some(edit) = &mut self.interaction.text_edit
            && edit
                .bounds
                .is_some_and(|bounds| bounds.dilate(px(5.)).contains(&e.position))
        {
            edit.buffer
                .move_to(edit.index(e.position), e.modifiers.shift);
            edit.selecting = true;
            edit.caret_on = true;
            cx.notify();
            return;
        }
        self.dispatch_ui(Action::CommitText, cx);
        let Some(mut p) = self.coordinate(e.position, false) else {
            return;
        };
        if self.interaction.tool == Tool::Crop {
            p = navigation::endpoint(Tool::Crop, p, p, false, self.viewport.layout.get());
        }
        let selected = self.selected_indices();
        if !e.modifiers.shift || self.interaction.tool != Tool::Select {
            if selected.len() == 1 {
                let index = selected[0];
                let mark = &self.document.marks[index];
                let handle = arrow::handle_at(mark, p, 7. / self.viewport.layout.get().scale);
                if handle.is_some() || mark.hit(p, 5. / self.viewport.layout.get().scale) {
                    self.start_annotation_drag(index, p, handle);
                    cx.notify();
                    return;
                }
            } else if let Some(index) = self.document.pick(p, 5. / self.viewport.layout.get().scale)
                && selected.contains(&index)
            {
                self.start_annotation_drag(index, p, None);
                cx.notify();
                return;
            }
        }
        if self.interaction.tool == Tool::Select {
            let hit = self.document.pick(p, 5. / self.viewport.layout.get().scale);
            if let Some(index) = hit {
                let mut indices = if e.modifiers.shift {
                    selected
                } else {
                    Vec::new()
                };
                if e.modifiers.shift && indices.contains(&index) {
                    indices.retain(|i| *i != index);
                } else {
                    indices.push(index);
                }
                self.dispatch_ui(
                    Action::SelectAnnotations {
                        ids: indices
                            .iter()
                            .map(|i| format!("{}:{i}", self.preview.revision))
                            .collect(),
                    },
                    cx,
                );
                if !e.modifiers.shift {
                    self.start_annotation_drag(index, p, None);
                }
            } else {
                self.interaction.gesture = Gesture::Selecting {
                    origin: p,
                    current: p,
                    additive: e.modifiers.shift,
                    previous: selected,
                };
                if !e.modifiers.shift {
                    self.set_selection(Vec::new());
                }
            }
            cx.notify();
            return;
        }
        self.set_selection(Vec::new());
        let settings = self.interaction.defaults[self.interaction.tool.index()];
        self.interaction.color = settings.color;
        self.interaction.width = settings.width;
        let mut mark = Mark {
            style: settings.style,
            tool: self.interaction.tool,
            curve: None,
            points: vec![p],
            color: self.interaction.color,
            width: settings.width,
            text: if self.interaction.tool == Tool::Magnifier {
                settings.magnification.to_string()
            } else {
                String::new()
            },
        };
        if self.interaction.tool == Tool::Counter {
            mark.text = self.counter_number().to_string();
            self.interaction.next_counter = self
                .interaction
                .next_counter
                .map(|n| n.saturating_add(1).min(999));
            self.dispatch_ui(
                Action::Edit {
                    edit: DocumentAction::AddAnnotation { mark },
                },
                cx,
            );
            cx.notify();
            return;
        }
        if self.interaction.tool == Tool::Text {
            self.interaction.text_edit = Some(text::Edit::new(mark));
            self.interaction.text_session += 1;
            let session = self.interaction.text_session;
            self.feedback.status = "Type your label • Enter to finish • Escape to cancel".into();
            cx.spawn(async move |view, cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(500))
                        .await;
                    let keep = view
                        .update(cx, |editor, cx| {
                            if editor.interaction.text_session != session {
                                return false;
                            }
                            if let Some(edit) = &mut editor.interaction.text_edit {
                                edit.caret_on = !edit.caret_on;
                                cx.notify();
                                true
                            } else {
                                false
                            }
                        })
                        .unwrap_or(false);
                    if !keep {
                        break;
                    }
                }
            })
            .detach();
        } else {
            self.interaction.gesture = Gesture::Drawing(mark);
        }
        cx.notify();
    }
    pub(super) fn motion(&mut self, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.animation_slider_move(e.position, cx) {
            return;
        }
        if self.backdrop_slider_move(e.position, cx) {
            return;
        }
        if let Gesture::Panning(previous) = &mut self.interaction.gesture {
            let delta = (
                f32::from(e.position.x - previous.x),
                f32::from(e.position.y - previous.y),
            );
            *previous = e.position;
            self.dispatch_ui(Action::PanBy { delta }, cx);
            return;
        }
        if let Some(edit) = &mut self.interaction.text_edit
            && edit.selecting
        {
            edit.buffer.move_to(edit.index(e.position), true);
            edit.caret_on = true;
            cx.notify();
            return;
        }
        let Some(p) = self.coordinate(e.position, true) else {
            return;
        };
        if let Gesture::Selecting {
            origin,
            additive,
            previous,
            ..
        } = &self.interaction.gesture
        {
            let (origin, additive, previous) = (*origin, *additive, previous.clone());
            self.set_selection(if additive { previous } else { Vec::new() });
            self.select_region(selection_rectangle(origin, p), additive);
        }
        match &mut self.interaction.gesture {
            Gesture::MovingAnnotation(drag) => drag.update(p, e.modifiers.shift, None),
            Gesture::MovingSelection(drags) => {
                for drag in drags {
                    drag.update(p, e.modifiers.shift, None);
                }
            }
            Gesture::Selecting { current, .. } => {
                *current = p;
            }
            Gesture::EditingArrow { drag, handle } => {
                drag.update(p, e.modifiers.shift, Some(*handle))
            }
            Gesture::Drawing(mark) => {
                if mark.tool == Tool::Pen {
                    if let Some(last) = mark.points.last()
                        && (p.0 - last.0).hypot(p.1 - last.1) * self.viewport.layout.get().scale
                            < 0.35
                    {
                        return;
                    }
                    mark.points.push(p);
                } else {
                    mark.points.truncate(1);
                    mark.points.push(navigation::crop_endpoint(
                        self.interaction.crop_ratio,
                        navigation::endpoint(
                            mark.tool,
                            mark.points[0],
                            p,
                            e.modifiers.shift,
                            self.viewport.layout.get(),
                        ),
                        mark.tool,
                        mark.points[0],
                        e.modifiers.shift,
                        self.viewport.layout.get(),
                    ));
                }
            }
            _ => return,
        }
        cx.notify();
    }
    pub(super) fn finish(&mut self, e: &MouseUpEvent, cx: &mut Context<Self>) {
        if let Some(edit) = &mut self.interaction.text_edit {
            edit.selecting = false;
        }
        let gesture = std::mem::take(&mut self.interaction.gesture);
        let (mut drag, handle) = match gesture {
            Gesture::MovingAnnotation(drag) => (drag, None),
            Gesture::MovingSelection(mut drags) => {
                if let Some(p) = self.coordinate(e.position, true) {
                    for drag in &mut drags {
                        drag.update(p, e.modifiers.shift, None);
                    }
                }
                let updates = drags.into_iter().map(|d| (d.index, d.moved)).collect();
                let revision = self.preview.revision;
                self.dispatch_ui(
                    Action::Edit {
                        edit: DocumentAction::EditAnnotations {
                            updates,
                            additions: Vec::new(),
                            deletions: Vec::new(),
                            remember: true,
                        },
                    },
                    cx,
                );
                if self.preview.revision == revision {
                    self.changed();
                }
                cx.notify();
                return;
            }
            Gesture::Selecting {
                origin,
                current,
                additive,
                previous,
            } => {
                let end = self.coordinate(e.position, true).unwrap_or(current);
                // A click on empty canvas clears selection, without selecting nearby bounds.
                let rectangle = selection_rectangle(origin, end);
                self.set_selection(if additive { previous } else { Vec::new() });
                if rectangle.2 * self.viewport.layout.get().scale >= 3.
                    || rectangle.3 * self.viewport.layout.get().scale >= 3.
                {
                    self.dispatch_ui(
                        Action::SelectRegion {
                            rectangle,
                            additive,
                        },
                        cx,
                    );
                }
                cx.notify();
                return;
            }
            Gesture::EditingArrow { drag, handle } => (drag, Some(handle)),
            Gesture::Drawing(mut mark) => {
                if let Some(p) = self.coordinate(e.position, true) {
                    mark.points.push(navigation::crop_endpoint(
                        self.interaction.crop_ratio,
                        navigation::endpoint(
                            mark.tool,
                            mark.points[0],
                            p,
                            e.modifiers.shift,
                            self.viewport.layout.get(),
                        ),
                        mark.tool,
                        mark.points[0],
                        e.modifiers.shift,
                        self.viewport.layout.get(),
                    ));
                }
                if mark.tool == Tool::Spotlight {
                    let a = mark.points[0];
                    let b = *mark.points.last().unwrap();
                    if (a.0 - b.0).abs() < 2. || (a.1 - b.1).abs() < 2. {
                        cx.notify();
                        return;
                    }
                }
                if mark.tool != Tool::Pen && mark.points.len() > 2 {
                    let end = *mark.points.last().unwrap();
                    mark.points.truncate(1);
                    mark.points.push(end);
                }
                let edit = if mark.tool == Tool::Crop {
                    let first = mark.points[0];
                    let last = *mark.points.last().unwrap();
                    let rectangle = (
                        first.0.min(last.0),
                        first.1.min(last.1),
                        (first.0 - last.0).abs(),
                        (first.1 - last.1).abs(),
                    );
                    if rectangle.2 < 2. || rectangle.3 < 2. {
                        cx.notify();
                        return;
                    }
                    DocumentAction::Crop { rectangle }
                } else {
                    DocumentAction::AddAnnotation { mark }
                };
                self.dispatch_ui(Action::Edit { edit }, cx);
                cx.notify();
                return;
            }
            Gesture::Idle => return,
            Gesture::Panning(_)
            | Gesture::AdjustingBackdrop(..)
            | Gesture::AdjustingAnimation(..) => {
                cx.notify();
                return;
            }
        };
        if let Some(p) = self.coordinate(e.position, true) {
            drag.update(p, e.modifiers.shift, handle);
        }
        if let Some(original) = self.document.marks.get(drag.index)
            && (drag.moved.points != original.points || drag.moved.curve != original.curve)
        {
            self.dispatch_ui(
                Action::Edit {
                    edit: DocumentAction::UpdateAnnotation {
                        index: drag.index,
                        mark: drag.moved,
                    },
                },
                cx,
            );
        } else {
            // Restore the complete preview even when a selection click did not move.
            self.changed();
        }
        cx.notify();
    }
    pub(super) fn start_annotation_drag(
        &mut self,
        index: usize,
        origin: (f32, f32),
        handle: Option<usize>,
    ) {
        if handle.is_none() && self.selected_indices().len() > 1 {
            let drags = self
                .selected_indices()
                .into_iter()
                .map(|index| {
                    let original = self.document.marks[index].clone();
                    AnnotationDrag {
                        index,
                        origin,
                        moved: original.clone(),
                        original,
                    }
                })
                .collect();
            self.interaction.gesture = Gesture::MovingSelection(drags);
            self.changed();
            return;
        }
        let original = self.document.marks[index].clone();
        let drag = AnnotationDrag {
            index,
            origin,
            moved: original.clone(),
            original,
        };
        self.interaction.gesture = match handle {
            Some(handle) => Gesture::EditingArrow { drag, handle },
            None => Gesture::MovingAnnotation(drag),
        };
        self.changed();
    }
    pub(super) fn begin_pan(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        self.cancel_gesture();
        self.interaction.gesture = Gesture::Panning(position);
        cx.notify();
    }
    pub(super) fn end_pan(&mut self) {
        if matches!(self.interaction.gesture, Gesture::Panning(_)) {
            self.interaction.gesture = Gesture::Idle;
        }
    }
    pub(super) fn key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.tool_color_picker.read(cx).has_focus(window)
            || self
                .number_inputs
                .values()
                .any(|input| input.read(cx).has_focus(window))
            || self
                .color_pickers
                .iter()
                .any(|picker| picker.read(cx).has_focus(window))
        {
            return;
        }
        let key = e.keystroke.key.as_str();
        if self.chatgpt_key(key, cx) || self.popup_key(key, cx) {
            cx.stop_propagation();
            return;
        }
        if key == "escape" && self.video_export.cancel.is_some() {
            self.dispatch_ui(Action::CancelExport, cx);
            cx.stop_propagation();
            return;
        }
        let m = e.keystroke.modifiers;
        let command = crate::platform::command_pressed(m);
        if self.interaction.text_edit.is_some() {
            if matches!(key, "enter" | "escape")
                && self
                    .interaction
                    .text_edit
                    .as_ref()
                    .is_some_and(|e| e.buffer.marked.is_some())
            {
                return;
            }
            match key {
                "enter" => {
                    self.dispatch_ui(Action::CommitText, cx);
                    cx.stop_propagation();
                    return;
                }
                "escape" => {
                    self.dispatch_ui(Action::Cancel, cx);
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
            if command && (matches!(key, "s" | "o" | "q") || (m.shift && key == "c")) {
                self.dispatch_ui(Action::CommitText, cx);
            } else {
                let text_action = if command {
                    match key {
                        "a" => Some(Action::SelectAll),
                        "c" => Some(Action::Copy),
                        "x" => Some(Action::Cut),
                        "v" => Some(Action::Paste),
                        "z" => Some(if m.shift { Action::Redo } else { Action::Undo }),
                        _ => None,
                    }
                } else if key == "backspace" {
                    Some(Action::Delete)
                } else {
                    None
                };
                if let Some(action) = text_action {
                    self.dispatch_ui(action, cx);
                    cx.stop_propagation();
                    return;
                }
                let edit = self.interaction.text_edit.as_mut().unwrap();
                let mut handled = true;
                if command {
                    match key {
                        "left" => edit.buffer.move_to(0, m.shift),
                        "right" => edit.buffer.move_to(edit.buffer.text().len(), m.shift),
                        _ => handled = false,
                    }
                } else {
                    match key {
                        "delete" => edit.buffer.delete(true),
                        "left" => edit.buffer.move_cursor(false, m.shift),
                        "right" => edit.buffer.move_cursor(true, m.shift),
                        "home" => edit.buffer.move_to(0, m.shift),
                        "end" => edit.buffer.move_to(edit.buffer.text().len(), m.shift),
                        _ => handled = false,
                    }
                }
                if handled {
                    edit.caret_on = true;
                    cx.stop_propagation();
                    cx.notify();
                }
                let _ = window;
                return;
            }
        }
        if !command && !m.alt && !m.control {
            if key == "space" {
                self.viewport.space_down = true;
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if key == "z" {
                self.viewport.zoom_down = true;
                cx.notify();
                cx.stop_propagation();
                return;
            }
            if matches!(key, "left" | "right" | "up" | "down")
                && self.interaction.selected.is_some()
                && !self.is_busy()
            {
                let step = if m.shift { 10. } else { 1. };
                let delta = match key {
                    "left" => (-step, 0.),
                    "right" => (step, 0.),
                    "up" => (0., -step),
                    _ => (0., step),
                };
                self.dispatch_ui(
                    Action::NudgeSelection {
                        delta,
                        remember: !e.is_held,
                    },
                    cx,
                );
                cx.stop_propagation();
                return;
            }
        }
        let action = if command && m.alt && matches!(key, "2" | "3") {
            Some(Action::Capture { area: key == "2" })
        } else if command {
            match key {
                "a" => Some(Action::SelectAll),
                "q" => Some(Action::Quit),
                "c" if m.shift => Some(Action::CopyRemote),
                "c" => Some(Action::Copy),
                "s" => Some(Action::SaveImage),
                "o" => Some(Action::OpenImage),
                "v" => Some(Action::Paste),
                "d" => Some(Action::DuplicateSelection),
                "z" => Some(if m.shift { Action::Redo } else { Action::Undo }),
                "1" => Some(Action::Fit),
                "0" => Some(Action::ActualSize),
                "+" | "=" => Some(Action::Zoom { factor: 1.25 }),
                "-" => Some(Action::Zoom { factor: 0.8 }),
                _ => None,
            }
        } else if !m.alt && !m.control {
            match key {
                "v" => Some(Action::SelectTool { tool: Tool::Select }),
                "backspace" | "delete" => Some(Action::Delete),
                "p" => Some(Action::SelectTool { tool: Tool::Pen }),
                "a" => Some(Action::SelectTool { tool: Tool::Arrow }),
                "r" => Some(Action::SelectTool {
                    tool: Tool::Rectangle,
                }),
                "h" => Some(Action::SelectTool {
                    tool: Tool::Highlight,
                }),
                "b" => Some(Action::SelectTool {
                    tool: Tool::Pixelate,
                }),
                "x" => Some(Action::SelectTool { tool: Tool::Crop }),
                "t" => Some(Action::SelectTool { tool: Tool::Text }),
                "n" => Some(Action::SelectTool {
                    tool: Tool::Counter,
                }),
                "s" => Some(Action::SelectTool {
                    tool: Tool::Spotlight,
                }),
                "m" => Some(Action::SelectTool {
                    tool: Tool::Magnifier,
                }),
                "escape" => Some(Action::Cancel),
                _ => None,
            }
        } else {
            None
        };
        if let Some(action) = action {
            self.dispatch_ui(action, cx);
        }
        cx.stop_propagation();
    }
    pub(super) fn zoom_at(&mut self, factor: f32, anchor: (f32, f32), cx: &mut Context<Self>) {
        if self.interaction.gesture.is_active() {
            return;
        }
        let bounds = self.viewport.canvas_bounds.get();
        let center = (
            f32::from(bounds.origin.x + bounds.size.width * 0.5),
            f32::from(bounds.origin.y + bounds.size.height * 0.5),
        );
        if let Some((zoom, pan)) = navigation::anchored_zoom(
            self.viewport
                .zoom
                .unwrap_or(self.viewport.layout.get().scale),
            self.viewport.pan,
            center,
            anchor,
            factor,
        ) {
            self.viewport.zoom = Some(zoom);
            self.viewport.pan = pan;
            cx.notify();
        }
    }
    pub(super) fn change_zoom(&mut self, factor: f32, cx: &mut Context<Self>) {
        let b = self.viewport.canvas_bounds.get();
        self.zoom_at(
            factor,
            (
                f32::from(b.origin.x + b.size.width * 0.5),
                f32::from(b.origin.y + b.size.height * 0.5),
            ),
            cx,
        );
    }
    pub(super) fn scroll(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        if self.interaction.gesture.is_active()
            || !self.viewport.canvas_bounds.get().contains(&e.position)
        {
            return;
        }
        let delta = e.delta.pixel_delta(px(24.));
        let action = if crate::platform::command_pressed(e.modifiers) {
            Action::ZoomAt {
                factor: (f32::from(delta.y) * 0.008).exp(),
                anchor: (f32::from(e.position.x), f32::from(e.position.y)),
            }
        } else {
            Action::PanBy {
                delta: if e.modifiers.shift && f32::from(delta.x).abs() < 0.01 {
                    (f32::from(delta.y), 0.)
                } else {
                    (f32::from(delta.x), f32::from(delta.y))
                },
            }
        };
        self.dispatch_ui(action, cx);
        cx.stop_propagation();
    }
    pub(super) fn backdrop_slider_move(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Gesture::AdjustingBackdrop(control, bounds) = self.interaction.gesture else {
            return false;
        };
        if self.document.backdrop.is_some() && f32::from(bounds.size.width) > 0. {
            let ratio = f32::from(position.x - bounds.left()) / f32::from(bounds.size.width);
            let value = control.min()
                + (ratio.clamp(0., 1.) * (control.max() - control.min()) as f32).round() as u32;
            self.dispatch_ui(Action::SetBackdropControl { control, value }, cx);
        }
        true
    }
}

fn selection_rectangle(a: (f32, f32), b: (f32, f32)) -> (f32, f32, f32, f32) {
    (
        a.0.min(b.0),
        a.1.min(b.1),
        (a.0 - b.0).abs(),
        (a.1 - b.1).abs(),
    )
}
